//! Windows system-audio (loopback) capture producer (IF-W01, WIN-CAP-001).
//!
//! Captures everything the default render endpoint plays (the remote side of a Teams or Meet
//! call, through headset or speakers alike) with a WASAPI shared-mode **loopback** stream, and
//! feeds it into the same `run_consumer` pipeline as the microphone `AudioRecorder` and the macOS
//! Process Tap. The only platform-specific part is the sample source: a capture thread that
//! waits on the WASAPI event, drains `IAudioCaptureClient`, converts the endpoint's mix format
//! to mono f32 and hands chunks to the consumer. Everything downstream (resampling to 16 kHz,
//! chunk sink to the PCM writer, mix, finalize, diarization) is shared and untouched.
//!
//! Two Windows facts shape the design:
//!
//! 1. **Silence stops the clock.** A loopback stream only delivers packets while *something*
//!    renders; when every app goes quiet, WASAPI delivers nothing and (on some drivers) never
//!    signals the event either. A naive capture would then produce a WAV shorter than the
//!    meeting, and the mic track and the system track would drift apart (risk R1). The capture
//!    thread therefore keeps its own monotonic clock and inserts zero frames whenever the
//!    emitted frame count falls behind wall time (`gap_fill_frames`), so total length ==
//!    elapsed time within a buffer period. The event wait has a short timeout for the same
//!    reason: the loop must run even when WASAPI is mute.
//! 2. **COM lives on one thread.** Every COM object is created, used and released on the
//!    capture thread (MTA). `open()` only waits for that thread's init result. That keeps
//!    `SystemAudioRecorder` trivially `Send` (it owns channels and join handles, no COM
//!    pointers), which `SessionManager` requires.
//!
//! All `unsafe` FFI is confined to this file behind the safe `SystemAudioRecorder` API.
#![cfg(all(target_os = "windows", target_arch = "x86_64"))]
#![allow(dead_code)]

use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::time::{Duration, Instant};

use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Media::Audio::{
    eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDevice, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_E_DEVICE_INVALIDATED,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_EVENTCALLBACK, AUDCLNT_STREAMFLAGS_LOOPBACK,
    WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_MULTITHREADED,
};
use windows::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

use super::recorder::{run_consumer, AudioChunk, Cmd, FrameSinks, VadPolicy};

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

/// How one sample is laid out in the endpoint's mix format. Shared-mode Windows mixes in 32-bit
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

/// The facts about the mix format the capture loop needs, extracted once at `open()`.
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

/// Clock discipline for the loopback stream (R1). `expected` is how many frames wall time says
/// should have been emitted since the stream started; `emitted` is how many were (real plus
/// zero). Returns the number of zero frames to insert *now*:
///
/// * `0` while the deficit is below `min_gap` — real packets arrive in bursts up to one buffer
///   period late, and filling inside that window would interleave zeros with real audio;
/// * the whole deficit once it exceeds `min_gap` — the renderer went quiet, catch up to the
///   clock so the WAV keeps wall-clock length and the mic track stays aligned;
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

/// Deficit beyond which the clock is re-based instead of filled: the machine slept, or the
/// thread was starved for a very long time. Filling minutes of zeros would make the system
/// track *longer* than the mic track (whose cpal stream simply stopped), so re-sync instead
/// and log it; the session's own sleep-wake handling owns that case.
const RESYNC_AFTER: Duration = Duration::from_secs(10);

/// Below this deficit the loop does not fill (see [`gap_fill_frames`]). Two default WASAPI
/// periods (10 ms each) plus margin: long enough that bursty delivery never triggers it,
/// short enough that the first real packet after a silence lands at most this late.
const MIN_GAP: Duration = Duration::from_millis(50);

/// How long the capture loop waits for the WASAPI event before running a clock check anyway.
const EVENT_WAIT: Duration = Duration::from_millis(20);

/// Shared-mode buffer the endpoint keeps for us, in 100 ns units (200 ms). Generous so a busy
/// UI thread cannot overflow it; latency is irrelevant for a recording.
const BUFFER_DURATION_HNS: i64 = 2_000_000;

// ---------------------------------------------------------------------------------------------
// The recorder
// ---------------------------------------------------------------------------------------------

pub struct SystemAudioRecorder {
    cmd_tx: Option<mpsc::Sender<Cmd>>,
    /// The shared `run_consumer` worker (resample + sinks), same as the mic path.
    worker_handle: Option<std::thread::JoinHandle<()>>,
    /// The WASAPI capture thread; owns every COM object.
    capture_handle: Option<std::thread::JoinHandle<()>>,
    level_cb: Option<Arc<dyn Fn(Vec<f32>) + Send + Sync + 'static>>,
    chunk_sink: Option<mpsc::Sender<Vec<f32>>>,
    /// Shared with `run_consumer`: true between `Cmd::Stop` and the next `Cmd::Start`, when the
    /// producer must send `EndOfStream` once and otherwise stay quiet.
    stop_flag: Arc<AtomicBool>,
    /// Tells the capture thread to tear down. Set by `close()` only.
    quit: Arc<AtomicBool>,
}

impl SystemAudioRecorder {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        Ok(SystemAudioRecorder {
            cmd_tx: None,
            worker_handle: None,
            capture_handle: None,
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

    /// Open the loopback stream on the default render endpoint and start the two threads.
    /// Returns the endpoint's error verbatim when WASAPI refuses (no render device, exclusive
    /// mode held by another app, ...). Idempotent while open.
    pub fn open(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if self.worker_handle.is_some() {
            return Ok(()); // already open
        }

        let (sample_tx, sample_rx) = mpsc::channel::<AudioChunk>();
        let (cmd_tx, cmd_rx) = mpsc::channel::<Cmd>();
        let (init_tx, init_rx) = mpsc::channel::<Result<u32, String>>();

        let stop_flag = Arc::new(AtomicBool::new(false));
        let quit = Arc::new(AtomicBool::new(false));
        self.stop_flag = stop_flag.clone();
        self.quit = quit.clone();

        // 1. Capture thread: all COM work happens here. It reports the sample rate (or the
        //    init error) once, then runs until `quit`.
        let capture = std::thread::Builder::new()
            .name("plaudy-wasapi-loopback".into())
            .spawn({
                let stop_flag = stop_flag.clone();
                let quit = quit.clone();
                move || capture_thread(init_tx, sample_tx, stop_flag, quit)
            })?;

        let sample_rate = match init_rx.recv() {
            Ok(Ok(rate)) => rate,
            Ok(Err(msg)) => {
                let _ = capture.join();
                return Err(msg.into());
            }
            Err(_) => {
                let _ = capture.join();
                return Err("WASAPI capture thread died during initialisation".into());
            }
        };

        // 2. Consumer worker, identical wiring to the macOS tap and the mic recorder.
        let level_cb = self.level_cb.clone();
        let chunk_sink = self.chunk_sink.clone();
        let worker = std::thread::spawn({
            let stop_flag = stop_flag.clone();
            move || {
                run_consumer(
                    sample_rate,
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
        self.capture_handle = Some(capture);
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
    /// the capture thread sees it and injects `EndOfStream` once, and `run_consumer`'s drain
    /// loop breaks on the sentinel and replies with the drained samples (an empty Vec when a
    /// chunk sink took them, exactly like macOS).
    pub fn stop(&self) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        let (resp_tx, resp_rx) = mpsc::channel();
        if let Some(tx) = &self.cmd_tx {
            tx.send(Cmd::Stop(resp_tx))?;
        }
        Ok(resp_rx.recv_timeout(super::recorder::STOP_REPLY_TIMEOUT)?)
    }

    /// Stop the stream, release every COM object (on the capture thread), join both threads.
    /// Safe to call twice and from `Drop`.
    pub fn close(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(tx) = self.cmd_tx.take() {
            let _ = tx.send(Cmd::Shutdown);
        }
        self.stop_flag.store(true, Ordering::Relaxed);
        self.quit.store(true, Ordering::Relaxed);

        // The capture thread owns sample_tx; joining it first closes the channel so the
        // consumer cannot block on a producer that is gone.
        if let Some(h) = self.capture_handle.take() {
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
// Capture thread
// ---------------------------------------------------------------------------------------------

/// Everything the capture loop needs, created on and never leaving the capture thread.
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

/// Body of the capture thread. Initialises COM (MTA) and the loopback stream, reports the
/// outcome through `init_tx`, then runs the capture loop until `quit`.
fn capture_thread(
    init_tx: mpsc::Sender<Result<u32, String>>,
    sample_tx: mpsc::Sender<AudioChunk>,
    stop_flag: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
) {
    // SAFETY: a fresh thread; MTA is the right apartment for an audio worker and matches what
    // cpal's WASAPI host uses in-process.
    let com = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    if com.is_err() {
        let _ = init_tx.send(Err(format!("CoInitializeEx failed: {com}")));
        return;
    }

    let loopback = match unsafe { open_loopback() } {
        Ok(l) => l,
        Err(e) => {
            let _ = init_tx.send(Err(e));
            unsafe { CoUninitialize() };
            return;
        }
    };
    log::info!(
        "WASAPI loopback open: {} Hz, {} ch, {:?}, block {} B",
        loopback.fmt.sample_rate,
        loopback.fmt.channels,
        loopback.fmt.format,
        loopback.fmt.block_align
    );
    if let Err(e) = unsafe { loopback.client.Start() } {
        let _ = init_tx.send(Err(format!("IAudioClient::Start failed: {e}")));
        drop(loopback);
        unsafe { CoUninitialize() };
        return;
    }
    // From here on the recorder is live; report success and run.
    let _ = init_tx.send(Ok(loopback.fmt.sample_rate));

    capture_loop(&loopback, &sample_tx, &stop_flag, &quit);

    drop(loopback);
    unsafe { CoUninitialize() };
}

/// Activate the default render endpoint for shared-mode, event-driven loopback at its mix
/// format. Every failure names the call that produced it.
unsafe fn open_loopback() -> Result<Loopback, String> {
    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| format!("CoCreateInstance(MMDeviceEnumerator) failed: {e}"))?;
    let device: IMMDevice = enumerator
        .GetDefaultAudioEndpoint(eRender, eConsole)
        .map_err(|e| format!("no default render endpoint (is any output device enabled?): {e}"))?;
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

/// The realtime-ish loop. Each iteration: wait for the event (bounded), drain every pending
/// packet into `buf` (silent packets become zeros), top up with zeros if the clock says the
/// renderer went quiet, then hand the chunk to the consumer under the same stop/EndOfStream
/// contract as the cpal callback. A device loss is logged once and the loop keeps time with
/// zeros until `quit`, so `stop()` still gets its reply and the WAV keeps its length.
fn capture_loop(
    lb: &Loopback,
    sample_tx: &mpsc::Sender<AudioChunk>,
    stop_flag: &AtomicBool,
    quit: &AtomicBool,
) {
    let rate = lb.fmt.sample_rate as u64;
    let min_gap = MIN_GAP.as_secs_f64() * rate as f64;
    let min_gap = min_gap as u64;
    let resync_after = (RESYNC_AFTER.as_secs_f64() * rate as f64) as u64;

    let mut started = Instant::now();
    let mut emitted: u64 = 0;
    let mut eos_sent = false;
    let mut device_dead = false;
    let mut buf: Vec<f32> = Vec::new();

    while !quit.load(Ordering::Relaxed) {
        if device_dead {
            std::thread::sleep(EVENT_WAIT);
        } else {
            // SAFETY: `event` is a live auto-reset event owned by `lb`.
            let _ = unsafe { WaitForSingleObject(lb.event, EVENT_WAIT.as_millis() as u32) };
        }

        buf.clear();
        if !device_dead {
            match unsafe { drain_packets(lb, &mut buf) } {
                Ok(frames) => emitted += frames,
                Err(e) if e.code() == AUDCLNT_E_DEVICE_INVALIDATED => {
                    log::warn!(
                        "WASAPI loopback: default render device invalidated (unplugged or \
                         changed); keeping time with silence until the session stops"
                    );
                    device_dead = true;
                }
                Err(e) => {
                    log::error!("WASAPI loopback: capture error {e}; switching to silence");
                    device_dead = true;
                }
            }
        }

        // Clock discipline (R1).
        let expected = (started.elapsed().as_secs_f64() * rate as f64) as u64;
        let deficit = expected.saturating_sub(emitted);
        if deficit >= resync_after {
            log::warn!(
                "WASAPI loopback: {:.1} s behind wall clock (sleep/wake or starvation); re-basing \
                 the clock instead of filling",
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

        // Hand-off, mirroring the cpal callback (recorder.rs::build_stream) and the CoreAudio
        // IOProc: while the consumer drains on Cmd::Stop, send EndOfStream exactly once and
        // otherwise stay silent; frames captured between a stop and the next start are dropped
        // (nobody is recording them).
        if stop_flag.load(Ordering::Relaxed) {
            if !eos_sent {
                let _ = sample_tx.send(AudioChunk::EndOfStream);
                eos_sent = true;
            }
            continue;
        }
        eos_sent = false;

        if !buf.is_empty() && sample_tx.send(AudioChunk::Samples(std::mem::take(&mut buf))).is_err() {
            // Consumer gone (Shutdown raced us): nothing left to feed.
            break;
        }
    }
}

/// Pull every packet WASAPI has queued, appending mono f32 to `out`. Returns the number of
/// frames taken (real or silent). A `SILENT` packet is zeros of the stated length: the stream
/// position still advances, so the clock stays honest.
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

    // --- Live acceptance (manual: real WASAPI endpoint) --------------------------------------
    // Run: cargo test --lib system_audio -- --ignored --nocapture

    /// Open the real default endpoint for 3 s and check the consumer returns ~3 s of 16 kHz
    /// audio whether or not anything is playing (silence is zero-filled by the clock).
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

    /// Self-contained signal check: record 9 s of loopback while PowerShell plays a Windows
    /// ring tone through the default endpoint during seconds 3..6. The played window must be
    /// clearly louder than the leading silence, proving the frames are the endpoint's audio and
    /// not just clock-filled zeros. Set `PLAUDY_LOOPBACK_WAV=<path>` to also dump the 16 kHz
    /// mono capture as a WAV for listening. Needs the endpoint unmuted.
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
        // The endpoint's own output is what we capture, so anything else the user is playing
        // (music, a call) is in the "quiet" window too. The ratio check therefore only applies
        // when that window really was quiet; the absolute floor always does.
        assert!(loud > 0.005, "played window carries no signal: loud={loud}");
        if quiet < 0.002 {
            assert!(loud > quiet * 5.0, "played window must be audibly louder: quiet={quiet} loud={loud}");
        } else {
            eprintln!("note: the endpoint was already playing something (quiet rms {quiet:.4}); ratio check skipped");
        }
    }
}
