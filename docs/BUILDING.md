# Building & Releasing

Developer notes for Space++ (Windows). End users don't need any of this —
grab a prebuilt exe from the [Releases](https://github.com/zzqq2199/SuperSpaceWin/releases) page.

## Prerequisites

- [Rust toolchain](https://rustup.rs/) (stable). Either the MSVC or GNU target works.
- Windows 10/11.

## Build

```powershell
cargo build --release
.\target\release\spacepp-win.exe
```

The result is a single static exe (~340 KB) with no runtime dependencies.
The default `config.json` is embedded at compile time, so the exe is fully
functional even when copied somewhere on its own.

## Test

The state machine and config parsing are pure logic modules with no OS calls,
so they run as ordinary unit tests:

```powershell
cargo test
```

## Publish to `publish/`

`scripts/publish.ps1` assembles a ready-to-run folder:

```powershell
.\scripts\publish.ps1
.\publish\spacepp-win.exe
```

It builds the release exe, copies it to `publish\spacepp-win.exe`, and creates
`publish\config.json` as a **relative symlink** to the repo-root `config.json`
(so editing the repo config takes effect immediately after a restart).

Notes:

- Creating the symlink requires Windows Developer Mode (Settings → System → For
  developers) or running as administrator.
- `publish\*.exe` is git-ignored; the `config.json` symlink itself is tracked.
  After cloning, run `git config core.symlinks true` so the link checks out
  correctly.
- If the published exe is running (and therefore locked), the script renames it
  to `spacepp-win.exe.old` and writes the fresh build alongside; restart the app
  to pick it up.

## Configuration resolution order

At startup the config is resolved in this order:

1. `SPACEPP_CONFIG` environment variable (if it points to an existing file)
2. `config.json` next to the executable
3. `config.json` in the current working directory
4. the embedded default (full mapping) if none of the above exist

## Logging

When any `verbose` flag is enabled in `config.json`, logs are appended to
`%TEMP%\spacepp.log`. With verbose off, only lifecycle lines (start, hook
ready, exit) are written. See the privacy note below.

> **Privacy:** `verbose.on_event` records every physical key code — treat it as
> a keylogger and only enable it for short debugging sessions, then delete the
> log. `on_state` and `on_action` do not record typed content.

## GitHub Actions release

`.github/workflows/release.yml` runs on every push to `main` (and via manual
dispatch):

1. `cargo test` on a Windows runner
2. `cargo build --release`
3. Package a zip (exe + `config.json` + `README.md`) and upload it as an
   Actions artifact
4. If the version in `Cargo.toml` has no matching `v<version>` tag yet, create a
   GitHub Release and attach the zip

Pushing the same version again only refreshes the artifact; it does not create a
duplicate release. **To cut a new release, bump `version` in `Cargo.toml`** and
push to `main`.

## Project layout

```
src/
  main.rs           # wiring: hook, tray, window loop, lock guard
  state_machine.rs  # the pure Hyper state machine + unit tests
  config.rs         # config.json parsing, key-name resolution, embedded default
  key_codes.rs      # key-name -> Windows virtual-key mapping
  keyboard.rs       # WH_KEYBOARD_LL hook + SendInput injection (tagged)
  foreground.rs     # foreground-process lookup for the blacklist
  lock_guard.rs     # Win+L policy guard + proxy lock
  tray.rs           # tray icon rendering + context menu
  autostart.rs      # HKCU Run registry toggle
  logger.rs         # file logger
icons/              # source SVGs the tray icons are drawn from
scripts/publish.ps1 # publish/ assembler
```
