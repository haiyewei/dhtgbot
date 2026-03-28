# dhtgbot-rs

[中文](./README.md)

`dhtgbot-rs` is a pure Rust Telegram bot project that coordinates multiple bots and external services for the following tasks:

- `master`: administration, backup, restore, debugging
- `tdl`: watches messages and forwards supported links to `tdlr`
- `xdl`: queries X/Twitter content, monitors likes, tracks authors, downloads media, and uploads through `tdlr`

The project itself is a single binary called `dhtgbot`, but it depends on three external tools at runtime:

- `amagi`: Twitter/X bridge service
- `tdlr`: Telegram upload and forward service
- `aria2`: download engine

## Features

- Pure Rust rewrite
- Configuration-driven, no hardcoded local absolute paths
- Unified global serial task queue
- SQLite storage with the legacy `key/value + {"value": ...}` model restored
- Database backup and restore via `master`
- External services launched from configurable commands
- Release packages ship with installer scripts

## Layout

```text
.
├── src/                   Rust sources
├── scripts/               installation scripts
├── .github/workflows/     CI / daily / release
├── config.example.yaml    example config
└── config.yaml            real runtime config (generated locally, not committed)
```

## Installation

### Windows

The release package includes:

- `scripts/install.ps1`

It will:

1. Download and install `amagi`
2. Download and install `tdlr`
3. Download and install `aria2` 1.37.0
4. Download or install `dhtgbot`
5. Create the app home, default config, launcher entry, and add commands to the user `PATH`

If `amagi`, `tdlr`, or `aria2c` already exists in the environment, the script downloads the package first and then asks whether it should overwrite the existing installation. The default answer is no.

Example:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

Remote execution:

```powershell
$tmp = Join-Path $env:TEMP "dhtgbot-install.ps1"
Invoke-WebRequest "https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install.ps1" -OutFile $tmp
powershell -ExecutionPolicy Bypass -File $tmp
```

### Linux / macOS

The release package includes:

- `scripts/install.sh`
- `scripts/install-systemd.sh` (Linux only)

Regular install:

```bash
bash ./scripts/install.sh
```

Remote execution:

```bash
curl -fsSL https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install.sh | bash
```

Install as a `systemd` service on Linux:

```bash
bash ./scripts/install-systemd.sh
```

Remote execution:

```bash
curl -fsSL https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install-systemd.sh | bash
```

`install.sh` will:

1. Download and install `amagi`
2. Download and install `tdlr`
3. Download the `aria2` 1.37.0 source archive and install it into the user environment
4. Download or install `dhtgbot`
5. Create the app home, default config, launcher entry, and add commands to the user `PATH`

Notes:

- On Linux and macOS, `aria2` currently uses source installation, so a basic build toolchain is required
- Existing `amagi`, `tdlr`, or `aria2c` installations are not overwritten automatically

### Overwrite policy

The installer supports:

```text
DHTGBOT_INSTALL_OVERWRITE=prompt   # default
DHTGBOT_INSTALL_OVERWRITE=always
DHTGBOT_INSTALL_OVERWRITE=never
```

### Remote binary source selection

Installer scripts download binaries from the matching workflow output based on version selection:

- default: `latest`
  downloads the latest stable GitHub Release
- `DHTGBOT_INSTALL_VERSION=daily`
  downloads assets published by the `Daily Build` workflow under the `daily` tag
- `DHTGBOT_INSTALL_VERSION=v0.1.0`
  downloads assets from a specific tagged Release

Dependencies follow the same pattern:

- `amagi` comes from the `amagi-rs` GitHub Release
- `tdlr` comes from the `tdlr` GitHub Release
- `aria2` comes from the official `release-1.37.0` GitHub Release

## Configuration

After installation, `config.yaml` is created in:

- Windows: `%LOCALAPPDATA%\Programs\dhtgbot\app\config.yaml`
- Linux/macOS: `~/.local/share/dhtgbot/config.yaml`

Start from [config.example.yaml](./config.example.yaml).

Important fields:

- `bots.master.base.token`
- `bots.master.admins`
- `bots.tdl.base.token`
- `bots.xdl.base.token`
- `bots.xdl.twitter.cookies`
- `services.amagi.base_url`
- `services.amagi.start_command`
- `services.tdlr.base_url`
- `services.tdlr.start_command`
- `services.aria2.rpc_url`
- `services.aria2.start_command`

Notes:

- It is recommended to wrap `bots.xdl.twitter.cookies` in single quotes
- `start_command` should use environment commands, not hardcoded absolute paths
- The program always reads `config.yaml` from the current working directory
- The generated launcher switches into the app home automatically

## Bots

### master

Main commands:

- `/help`
- `/backup`
- `/backup_status`
- `/restore`
- `/restore_cancel`
- `/restore_nozip`
- `/echo`
- `/mdata`

The restore flow is:

- upload ZIP
- provide ZIP password, or use `/restore_nozip`
- provide import password
- read `.sql` from the ZIP
- replace the SQLite database with the imported dump

### tdl

Watches configured chats/topics and forwards supported links to `tdlr`.

Main commands:

- `/help`
- `/version`
- `/forward`

### xdl

Provides X/Twitter related features.

Main commands:

- `/profile`
- `/tweet`
- `/tweets`
- `/search`
- `/tweetdl`
- `/tweet_like_dl`
- `/author_track`

Supported workflows:

- single tweet lookup
- media download
- likes polling
- author tracking
- media upload through `tdlr`

## Data model

The storage layer has been restored to the legacy schema:

- table shape: `bot_xxx(key, value)`
- value shape: `{"value": ...}`

Missing tables are created automatically at startup, and known old/intermediate row formats are normalized on boot.

## Development

```bash
cargo fmt
cargo test --locked --workspace
cargo run
```

Startup sequence:

1. read `config.yaml`
2. initialize SQLite
3. start or connect to `amagi`, `tdlr`, and `aria2`
4. run enabled bots

## Release

The repository contains three workflows:

- `Cargo CI`
- `Daily Build`
- `Release`

Release packages contain:

- `dhtgbot`
- `config.example.yaml`
- installer scripts
- Linux `systemd` installer

The installer flow supports both:

- local execution from a downloaded release package
- remote execution from GitHub Raw
- remote binary download from workflow-published assets

## Status

This is an operational repository, not a generic SDK. The main focus is:

- behavior parity
- explicit configuration
- fixed runtime directory
- complete installation flow
- easier future extension for additional bots
