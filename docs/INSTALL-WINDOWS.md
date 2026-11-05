# Install Plaudy on Windows

There is no signed installer yet (story WIN-BUILD-001), so Plaudy is built
from source. One script does it all.

## The short way

Open PowerShell (a normal window, not as administrator) and run:

```powershell
winget install --id Git.Git -e        # only if git is missing; then open a NEW window
cd $HOME\Documents
git clone https://github.com/ValerioJiang/plaudy
cd plaudy
powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1
```

The script:

1. checks the toolchain and installs what is missing with winget: Git, Rust,
   CMake, Bun, the Vulkan SDK, and Visual Studio Build Tools with the C++ workload
   (these two ask for admin rights once);
2. downloads the Sortformer speaker model (139 MB, checked against its sha256);
3. runs `bun install` and `bun tauri build --debug --no-bundle`;
4. puts a **Plaudy** shortcut on the desktop.

It is safe to run again; finished steps are skipped. The first build takes
about 5-10 minutes; a rebuild about one.

| Option | Effect |
|---|---|
| `-CheckOnly` | Only report what is installed or missing; change nothing |
| `-SkipInstall` | Never call winget |
| `-SkipModel` | Skip the speaker model (the app falls back to the older diarizer) |
| `-Test` | Also run the Rust and MCP test suites |
| `-NoShortcut` | No desktop shortcut |

If it stops with "Still missing after install", close the window, open a new
PowerShell and run the script again: some installers only update PATH for new
windows.

## After the install

- **Speech model:** on first launch, download a speech-to-text model in the app
  (Settings, Models).
- **Meetings from another PC:** close Plaudy on both machines, then copy
  `%APPDATA%\com.uppify.plaudy\` (`history.db` and `recordings\`) to the same
  place on the new PC.
- **Updating later:** `git pull`, then run the script again.
- `git status` may show `handy/src-tauri/Cargo.toml` as modified after a build:
  the Tauri CLI re-saves it with different line endings. `git checkout --
  handy/src-tauri/Cargo.toml` restores it.

## Let Claude Code do it

On the new PC, install [Claude Code](https://claude.com/claude-code), open it
in your Documents folder and paste:

```text
Install the Plaudy meeting recorder on this Windows PC.

1. If git is missing, install it with `winget install --id Git.Git -e`.
2. Clone https://github.com/ValerioJiang/plaudy into %USERPROFILE%\Documents\plaudy
   (or `git pull` if it is already there).
3. Read docs/INSTALL-WINDOWS.md, then run from the repo root:
     powershell -ExecutionPolicy Bypass -File scripts\setup-windows.ps1 -Test
   The build takes 5-10 minutes the first time; use a long timeout. Visual Studio
   Build Tools and the Vulkan SDK show a UAC prompt: tell me to accept it.
4. If the script stops, read its error, fix the cause (docs/adlc/BASELINE-WINDOWS.md
   describes a working reference build) and run it again. Stop and ask me if the
   same step fails twice.
5. Do not commit, push or edit source files: this is an install, not development.
6. Report: tool versions, GPU (`vulkaninfo --summary` if present), build time, test
   results, and the path of the built handy.exe.
```

## What gets installed where

| Thing | Where |
|---|---|
| The app | `plaudy\handy\src-tauri\target\debug\handy.exe` (plus DLLs next to it) |
| Your data | `%APPDATA%\com.uppify.plaudy\` |
| Toolchain | Normal winget locations; Rust in `%USERPROFILE%\.cargo` |

Speech-to-text runs on the GPU through Vulkan when a driver is present, and on
the CPU otherwise.
