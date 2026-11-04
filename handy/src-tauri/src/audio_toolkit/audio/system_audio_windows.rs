//! Windows system-audio (loopback) capture producer (IF-W01, WIN-CAP-001 + WIN-CAP-003).
//!
//! Captures everything Windows plays, whatever it plays it through, with WASAPI shared-mode
//! **loopback** streams, and feeds the sum into the same `run_consumer` pipeline as the
//! microphone `AudioRecorder` and the macOS Process Tap. Everything downstream (chunk sink to
//! the PCM writer, mix, finalize, diarization) is shared and untouched.
//!
//! Why *every* active render endpoint and not "the default device" (WIN-CAP-003): a call does
//! not necessarily play through the default device. Bluetooth earphones expose a second
//! "Hands-Free" endpoint that Teams and Meet switch to for calls, Windows keeps a separate
//! *communications* default, and plugging earphones in mid-meeting moves the audio to a device
//! that did not exist when the session started. So the recorder:
//!
//! * opens one loopback per active render endpoint (`EnumAudioEndpoints(eRender, ACTIVE)`),
//!   each on its own thread with its own COM apartment, converted to mono and resampled to
//!   16 kHz right there (endpoints run at different rates: 48 kHz speakers, 16 kHz hands-free);
//! * rescans the endpoint list every two seconds, starting captures for new devices and
//!   retiring ones that vanished or whose stream was invalidated;
//! * mixes the per-endpoint streams on a master clock and hands 16 kHz mono chunks to the
//!   consumer. Only one endpoint carries the call at any moment, so the sum is the call.
//!
//! Two Windows facts shape the per-endpoint loop:
//!
//! 1. **Silence stops the clock.** A loopback stream only delivers packets while something
//!    renders to that endpoint; when it goes quiet WASAPI delivers nothing and may not signal
//!    the event either. Each endpoint thread keeps a monotonic clock and inserts zero frames
//!    whenever its emitted count falls behind wall time (`gap_fill_frames`), so every stream,
//!    and therefore the mix, keeps wall-clock length and stays aligned with the mic track (R1).
//! 2. **COM lives on one thread.** Every COM object is created, used and released on the thread
//!    that owns it. `SystemAudioRecorder` holds only channels and join handles, so it is
//!    trivially `Send`, which `SessionManager` requires.
//!
//! All `unsafe` FFI is confined to this file behind the safe `SystemAudioRecorder` API.
#![cfg(all(target_os = "windows", target_arch = "x86_64"))]
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

use windows::core::{GUID, PCWSTR};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE_ACTIVE,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED, STGM_READ,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use super::recorder::{run_consumer, AudioChunk, Cmd, FrameSinks, VadPolicy};
use super::resampler::FrameResampler;

// ---------------------------------------------------------------------------------------------
// Pure helpers (unit-tested without COM)
// ---------------------------------------------------------------------------------------------

/// `WAVEFORMATEX.wFormatTag` values and the two `WAVEFORMATEXTENSIBLE` sub-formats a shared-mode
/// mix format can carry. Spelled out here (they are ABI constants, not configuration) rather than
/// pulling in the `Win32_Media_KernelStreaming` + `Win32_Media_Multimedia` feature sets of the
/// `windows` crate for three numbers and two GUIDs.
const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;
const KSDATAFORMAT_SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
const KSDATAFORMAT_SUBTYPE_IEEE_FLOAT: GUID =
    GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

/// How one sample is laid out in an endpoint's mix format. Shared-mode Windows mixes in 32-bit
/// float practically everywhere, but the API allows integer PCM and the spec asks for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SampleFormat {
    F32,
    I16,
    /// Packed 24-bit little-endian integer (3 bytes per sample).
    I24,
    /// 32-bit container; `wValidBitsPerSample` may be lower (e.g. 24 in 32) but the valid bits
    /// are MSB-aligned, so an i32 -> f32 scale by 2^31 is right either way.
    I32,
}

/// The facts about a mix format the capture loop needs, extracted once per endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct StreamFormat {
    pub sample_rate: u32,
    pub channels: usize,
    pub format: SampleFormat,
    /// Bytes per frame (`nBlockAlign`): `channels * container bytes`.
    pub block_align: usize,
}

/// Classify a `WAVEFORMATEX`(`EXTENSIBLE`) header into a [`StreamFormat`], or say exactly why it
/// cannot be captured. Pure so every branch is unit-tested with synthetic headers.
pub(crate) fn classify_format(
    format_tag: u16,
    sub_format: Option<GUID>,
    bits_per_sample: u16,
    channels: u16,
    sample_rate: u32,
    block_align: u16,
) -> Result<StreamFormat, String> {
    if channels == 0 || sample_rate == 0 {
        return Err(format!(
            "mix format reports {channels} channel(s) at {sample_rate} Hz"
        ));
    }
    let is_float = match (format_tag, sub_format) {
        (WAVE_FORMAT_IEEE_FLOAT, _) => true,
        (WAVE_FORMAT_PCM, _) => false,
        (WAVE_FORMAT_EXTENSIBLE, Some(sub)) if sub == KSDATAFORMAT_SUBTYPE_IEEE_FLOAT => true,
        (WAVE_FORMAT_EXTENSIBLE, Some(sub)) if sub == KSDATAFORMAT_SUBTYPE_PCM => false,
        (WAVE_FORMAT_EXTENSIBLE, Some(sub)) => {
            return Err(format!("unsupported WAVE_FORMAT_EXTENSIBLE sub-format {sub:?}"))
        }
        (WAVE_FORMAT_EXTENSIBLE, None) => {
            return Err("WAVE_FORMAT_EXTENSIBLE header without a sub-format".into())
        }
        (tag, _) => return Err(format!("unsupported wFormatTag {tag:#06x}")),
    };
    let format = match (is_float, bits_per_sample) {
        (true, 32) => SampleFormat::F32,
        (true, bits) => return Err(format!("float mix format with {bits} bits per sample")),
        (false, 16) => SampleFormat::I16,
        (false, 24) => SampleFormat::I24,
        (false, 32) => SampleFormat::I32,
        (false, bits) => return Err(format!("PCM mix format with {bits} bits per sample")),
    };
    let expected_align = channels as usize * (bits_per_sample as usize / 8);
    if block_align as usize != expected_align {
        return Err(format!(
            "nBlockAlign {block_align} does not match {channels} ch x {bits_per_sample} bit"
        ));
    }
    Ok(StreamFormat {
        sample_rate,
        channels: channels as usize,
        format,
        block_align: block_align as usize,
    })
}

/// Decode one interleaved frame-aligned byte buffer in `fmt` to mono f32 by averaging the
/// channels, appending to `out`. Trailing bytes that do not make a whole frame are ignored
/// (WASAPI hands whole frames; this is defensive). Pure, so the four sample formats and the
/// downmix are unit-tested with synthetic buffers.
pub(crate) fn bytes_to_mono_f32(bytes: &[u8], fmt: &StreamFormat, out: &mut Vec<f32>) {
    let ch = fmt.channels.max(1);
    let sample_bytes = fmt.block_align / ch;
    if sample_bytes == 0 {
        return;
    }
    out.reserve(bytes.len() / fmt.block_align);
    for frame in bytes.chunks_exact(fmt.block_align) {
        let mut acc = 0.0f32;
        for s in frame.chunks_exact(sample_bytes) {
            acc += match fmt.format {
                SampleFormat::F32 => f32::from_le_bytes([s[0], s[1], s[2], s[3]]),
                SampleFormat::I16 => i16::from_le_bytes([s[0], s[1]]) as f32 / 32768.0,
                SampleFormat::I24 => {
                    // Sign-extend the 24-bit value by placing it in the top of an i32.
                    (i32::from_le_bytes([0, s[0], s[1], s[2]]) >> 8) as f32 / 8_388_608.0
                }
                SampleFormat::I32 => {
                    i32::from_le_bytes([s[0], s[1], s[2], s[3]]) as f32 / 2_147_483_648.0
                }
            };
        }
        out.push(acc / ch as f32);
    }
}

/// Clock discipline for one loopback stream (R1). `expected` is how many frames wall time says
/// should have been emitted since the stream started; `emitted` is how many were (real plus
/// zero). Returns the number of zero frames to insert *now*:
///
/// * `0` while the deficit is below `min_gap` — real packets arrive in bursts up to one buffer
///   period late, and filling inside that window would interleave zeros with real audio;
/// * the whole deficit once it exceeds `min_gap` — the renderer went quiet, catch up to the
///   clock so the stream keeps wall-clock length;
/// * `0` when emitted is ahead of the clock (the device clock runs slightly fast): frames are
///   never dropped, a few ms of lead is harmless and self-corrects at the next silence.
pub(crate) fn gap_fill_frames(expected: u64, emitted: u64, min_gap: u64) -> u64 {
    let deficit = expected.saturating_sub(emitted);
    if deficit >= min_gap {
        deficit
    } else {
        0
    }
}

/// Mix the next `n` samples of every endpoint stream into one chunk. Each queue contributes
/// what it has (up to `n`) and is zero-padded for the rest, so a dead or late endpoint never
/// stalls the mix; the sum is clamped to [-1, 1]. A queue that has piled up more than
/// `max_backlog` samples beyond what was taken is trimmed from the front: that endpoint has
/// drifted ahead of the master clock (or its thread was starved and then caught up at once) and
/// keeping the excess would delay it forever. Pure, unit-tested.
pub(crate) fn mix_next(queues: &mut [VecDeque<f32>], n: usize, max_backlog: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; n];
    for q in queues.iter_mut() {
        let take = n.min(q.len());
        for (o, s) in out.iter_mut().zip(q.drain(..take)) {
            *o += s;
        }
        if q.len() > max_backlog {
            let excess = q.len() - max_backlog;
            q.drain(..excess);
        }
    }
    for o in &mut out {
        *o = o.clamp(-1.0, 1.0);
    }
    out
}

/// The rate every endpoint stream is converted to before mixing, and the rate the consumer is
/// told. Matches the pipeline's native 16 kHz so `run_consumer`'s resampler is a pass-through.
const MIX_RATE: u32 = 16_000;

/// How far behind wall clock the mixer runs. Endpoint streams arrive late by one WASAPI period
/// (10 ms), one resampler chunk (1024 input samples: 21 ms at 48 kHz, none at 16 kHz), one 30 ms
/// resampler frame and up to one tick (20 ms): about 80 ms for the usual endpoints. The mixer
/// waits this long so a healthy endpoint always has the samples it asks for; the price is that
/// the system track trails the mic track by about this much (the consumer starts taking chunks
/// at `Cmd::Start`, and the chunks are this old). An endpoint slower than this (an 8 kHz
/// narrowband hands-free profile resamples in 128 ms chunks) is padded with zeros once and then
/// runs that much later; its audio is still complete. Kept at 100 ms as the balance between
/// those two costs; measured mic-vs-system offset is in the WIN-CAP-003 worklog.
const MIX_LATENCY: Duration = Duration::from_millis(100);

/// Per-endpoint backlog tolerated beyond the mixer's demand before trimming (see [`mix_next`]).
const MAX_BACKLOG: Duration = Duration::from_secs(1);

/// How often the coordinator re-enumerates active render endpoints (earphones plugged in,
/// Bluetooth hands-free profile activated by a call, a device unplugged).
const RESCAN_EVERY: Duration = Duration::from_secs(2);

/// Deficit beyond which a clock is re-based instead of filled: the machine slept, or a thread
/// was starved for a very long time. Filling minutes of zeros would make the system track
/// *longer* than the mic track (whose cpal stream simply stopped), so re-sync and log instead.
const RESYNC_AFTER: Duration = Duration::from_secs(10);

/// Below this deficit an endpoint loop does not fill (see [`gap_fill_frames`]). Two default
/// WASAPI periods (10 ms each) plus margin.
const MIN_GAP: Duration = Duration::from_millis(50);

/// How long an endpoint loop waits for its WASAPI event before running a clock check anyway,
/// and the mixer's tick.
const TICK: Duration = Duration::from_millis(20);

/// Shared-mode buffer each endpoint keeps for us, in 100 ns units (200 ms). Generous so a busy
/// UI thread cannot overflow it; latency is irrelevant for a recording.
const BUFFER_DURATION_HNS: i64 = 2_000_000;

// ---------------------------------------------------------------------------------------------
// The recorder
// ---------------------------------------------------------------------------------------------

pub struct SystemAudioRecorder {
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    /// The shared `run_consumer` worker (resample + sinks), same as the mic path.
    worker_handle: Option<std::thread::JoinHandle<()>>,
    /// The coordinator thread: enumerates endpoints, owns the per-endpoint capture threads,
    /// mixes their streams on the master clock.
    coordinator_handle: Option<std::thread::JoinHandle<()>>,
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
    chunk_sink: Option<mpsc::Sender<Vec<f32>>>,
    /// Shared with `run_consumer`: true between `Cmd::Stop` and the next `Cmd::Start`, when the
    /// producer must send `EndOfStream` once and otherwise stay quiet.
    stop_flag: Arc<AtomicBool>,
    /// Tells the coordinator (and through it every endpoint thread) to tear down. `close()` only.
    quit: Arc<AtomicBool>,
}

impl SystemAudioRecorder {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(SystemAudioRecorder {
            cmd_tx: None,
            worker_handle: None,
            coordinator_handle: None,
            level_cb: None,
            chunk_sink: None,
            stop_flag: Arc::new(AtomicBool::new(false)),
            quit: Arc::new(AtomicBool::new(false)),
        })
    }

    pub fn with_level_callback<F>(mut self, cb: F) -> Self
    where
        F: Fn(Vec<f32>) + Send + Sync + 'static,
    {
        self.level_cb = Some(Arc::new(cb));
        self
    }

    pub fn with_chunk_sink(mut self, sink: mpsc::Sender<Vec<f32>>) -> Self {
        self.chunk_sink = Some(sink);
        self
    }

    /// Start the coordinator (which opens a loopback on every active render endpoint) and the
    /// consumer. Fails only when WASAPI itself cannot be reached (COM or the device enumerator);
    /// a machine with no active output device opens fine and records silence, so the mic track
    /// of the meeting is never lost over the system track. Idempotent while open.
    pub fn open(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            return Ok(()); // already open
        }

        let (sample_tx, sample_rx) = mpsc::channel::<AudioChunk>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (init_tx, init_rx) = mpsc::channel::<Result<(), String>>();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let quit = Arc::new(AtomicBool::new(false));
        self.stop_flag = stop_flag.clone();
        self.quit = quit.clone();

        let coordinator = std::thread::Builder::new()
            .name("plaudy-wasapi-mixer".into())
            .spawn({
                let stop_flag = stop_flag.clone();
                let quit = quit.clone();
                move || coordinator_thread(init_tx, sample_tx, stop_flag, quit)
            })?;

        match init_rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(msg)) => {
                let _ = coordinator.join();
                return Err(msg.into());
            }
            Err(_) => {
                let _ = coordinator.join();
                return Err("WASAPI coordinator thread died during initialisation".into());
            }
        }

        let level_cb = self.level_cb.clone();
        let chunk_sink = self.chunk_sink.clone();
        let worker = std::thread::spawn({
            let stop_flag = stop_flag.clone();
            move || {
                run_consumer(
                    MIX_RATE,
                    FrameSinks::new(None, None, chunk_sink),
                    sample_rx,
                    cmd_rx,
                    level_cb,
                    stop_flag,
                    Instant::now(),
                );
            }
        });

        self.cmd_tx = Some(cmd_tx);
        self.worker_handle = Some(worker);
        self.coordinator_handle = Some(coordinator);
        Ok(())
    }

    pub fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Start(
                VadPolicy::Disabled,
                Instant::now(),
                mpsc::channel().0,
            ))?;
        }
        Ok(())
    }

    /// Same handshake as the cpal and CoreAudio paths: `Cmd::Stop` raises the shared stop flag,
    /// the coordinator sees it and injects `EndOfStream` once, and `run_consumer`'s drain loop
    /// breaks on the sentinel and replies with the drained samples (an empty Vec when a chunk
    /// sink took them, exactly like macOS).
    pub fn stop(&self) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let (resp_tx, resp_rx) = mpsc::channel();
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Stop(resp_tx))?;
        }
        Ok(resp_rx.recv_timeout(super::recorder::STOP_REPLY_TIMEOUT)?)
    }

    /// Tear down every capture thread and the consumer. Safe to call twice and from `Drop`.
    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        self.stop_flag.store(true, Ordering::Relaxed);
        self.quit.store(true, Ordering::Relaxed);

        // The coordinator owns sample_tx; joining it first closes the channel so the consumer
        // cannot block on a producer that is gone.
        if let Some(h) = self.coordinator_handle.take() {
            let _ = h.join();
        }
        if let Some(h) = self.worker_handle.take() {
            let _ = h.join();
        }
        Ok(())
    }
}

impl Drop for SystemAudioRecorder {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

// ---------------------------------------------------------------------------------------------
// Coordinator: endpoint discovery + mixing
// ---------------------------------------------------------------------------------------------

/// One active render endpoint as the enumerator reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct EndpointInfo {
    /// The MMDevice id string (stable for the device's lifetime; what `GetDevice` takes).
    id: String,
    /// Friendly name for the log ("Headset (WH-1000XM4 Hands-Free)"), or the id if unreadable.
    name: String,
}

/// A running per-endpoint capture, as the coordinator sees it.
struct EndpointWorker {
    info: EndpointInfo,
    rx: mpsc::Receiver<Vec<f32>>,
    quit: Arc<AtomicBool>,
    /// Set by the endpoint thread when its stream died (device invalidated, capture error).
    dead: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
    /// 16 kHz mono samples received and not yet mixed.
    pending: VecDeque<f32>,
}

impl EndpointWorker {
    fn spawn(info: EndpointInfo) -> Self {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let quit = Arc::new(AtomicBool::new(false));
        let dead = Arc::new(AtomicBool::new(false));
        let handle = std::thread::Builder::new()
            .name(format!("plaudy-loopback:{}", info.name))
            .spawn({
                let info = info.clone();
                let quit = quit.clone();
                let dead = dead.clone();
                move || endpoint_thread(info, tx, quit, dead)
            })
            .ok();
        if handle.is_none() {
            dead.store(true, Ordering::Relaxed);
        }
        EndpointWorker {
            info,
            rx,
            quit,
            dead,
            handle,
            pending: VecDeque::new(),
        }
    }

    fn retire(mut self) {
        self.quit.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// Body of the coordinator thread.
fn coordinator_thread(
    init_tx: mpsc::Sender<Result<(), String>>,
    sample_tx: mpsc::Sender<AudioChunk>,
    stop_flag: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
) {
    // SAFETY: a fresh thread; MTA is the right apartment for an audio worker.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if com.is_err() {
        let _ = init_tx.send(Err(format!("CoInitializeEx failed: {com}")));
        return;
    }
    let enumerator: IMMDeviceEnumerator =
        match unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) } {
            Ok(e) => e,
            Err(e) => {
                let _ = init_tx.send(Err(format!(
                    "CoCreateInstance(MMDeviceEnumerator) failed: {e}"
                )));
                unsafe { CoUninitialize() };
                return;
            }
        };

    let mut workers: Vec<EndpointWorker> = Vec::new();
    let initial = unsafe { active_render_endpoints(&enumerator) };
    if initial.is_empty() {
        log::warn!("WASAPI loopback: no active render endpoint; the system track will be silent until one appears");
    }
    for info in initial {
        log::info!("WASAPI loopback: capturing \"{}\"", info.name);
        workers.push(EndpointWorker::spawn(info));
    }
    let _ = init_tx.send(Ok(()));

    let rate = MIX_RATE as f64;
    let max_backlog = (MAX_BACKLOG.as_secs_f64() * rate) as usize;
    let max_tail = (2.0 * MIX_LATENCY.as_secs_f64() * rate) as usize;
    let resync_after = (RESYNC_AFTER.as_secs_f64() * rate) as u64;
    let mut started = Instant::now();
    let mut emitted: u64 = 0;
    let mut last_scan = Instant::now();
    let mut eos_sent = false;

    while !quit.load(Ordering::Relaxed) {
        std::thread::sleep(TICK);

        // Follow the device list: new endpoints start capturing, vanished or dead ones retire
        // (a dead one that is still active is re-spawned here as if new, which re-opens it).
        if last_scan.elapsed() >= RESCAN_EVERY {
            last_scan = Instant::now();
            let active = unsafe { active_render_endpoints(&enumerator) };
            let mut keep: Vec<EndpointWorker> = Vec::with_capacity(workers.len());
            for w in workers.drain(..) {
                let still_active = active.iter().any(|a| a.id == w.info.id);
                if still_active && !w.dead.load(Ordering::Relaxed) {
                    keep.push(w);
                } else {
                    log::info!(
                        "WASAPI loopback: retiring \"{}\" ({})",
                        w.info.name,
                        if still_active { "stream died" } else { "endpoint gone" }
                    );
                    w.retire();
                }
            }
            for info in active {
                if !keep.iter().any(|w| w.info.id == info.id) {
                    log::info!("WASAPI loopback: capturing \"{}\" (new endpoint)", info.name);
                    keep.push(EndpointWorker::spawn(info));
                }
            }
            workers = keep;
        }

        for w in &mut workers {
            while let Ok(chunk) = w.rx.try_recv() {
                w.pending.extend(chunk);
            }
        }

        // Master clock, MIX_LATENCY behind wall time so every healthy endpoint has delivered.
        let elapsed = started.elapsed().saturating_sub(MIX_LATENCY);
        let expected = (elapsed.as_secs_f64() * rate) as u64;
        let deficit = expected.saturating_sub(emitted);
        if deficit >= resync_after {
            log::warn!(
                "WASAPI loopback: mixer {:.1} s behind wall clock (sleep/wake or starvation); re-basing",
                deficit as f64 / rate
            );
            started = Instant::now();
            emitted = 0;
            for w in &mut workers {
                w.pending.clear();
            }
            continue;
        }
        if deficit == 0 {
            continue;
        }

        let mut queues: Vec<VecDeque<f32>> = workers
            .iter_mut()
            .map(|w| std::mem::take(&mut w.pending))
            .collect();
        let mixed = mix_next(&mut queues, deficit as usize, max_backlog);
        for (w, q) in workers.iter_mut().zip(queues) {
            w.pending = q;
        }
        emitted += deficit;

        // Hand-off, mirroring the cpal callback (recorder.rs::build_stream) and the CoreAudio
        // IOProc: while the consumer drains on Cmd::Stop, send EndOfStream exactly once and
        // otherwise stay silent; frames mixed between a stop and the next start are dropped.
        // On the stop edge this tick's chunk still belongs to the recording, and so does the
        // tail the mixer is holding back (up to MIX_LATENCY of audio the endpoints have already
        // delivered): flush both ahead of the sentinel so the track ends at the stop instant.
        if stop_flag.load(Ordering::Relaxed) {
            if !eos_sent {
                let _ = sample_tx.send(AudioChunk::Samples(mixed));
                let tail = workers
                    .iter()
                    .map(|w| w.pending.len())
                    .max()
                    .unwrap_or(0)
                    .min(max_tail);
                if tail > 0 {
                    let mut queues: Vec<VecDeque<f32>> = workers
                        .iter_mut()
                        .map(|w| std::mem::take(&mut w.pending))
                        .collect();
                    let flushed = mix_next(&mut queues, tail, max_backlog);
                    for (w, q) in workers.iter_mut().zip(queues) {
                        w.pending = q;
                    }
                    emitted += tail as u64;
                    let _ = sample_tx.send(AudioChunk::Samples(flushed));
                }
                let _ = sample_tx.send(AudioChunk::EndOfStream);
                eos_sent = true;
            }
            continue;
        }
        eos_sent = false;
        if sample_tx.send(AudioChunk::Samples(mixed)).is_err() {
            break; // consumer gone (Shutdown raced us)
        }
    }

    for w in workers.drain(..) {
        w.retire();
    }
    drop(enumerator);
    unsafe { CoUninitialize() };
}

/// Every render endpoint in the ACTIVE state, with its friendly name. Empty on any error.
unsafe fn active_render_endpoints(enumerator: &IMMDeviceEnumerator) -> Vec<EndpointInfo> {
    let Ok(collection) = enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else {
        return Vec::new();
    };
    let count = collection.GetCount().unwrap_or(0);
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let Ok(device) = collection.Item(i) else {
            continue;
        };
        let Ok(id_ptr) = device.GetId() else {
            continue;
        };
        let id = id_ptr.to_string().unwrap_or_default();
        CoTaskMemFree(Some(id_ptr.as_ptr() as *const _));
        if id.is_empty() {
            continue;
        }
        let name = device
            .OpenPropertyStore(STGM_READ)
            .and_then(|store| store.GetValue(&PKEY_Device_FriendlyName))
            .map(|v| v.to_string())
            .ok()
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| id.clone());
        out.push(EndpointInfo { id, name });
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Per-endpoint capture thread
// ---------------------------------------------------------------------------------------------

/// Everything one endpoint's capture loop needs, created on and never leaving its thread.
struct Loopback {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    event: HANDLE,
    fmt: StreamFormat,
}

impl Drop for Loopback {
    fn drop(&mut self) {
        // Stop before the COM pointers go away; errors here are not actionable.
        unsafe {
            let _ = self.client.Stop();
            if !self.event.is_invalid() {
                let _ = CloseHandle(self.event);
            }
        }
    }
}

/// Body of one endpoint's capture thread: open the loopback on that device, then run until
/// `quit` or the stream dies (`dead` is raised so the coordinator retires and re-opens it).
fn endpoint_thread(
    info: EndpointInfo,
    tx: mpsc::Sender<Vec<f32>>,
    quit: Arc<AtomicBool>,
    dead: Arc<AtomicBool>,
) {
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if com.is_err() {
        log::warn!("WASAPI loopback \"{}\": CoInitializeEx failed: {com}", info.name);
        dead.store(true, Ordering::Relaxed);
        return;
    }
    let result = unsafe { open_loopback(&info.id) };
    let loopback = match result {
        Ok(l) => l,
        Err(e) => {
            log::warn!("WASAPI loopback \"{}\": {e}", info.name);
            dead.store(true, Ordering::Relaxed);
            unsafe { CoUninitialize() };
            return;
        }
    };
    log::info!(
        "WASAPI loopback \"{}\": {} Hz, {} ch, {:?}, block {} B",
        info.name,
        loopback.fmt.sample_rate,
        loopback.fmt.channels,
        loopback.fmt.format,
        loopback.fmt.block_align
    );
    if let Err(e) = unsafe { loopback.client.Start() } {
        log::warn!("WASAPI loopback \"{}\": IAudioClient::Start failed: {e}", info.name);
        dead.store(true, Ordering::Relaxed);
        drop(loopback);
        unsafe { CoUninitialize() };
        return;
    }

    capture_loop(&info, &loopback, &tx, &quit, &dead);

    drop(loopback);
    unsafe { CoUninitialize() };
}

/// Activate the endpoint `id` for shared-mode, event-driven loopback at its mix format.
/// Every failure names the call that produced it.
unsafe fn open_loopback(id: &str) -> Result<Loopback, String> {
    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| format!("CoCreateInstance(MMDeviceEnumerator) failed: {e}"))?;
    let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
    let device: IMMDevice = enumerator
        .GetDevice(PCWSTR(wide.as_ptr()))
        .map_err(|e| format!("IMMDeviceEnumerator::GetDevice failed: {e}"))?;
    let client: IAudioClient = device
        .Activate(CLSCTX_ALL, None)
        .map_err(|e| format!("IMMDevice::Activate(IAudioClient) failed: {e}"))?;

    let mix: *mut WAVEFORMATEX = client
        .GetMixFormat()
        .map_err(|e| format!("IAudioClient::GetMixFormat failed: {e}"))?;
    if mix.is_null() {
        return Err("IAudioClient::GetMixFormat returned null".into());
    }
    // Decode the header before Initialize so an unsupported format fails here, cleanly.
    let fmt = {
        let wf = &*mix;
        let sub = if wf.wFormatTag == WAVE_FORMAT_EXTENSIBLE && wf.cbSize >= 22 {
            Some((*(mix as *const WAVEFORMATEXTENSIBLE)).SubFormat)
        } else {
            None
        };
        classify_format(
            wf.wFormatTag,
            sub,
            wf.wBitsPerSample,
            wf.nChannels,
            wf.nSamplesPerSec,
            wf.nBlockAlign,
        )
    };
    let init = fmt.as_ref().ok().map(|_| {
        client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            BUFFER_DURATION_HNS,
            0,
            mix,
            None,
        )
    });
    CoTaskMemFree(Some(mix as *const _));
    let fmt = fmt?;
    init.expect("fmt is Ok here")
        .map_err(|e| format!("IAudioClient::Initialize(loopback) failed: {e}"))?;

    let event = CreateEventW(None, false, false, PCWSTR::null())
        .map_err(|e| format!("CreateEventW failed: {e}"))?;
    if let Err(e) = client.SetEventHandle(event) {
        let _ = CloseHandle(event);
        return Err(format!("IAudioClient::SetEventHandle failed: {e}"));
    }
    let capture: IAudioCaptureClient = match client.GetService() {
        Ok(c) => c,
        Err(e) => {
            let _ = CloseHandle(event);
            return Err(format!("IAudioClient::GetService(IAudioCaptureClient) failed: {e}"));
        }
    };
    Ok(Loopback {
        client,
        capture,
        event,
        fmt,
    })
}

/// One endpoint's loop. Each iteration: wait for the event (bounded), drain every pending
/// packet into `buf` (silent packets become zeros), top up with zeros if the clock says the
/// endpoint went quiet, resample to 16 kHz mono and send to the coordinator. Any capture error
/// (device unplugged, format changed) ends the loop with `dead` raised; the coordinator pads
/// zeros meanwhile and re-opens the endpoint if it is still active at the next scan.
fn capture_loop(
    info: &EndpointInfo,
    lb: &Loopback,
    tx: &mpsc::Sender<Vec<f32>>,
    quit: &AtomicBool,
    dead: &AtomicBool,
) {
    let rate = lb.fmt.sample_rate as u64;
    let min_gap = (MIN_GAP.as_secs_f64() * rate as f64) as u64;
    let resync_after = (RESYNC_AFTER.as_secs_f64() * rate as f64) as u64;

    let mut resampler = FrameResampler::new(
        lb.fmt.sample_rate as usize,
        MIX_RATE as usize,
        Duration::from_millis(30),
    );
    let mut started = Instant::now();
    let mut emitted: u64 = 0;
    let mut buf: Vec<f32> = Vec::new();
    let mut out: Vec<f32> = Vec::new();

    while !quit.load(Ordering::Relaxed) {
        // SAFETY: `event` is a live auto-reset event owned by `lb`.
        let _ = unsafe { WaitForSingleObject(lb.event, TICK.as_millis() as u32) };

        buf.clear();
        match unsafe { drain_packets(lb, &mut buf) } {
            Ok(frames) => emitted += frames,
            Err(e) => {
                log::warn!(
                    "WASAPI loopback \"{}\": capture error {e}; stream retired (re-opened at the \
                     next scan if the endpoint is still active)",
                    info.name
                );
                dead.store(true, Ordering::Relaxed);
                break;
            }
        }

        // Clock discipline (R1), in device frames.
        let expected = (started.elapsed().as_secs_f64() * rate as f64) as u64;
        let deficit = expected.saturating_sub(emitted);
        if deficit >= resync_after {
            log::warn!(
                "WASAPI loopback \"{}\": {:.1} s behind wall clock; re-basing",
                info.name,
                deficit as f64 / rate as f64
            );
            started = Instant::now();
            emitted = 0;
        } else {
            let fill = gap_fill_frames(expected, emitted, min_gap);
            if fill > 0 {
                buf.extend(std::iter::repeat(0.0f32).take(fill as usize));
                emitted += fill;
            }
        }

        if buf.is_empty() {
            continue;
        }
        out.clear();
        resampler.push(&buf, |frame: &[f32]| out.extend_from_slice(frame));
        if !out.is_empty() && tx.send(std::mem::take(&mut out)).is_err() {
            break; // coordinator gone
        }
    }
}

/// Pull every packet WASAPI has queued, appending mono f32 at the device rate to `out`.
/// Returns the number of frames taken (real or silent). A `SILENT` packet is zeros of the stated
/// length: the stream position still advances, so the clock stays honest.
unsafe fn drain_packets(lb: &Loopback, out: &mut Vec<f32>) -> windows::core::Result<u64> {
    let mut total: u64 = 0;
    loop {
        let packet = lb.capture.GetNextPacketSize()?;
        if packet == 0 {
            return Ok(total);
        }
        let mut data: *mut u8 = std::ptr::null_mut();
        let mut frames: u32 = 0;
        let mut flags: u32 = 0;
        lb.capture
            .GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
        if frames > 0 {
            let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
            if silent || data.is_null() {
                out.extend(std::iter::repeat(0.0f32).take(frames as usize));
            } else {
                let bytes =
                    std::slice::from_raw_parts(data, frames as usize * lb.fmt.block_align);
                bytes_to_mono_f32(bytes, &lb.fmt, out);
            }
            total += frames as u64;
        }
        lb.capture.ReleaseBuffer(frames)?;
    }
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_send<T: Send>() {}

    #[test]
    fn recorder_is_send() {
        // IF-W01: SessionManager stores the recorder behind a Mutex across threads.
        assert_send::<SystemAudioRecorder>();
    }

    // --- classify_format -------------------------------------------------------------------

    #[test]
    fn classifies_extensible_float_stereo_48k() {
        let f = classify_format(
            WAVE_FORMAT_EXTENSIBLE,
            Some(KSDATAFORMAT_SUBTYPE_IEEE_FLOAT),
            32,
            2,
            48_000,
            8,
        )
        .unwrap();
        assert_eq!(
            f,
            StreamFormat {
                sample_rate: 48_000,
                channels: 2,
                format: SampleFormat::F32,
                block_align: 8
            }
        );
    }

    #[test]
    fn classifies_plain_pcm_tags_and_extensible_pcm_subformats() {
        assert_eq!(
            classify_format(WAVE_FORMAT_PCM, None, 16, 2, 44_100, 4)
                .unwrap()
                .format,
            SampleFormat::I16
        );
        assert_eq!(
            classify_format(WAVE_FORMAT_IEEE_FLOAT, None, 32, 1, 16_000, 4)
                .unwrap()
                .format,
            SampleFormat::F32
        );
        assert_eq!(
            classify_format(
                WAVE_FORMAT_EXTENSIBLE,
                Some(KSDATAFORMAT_SUBTYPE_PCM),
                24,
                6,
                96_000,
                18
            )
            .unwrap()
            .format,
            SampleFormat::I24
        );
        assert_eq!(
            classify_format(WAVE_FORMAT_EXTENSIBLE, Some(KSDATAFORMAT_SUBTYPE_PCM), 32, 2, 48_000, 8)
                .unwrap()
                .format,
            SampleFormat::I32
        );
    }

    #[test]
    fn rejects_formats_it_cannot_decode() {
        let unknown_sub = GUID::from_u128(0x1234);
        assert!(classify_format(WAVE_FORMAT_EXTENSIBLE, Some(unknown_sub), 32, 2, 48_000, 8).is_err());
        assert!(classify_format(WAVE_FORMAT_EXTENSIBLE, None, 32, 2, 48_000, 8).is_err());
        assert!(classify_format(0x0055, None, 16, 2, 48_000, 4).is_err()); // MP3 tag
        assert!(classify_format(WAVE_FORMAT_IEEE_FLOAT, None, 64, 2, 48_000, 16).is_err());
        assert!(classify_format(WAVE_FORMAT_PCM, None, 8, 2, 48_000, 2).is_err());
        assert!(classify_format(WAVE_FORMAT_PCM, None, 16, 0, 48_000, 0).is_err());
        assert!(classify_format(WAVE_FORMAT_PCM, None, 16, 2, 0, 4).is_err());
        // block align inconsistent with channels x bits
        assert!(classify_format(WAVE_FORMAT_PCM, None, 16, 2, 48_000, 6).is_err());
    }

    // --- bytes_to_mono_f32 -----------------------------------------------------------------

    fn fmt(format: SampleFormat, channels: usize) -> StreamFormat {
        let bytes = match format {
            SampleFormat::F32 | SampleFormat::I32 => 4,
            SampleFormat::I24 => 3,
            SampleFormat::I16 => 2,
        };
        StreamFormat {
            sample_rate: 48_000,
            channels,
            format,
            block_align: channels * bytes,
        }
    }

    #[test]
    fn f32_stereo_is_averaged_to_mono() {
        let mut bytes = Vec::new();
        for s in [0.5f32, -0.5, 1.0, 0.0] {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let mut out = Vec::new();
        bytes_to_mono_f32(&bytes, &fmt(SampleFormat::F32, 2), &mut out);
        assert_eq!(out, vec![0.0, 0.5]);
    }

    #[test]
    fn i16_full_scale_maps_to_unit_range() {
        let mut bytes = Vec::new();
        for s in [i16::MAX, i16::MIN, 0] {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let mut out = Vec::new();
        bytes_to_mono_f32(&bytes, &fmt(SampleFormat::I16, 1), &mut out);
        assert!((out[0] - 0.99997).abs() < 1e-4);
        assert_eq!(out[1], -1.0);
        assert_eq!(out[2], 0.0);
    }

    #[test]
    fn i24_packed_samples_are_sign_extended() {
        // +0x400000 = half scale, -0x400000 = -half scale, in 3-byte little endian.
        let bytes = [0x00, 0x00, 0x40, 0x00, 0x00, 0xC0];
        let mut out = Vec::new();
        bytes_to_mono_f32(&bytes, &fmt(SampleFormat::I24, 1), &mut out);
        assert_eq!(out, vec![0.5, -0.5]);
    }

    #[test]
    fn i32_container_scales_by_2_pow_31() {
        let mut bytes = Vec::new();
        for s in [i32::MIN, 1 << 30] {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let mut out = Vec::new();
        bytes_to_mono_f32(&bytes, &fmt(SampleFormat::I32, 1), &mut out);
        assert_eq!(out, vec![-1.0, 0.5]);
    }

    #[test]
    fn partial_trailing_frame_is_ignored_and_output_appends() {
        let f = fmt(SampleFormat::I16, 2);
        let mut bytes = Vec::new();
        for s in [1000i16, 3000, 7] {
            bytes.extend_from_slice(&s.to_le_bytes());
        }
        let mut out = vec![9.0];
        bytes_to_mono_f32(&bytes, &f, &mut out);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], 9.0);
        assert!((out[1] - 2000.0 / 32768.0).abs() < 1e-6);
    }

    // --- gap_fill_frames -------------------------------------------------------------------

    #[test]
    fn small_deficits_are_not_filled() {
        // One 10 ms WASAPI period late at 48 kHz = 480 frames; threshold 2400 (50 ms).
        assert_eq!(gap_fill_frames(48_000, 47_520, 2_400), 0);
        assert_eq!(gap_fill_frames(48_000, 48_000, 2_400), 0);
    }

    #[test]
    fn deficit_at_or_over_threshold_is_filled_entirely() {
        assert_eq!(gap_fill_frames(48_000, 45_600, 2_400), 2_400);
        assert_eq!(gap_fill_frames(480_000, 100, 2_400), 479_900);
    }

    #[test]
    fn running_ahead_of_the_clock_never_drops() {
        assert_eq!(gap_fill_frames(48_000, 48_500, 2_400), 0);
        assert_eq!(gap_fill_frames(0, 10, 2_400), 0);
    }

    /// The R1 scenario, simulated: 60 s at 48 kHz, the renderer delivers 10 ms packets only
    /// during [0, 15) s and [45, 60) s, the loop ticks every 20 ms. Total emitted frames must
    /// equal wall time within 0.5 %.
    #[test]
    fn simulated_30s_silence_keeps_wall_clock_length() {
        let rate: u64 = 48_000;
        let min_gap = (MIN_GAP.as_secs_f64() * rate as f64) as u64;
        let tick = Duration::from_millis(20);
        let packet = rate / 100; // 10 ms
        let mut now = Duration::ZERO;
        let mut emitted: u64 = 0;
        let mut delivered_until = Duration::ZERO;
        let mut inserted_zero_runs = 0usize;
        while now < Duration::from_secs(60) {
            now += tick;
            let rendering = now < Duration::from_secs(15) || now >= Duration::from_secs(45);
            if rendering {
                // The driver catches us up in whole packets.
                while delivered_until + Duration::from_millis(10) <= now {
                    delivered_until += Duration::from_millis(10);
                    emitted += packet;
                }
            } else {
                delivered_until = now; // nothing rendered: those frames never exist
            }
            let expected = (now.as_secs_f64() * rate as f64) as u64;
            let fill = gap_fill_frames(expected, emitted, min_gap);
            if fill > 0 {
                emitted += fill;
                inserted_zero_runs += 1;
            }
        }
        let expected_total = 60 * rate;
        let err = (emitted as f64 - expected_total as f64).abs() / expected_total as f64;
        assert!(err < 0.005, "emitted {emitted} vs {expected_total} ({err:.4})");
        assert!(inserted_zero_runs > 0, "the silent 30 s must have been zero-filled");
    }

    // --- mix_next ---------------------------------------------------------------------------

    fn q(v: &[f32]) -> VecDeque<f32> {
        v.iter().copied().collect()
    }

    #[test]
    fn mix_sums_endpoints_and_pads_the_short_one_with_silence() {
        // Speakers carry 4 samples, the hands-free endpoint only 2 so far.
        let mut queues = vec![q(&[0.1, 0.2, 0.3, 0.4]), q(&[0.5, 0.5])];
        let out = mix_next(&mut queues, 4, 1_000);
        assert_eq!(out, vec![0.6, 0.7, 0.3, 0.4]);
        assert!(queues[0].is_empty() && queues[1].is_empty());
    }

    #[test]
    fn mix_with_no_endpoints_is_silence_of_the_requested_length() {
        let mut queues: Vec<VecDeque<f32>> = Vec::new();
        assert_eq!(mix_next(&mut queues, 3, 10), vec![0.0; 3]);
        let mut dead = vec![VecDeque::new()];
        assert_eq!(mix_next(&mut dead, 2, 10), vec![0.0; 2]);
    }

    #[test]
    fn mix_clamps_and_leaves_the_remainder_queued() {
        let mut queues = vec![q(&[0.9, 0.9, 0.1]), q(&[0.9, -2.0, 0.1])];
        let out = mix_next(&mut queues, 2, 1_000);
        assert_eq!(out, vec![1.0, -1.0]);
        assert_eq!(queues[0], q(&[0.1]));
        assert_eq!(queues[1], q(&[0.1]));
    }

    #[test]
    fn mix_trims_a_runaway_backlog_from_the_front() {
        // An endpoint that raced ahead keeps only the newest `max_backlog` samples after the take.
        let mut queues = vec![q(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0])];
        let out = mix_next(&mut queues, 1, 2);
        assert_eq!(out, vec![1.0]);
        assert_eq!(queues[0], q(&[5.0, 6.0]));
    }

    // --- Live acceptance (manual: real WASAPI endpoints) ------------------------------------
    // Run: cargo test --lib system_audio -- --ignored --nocapture

    /// Open every active endpoint for 3 s and check the consumer returns ~3 s of 16 kHz audio
    /// whether or not anything is playing (silence is zero-filled by the clocks).
    #[test]
    #[ignore = "manual: needs a live render endpoint"]
    fn live_three_seconds_have_three_seconds_of_frames() {
        let mut r = SystemAudioRecorder::new().unwrap();
        r.open().expect("open WASAPI loopback");
        r.start().expect("start");
        std::thread::sleep(Duration::from_secs(3));
        let samples = r.stop().expect("stop");
        r.close().unwrap();
        let secs = samples.len() as f64 / 16_000.0;
        eprintln!("captured {secs:.3} s ({} samples)", samples.len());
        assert!((secs - 3.0).abs() < 0.3, "expected ~3 s, got {secs:.3} s");
    }

    /// Self-contained signal check: record ~12 s while PowerShell plays a Windows ring tone
    /// through the default endpoint during seconds 3..6. The played window must carry signal,
    /// and must be clearly louder than the leading window when that one was really quiet (the
    /// user may already be playing something). Set `PLAUDY_LOOPBACK_WAV=<path>` to dump the
    /// 16 kHz mono capture as a WAV for listening. Needs the endpoint unmuted.
    #[test]
    #[ignore = "manual: needs a live, unmuted render endpoint and plays a sound"]
    fn live_played_sound_is_louder_than_silence() {
        let mut r = SystemAudioRecorder::new().unwrap();
        r.open().expect("open WASAPI loopback");
        r.start().expect("start");
        std::thread::sleep(Duration::from_secs(3));
        let mut player = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "$p = New-Object System.Media.SoundPlayer 'C:/Windows/Media/Ring01.wav'; $p.Load(); $end = (Get-Date).AddSeconds(3); while ((Get-Date) -lt $end) { $p.PlaySync() }",
            ])
            .spawn()
            .expect("spawn powershell player");
        let _ = player.wait();
        std::thread::sleep(Duration::from_secs(3));
        let samples = r.stop().expect("stop");
        r.close().unwrap();

        let rms = |from_s: usize, to_s: usize| -> f32 {
            let seg = &samples[(from_s * 16_000).min(samples.len())..(to_s * 16_000).min(samples.len())];
            (seg.iter().map(|x| x * x).sum::<f32>() / seg.len().max(1) as f32).sqrt()
        };
        let quiet = rms(0, 2);
        let loud = rms(4, 6);
        eprintln!(
            "captured {:.3} s; rms quiet[0..2s]={quiet:.5} loud[4..6s]={loud:.5}",
            samples.len() as f64 / 16_000.0
        );
        if let Ok(path) = std::env::var("PLAUDY_LOOPBACK_WAV") {
            let spec = hound::WavSpec {
                channels: 1,
                sample_rate: 16_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            };
            let mut w = hound::WavWriter::create(&path, spec).unwrap();
            for s in &samples {
                w.write_sample((s.clamp(-1.0, 1.0) * 32767.0) as i16).unwrap();
            }
            w.finalize().unwrap();
            eprintln!("wrote {path}");
        }
        assert!(loud > 0.005, "played window carries no signal: loud={loud}");
        if quiet < 0.002 {
            assert!(loud > quiet * 5.0, "played window must be audibly louder: quiet={quiet} loud={loud}");
        } else {
            eprintln!("note: the endpoint was already playing something (quiet rms {quiet:.4}); ratio check skipped");
        }
    }
}
