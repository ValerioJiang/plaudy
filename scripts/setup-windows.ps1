<#
.SYNOPSIS
  One-shot setup for Plaudy on a Windows PC: installs the toolchain, fetches the
  speaker model, builds the app, and puts a shortcut on the desktop.

.DESCRIPTION
  Safe to run again: every step checks first and skips what is already done.
  Run it from the repo root in a normal PowerShell window:

    powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1

  Visual Studio Build Tools and the Vulkan SDK ask for administrator rights
  (a UAC prompt) the first time.

.PARAMETER CheckOnly   Only report what is installed and what is missing. Changes nothing.
.PARAMETER SkipInstall Do not install anything with winget; fail if a tool is missing.
.PARAMETER SkipModel   Do not download the 139 MB Sortformer speaker model.
.PARAMETER Test        Also run the Rust and MCP test suites after the build.
.PARAMETER NoShortcut  Do not create the desktop shortcut.
#>
[CmdletBinding()]
param(
  [switch]$CheckOnly,
  [switch]$SkipInstall,
  [switch]$SkipModel,
  [switch]$Test,
  [switch]$NoShortcut
)

$ErrorActionPreference = 'Stop'
$RepoRoot = Split-Path -Parent $PSScriptRoot
$Handy = Join-Path $RepoRoot 'handy'
$Tauri = Join-Path $Handy 'src-tauri'
$Exe = Join-Path $Tauri 'target\debug\handy.exe'
$started = Get-Date

function Step($msg) { Write-Host "`n==> $msg" -ForegroundColor Cyan }
function Ok($msg) { Write-Host "    ok   $msg" -ForegroundColor Green }
function Miss($msg) { Write-Host "    miss $msg" -ForegroundColor Yellow }
function Fail($msg) { Write-Host "`nERROR: $msg" -ForegroundColor Red; exit 1 }

function Invoke-Native {
  param([string]$What, [scriptblock]$Cmd)
  & $Cmd
  if ($LASTEXITCODE -ne 0) { Fail "$What failed (exit code $LASTEXITCODE)." }
}

# winget installs update the registry, not this session: re-read PATH and VULKAN_SDK.
function Refresh-Env {
  $machine = [Environment]::GetEnvironmentVariable('Path', 'Machine')
  $user = [Environment]::GetEnvironmentVariable('Path', 'User')
  $env:Path = "$machine;$user"
  $cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
  if (Test-Path $cargoBin) { $env:Path = "$cargoBin;$env:Path" }
  $vk = [Environment]::GetEnvironmentVariable('VULKAN_SDK', 'Machine')
  if (-not $vk) { $vk = [Environment]::GetEnvironmentVariable('VULKAN_SDK', 'User') }
  if (-not $vk) {
    $latest = Get-ChildItem 'C:\VulkanSDK' -Directory -ErrorAction SilentlyContinue |
      Sort-Object Name -Descending | Select-Object -First 1
    if ($latest) { $vk = $latest.FullName }
  }
  if ($vk) {
    $env:VULKAN_SDK = $vk
    $env:Path = "$(Join-Path $vk 'Bin');$env:Path"
  }
}

function Find-Bun {
  $cmd = Get-Command bun -ErrorAction SilentlyContinue
  if ($cmd) { return $cmd.Source }
  $candidates = @()
  $wingetPkgs = Join-Path $env:LOCALAPPDATA 'Microsoft\WinGet\Packages'
  if (Test-Path $wingetPkgs) {
    $candidates += Get-ChildItem $wingetPkgs -Directory -Filter 'Oven-sh.Bun*' -ErrorAction SilentlyContinue |
      ForEach-Object { Join-Path $_.FullName 'bun-windows-x64\bun.exe' }
  }
  $candidates += (Join-Path $env:USERPROFILE '.bun\bin\bun.exe')
  foreach ($c in $candidates) { if (Test-Path $c) { return $c } }
  return $null
}

function Find-GitBash {
  # Never the System32 bash.exe: that one is WSL, not Git Bash.
  $candidates = @(
    (Join-Path $env:ProgramFiles 'Git\bin\bash.exe'),
    (Join-Path $env:LOCALAPPDATA 'Programs\Git\bin\bash.exe')
  )
  $git = Get-Command git -ErrorAction SilentlyContinue
  if ($git) { $candidates += (Join-Path (Split-Path (Split-Path $git.Source)) 'bin\bash.exe') }
  foreach ($c in $candidates) { if (Test-Path $c) { return $c } }
  return $null
}

function Find-Msvc {
  $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
  if (-not (Test-Path $vswhere)) { return $null }
  $path = & $vswhere -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -latest -property installationPath
  if ($path) { return ($path | Select-Object -First 1) }
  return $null
}

function Test-Tools {
  $r = [ordered]@{}
  $r['git'] = [bool](Get-Command git -ErrorAction SilentlyContinue)
  $r['rust'] = [bool](Get-Command cargo -ErrorAction SilentlyContinue) -and [bool](Get-Command rustc -ErrorAction SilentlyContinue)
  $r['cmake'] = [bool](Get-Command cmake -ErrorAction SilentlyContinue)
  $r['bun'] = [bool](Find-Bun)
  $r['vulkan'] = [bool]$env:VULKAN_SDK -and (Test-Path (Join-Path "$env:VULKAN_SDK" 'Bin\glslc.exe'))
  $r['msvc'] = [bool](Find-Msvc)
  return $r
}

function Show-Tools($r) {
  foreach ($k in $r.Keys) {
    if ($r[$k]) {
      $detail = switch ($k) {
        'git'    { (git --version) }
        'rust'   { (rustc -V) }
        'cmake'  { (cmake --version | Select-Object -First 1) }
        'bun'    { "bun $(& (Find-Bun) -v)  ($(Find-Bun))" }
        'vulkan' { "VULKAN_SDK=$env:VULKAN_SDK" }
        'msvc'   { "MSVC in $(Find-Msvc)" }
      }
      Ok $detail
    } else { Miss $k }
  }
}

$WingetIds = [ordered]@{
  git    = @('Git.Git')
  rust   = @('Rustlang.Rustup')
  cmake  = @('Kitware.CMake')
  bun    = @('Oven-sh.Bun')
  vulkan = @('KhronosGroup.VulkanSDK')
  msvc   = @('Microsoft.VisualStudio.2022.BuildTools',
             '--override', '--quiet --wait --norestart --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended')
}

# ---------------------------------------------------------------------------
Step 'Checking the toolchain'
if (-not (Test-Path (Join-Path $Handy 'package.json'))) { Fail "Run this from a plaudy clone (no handy\package.json under $RepoRoot)." }
Refresh-Env
$tools = Test-Tools
Show-Tools $tools
$missing = @($tools.Keys | Where-Object { -not $tools[$_] })

if ($CheckOnly) {
  if ($missing.Count -eq 0) { Write-Host "`nEverything needed is installed." -ForegroundColor Green }
  else { Write-Host "`nMissing: $($missing -join ', '). Run without -CheckOnly to install." -ForegroundColor Yellow }
  if (Test-Path $Exe) { Write-Host "Built app: $Exe" } else { Write-Host 'App not built yet.' }
  exit 0
}

if ($missing.Count -gt 0) {
  if ($SkipInstall) { Fail "Missing: $($missing -join ', '). Re-run without -SkipInstall." }
  if (-not (Get-Command winget -ErrorAction SilentlyContinue)) {
    Fail 'winget is not available. Install "App Installer" from the Microsoft Store, then re-run.'
  }
  Step "Installing with winget: $($missing -join ', ')"
  foreach ($k in $missing) {
    $spec = $WingetIds[$k]
    $id = $spec[0]
    $extra = @()
    if ($spec.Count -gt 1) { $extra = $spec[1..($spec.Count - 1)] }
    Write-Host "    installing $id ..."
    & winget install --id $id -e --source winget --accept-source-agreements --accept-package-agreements @extra
    # winget returns non-zero for "already installed / no upgrade"; the re-check below is what counts.
  }
  Refresh-Env
  if (-not (Get-Command rustc -ErrorAction SilentlyContinue) -and (Get-Command rustup -ErrorAction SilentlyContinue)) {
    Invoke-Native 'rustup default stable' { rustup default stable }
  }
  $tools = Test-Tools
  Show-Tools $tools
  $still = @($tools.Keys | Where-Object { -not $tools[$_] })
  if ($still.Count -gt 0) {
    Fail ("Still missing after install: $($still -join ', '). Close this window, open a new PowerShell " +
          "and run the script again. If 'msvc' stays missing, install Visual Studio Build Tools by hand " +
          'with the "Desktop development with C++" workload.')
  }
}

$Bun = Find-Bun
$Bash = Find-GitBash
if (-not $Bash) { Fail 'Git Bash not found (needed for the model download script). Reinstall Git for Windows.' }

# ---------------------------------------------------------------------------
if ($SkipModel) {
  Step 'Skipping the Sortformer speaker model (-SkipModel); the app falls back to the older diarizer'
} else {
  Step 'Fetching the Sortformer speaker model (139 MB, verified by sha256)'
  Push-Location $RepoRoot
  try { Invoke-Native 'Model download' { & $Bash scripts/fetch-diarization-model.sh } }
  finally { Pop-Location }
}

# ---------------------------------------------------------------------------
Step 'Installing JavaScript dependencies (bun install)'
Push-Location $Handy
try { Invoke-Native 'bun install' { & $Bun install } }
finally { Pop-Location }

Step 'Building the app (first time about 5-10 minutes)'
# --no-bundle: tauri.conf.json signs the installer with Azure Trusted Signing,
# which only exists in CI. The exe itself is complete without the bundle.
Push-Location $Handy
try { Invoke-Native 'Build' { & $Bun tauri build --debug --no-bundle } }
finally { Pop-Location }
if (-not (Test-Path $Exe)) { Fail "Build finished but $Exe is missing." }
Ok $Exe

# ---------------------------------------------------------------------------
if ($Test) {
  Step 'Running the Rust tests (cargo test --lib)'
  Push-Location $Tauri
  try { Invoke-Native 'Rust tests' { cargo test --lib } }
  finally { Pop-Location }
  Step 'Running the MCP server tests (bun test)'
  Push-Location (Join-Path $Handy 'mcp')
  try {
    Invoke-Native 'bun install (mcp)' { & $Bun install }
    Invoke-Native 'MCP tests' { & $Bun test }
  }
  finally { Pop-Location }
}

# ---------------------------------------------------------------------------
if (-not $NoShortcut) {
  Step 'Creating the desktop shortcut'
  $lnk = Join-Path ([Environment]::GetFolderPath('Desktop')) 'Plaudy.lnk'
  $shell = New-Object -ComObject WScript.Shell
  $s = $shell.CreateShortcut($lnk)
  $s.TargetPath = $Exe
  $s.WorkingDirectory = Split-Path $Exe
  $s.IconLocation = "$Exe,0"
  $s.Description = 'Plaudy meeting recorder'
  $s.Save()
  Ok $lnk
}

$mins = [math]::Round(((Get-Date) - $started).TotalMinutes, 1)
Write-Host "`nDone in $mins min." -ForegroundColor Green
Write-Host "Start Plaudy: $Exe"
Write-Host 'First launch: download a speech-to-text model from inside the app (Settings > Models).'
Write-Host 'To bring meetings from another PC: close the app on both, then copy'
Write-Host '%APPDATA%\com.uppify.plaudy\ (history.db + recordings\) to the same place here.'
