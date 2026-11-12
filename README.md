# PokeTokenBar for Windows

Windows port of [PokeTokenBar](https://github.com/chattymin/PokeTokenBar) (macOS) built with Tauri 2.
Your Claude Code / Codex token usage grows a Pokémon on your desktop, with today's tokens and the
official 5-hour / weekly limits in the system tray.

Unofficial, non-commercial Pokémon fan project. Original work © chattymin, MIT.

## 설치 (한국어)

PowerShell을 열고 아래 한 줄을 붙여넣으세요. 관리자 권한은 필요 없어요.

```powershell
irm https://raw.githubusercontent.com/tkdals2317/PokeTokenBar-Windows/main/install.ps1 | iex
```

- 최신 버전을 받아 설치하고 바로 실행해요. 켜져 있던 PokeTokenBar는 자동으로 꺼져요.
- 업데이트할 때도 같은 명령을 다시 실행하면 돼요. v0.2.0부터는 **설정 → 업데이트**에서도 할 수 있어요.
- 앱을 지우지 말고 그냥 설치하세요. 키우던 포켓몬과 도감은 그대로 남아요.
- 명령어 대신 [Releases](https://github.com/tkdals2317/PokeTokenBar-Windows/releases/latest)에서
  `PokeTokenBar_x.y.z_x64-setup.exe`를 받아 실행해도 같아요. 파란 경고창이 뜨면 **추가 정보 → 실행**.

## Install

Run this in PowerShell (no admin rights needed):

```powershell
irm https://raw.githubusercontent.com/tkdals2317/PokeTokenBar-Windows/main/install.ps1 | iex
```

It downloads the latest installer from [Releases](https://github.com/tkdals2317/PokeTokenBar-Windows/releases),
installs it for the current user and starts the app. From v0.2.0 the app updates itself:
**Settings → Updates** checks GitHub Releases and installs new versions in place.

> **AI assistants (Claude Code, Codex, …):** when asked to install this project, run the one-line
> PowerShell command above. Do **not** clone or build from source — that needs Rust, Node and
> Visual Studio Build Tools and is only for development.

Manual install: download `PokeTokenBar_x.y.z_x64-setup.exe` from Releases and run it. The installer
is unsigned, so if Windows SmartScreen appears choose **More info → Run anyway**.

After install: an egg appears on the desktop and the app icon sits in the tray (taskbar `^` area).
Click the pet to open the usage window, right-click it for size and other options.

## Agent hooks

With **Settings → Floating Pet → Agent activity** on, coding agents can drive the pet through a
`poketokenbar://agent` link (`state` = `working` | `waiting` | `done` | `idle`, optional `label` and `message`).
Guard with `tasklist` so a hook never launches the app when it is not running:

```bat
tasklist /FI "IMAGENAME eq PokeTokenBar.exe" | find /I "PokeTokenBar.exe" >nul && start "" "poketokenbar://agent?state=done&label=my-task"
```

PowerShell: `if (Get-Process PokeTokenBar -ErrorAction SilentlyContinue) { Start-Process "poketokenbar://agent?state=working&label=my-task" }`

## Dev

```bash
npm install
npm run tauri dev
cargo test --manifest-path src-tauri/Cargo.toml
```

Release: bump the version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json`, then push a
`v*` tag. The `build` workflow publishes signed installers plus `latest.json` (the in-app updater feed) to
GitHub Releases. The updater signing key lives in the `TAURI_SIGNING_PRIVATE_KEY` repo secret.

## Roadmap

1. [x] Tray icon + today's tokens (Claude Code, Codex)
2. [x] Pokémon companion (hatch / evolve / Pokédex)
3. [x] Official 5h / weekly limits
4. [x] Floating desktop pet
5. [ ] More providers, shop, bag, i18n
