# dhtgbot-rs

[中文](https://github.com/haiyewei/dhtgbot/blob/master/README.md)

[GitHub Repository](https://github.com/haiyewei/dhtgbot) | [Releases](https://github.com/haiyewei/dhtgbot/releases) | [Docker Hub](https://hub.docker.com/r/haiyewei/dhtgbot) | [GHCR](https://github.com/haiyewei/dhtgbot/pkgs/container/dhtgbot)

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
├── docker/                container entry scripts
├── scripts/               installation scripts
├── .github/workflows/     CI / daily / release
├── config.example.yaml    example config
├── config.example.docker.yaml Docker example config
├── Dockerfile             container build file
├── compose.yaml           local / published image runtime example
└── config.yaml            real runtime config (copied locally from the example, not committed)
```

## Installation

### Windows

The release package includes:

- `scripts/install.ps1`
- `scripts/uninstall.ps1`
- `scripts/upgrade.ps1`

It will:

1. Download and install `amagi`
2. Download and install `tdlr`
3. Download and install `aria2` 1.37.0
4. Download or install `dhtgbot`
5. Create the app home, example config, launcher entry, and add commands to the user `PATH`

By default the runtime is installed under `%LOCALAPPDATA%\Programs\dhtgbot\app`.
Dependencies are installed into their own environment-command directories, for example `%LOCALAPPDATA%\Programs\amagi\bin`, `%LOCALAPPDATA%\Programs\tdlr\bin`, and `%LOCALAPPDATA%\Programs\aria2\bin`, and those directories are added to `PATH`.

If `amagi`, `tdlr`, or `aria2c` already exists in the environment, the script downloads the package first and then asks whether it should overwrite the existing installation. The default answer is no.
The script only uses existing binaries: it either reuses a local prebuilt `dhtgbot.exe` or downloads GitHub Release assets directly. It does not run `cargo build`.

Example:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

Remote execution:

```powershell
irm https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install.ps1 | iex
```

Upgrade an existing installation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\upgrade.ps1
```

Uninstall an existing installation:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\uninstall.ps1
```

`upgrade.ps1` first tries to detect an existing install from the current directory, `.\dhtgbot` under the current directory, `%LOCALAPPDATA%\Programs\dhtgbot\app`, and an existing `dhtgbot` launcher found in `PATH`.

Remote upgrade:

```powershell
irm https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/upgrade.ps1 | iex
```

By default it upgrades all runtime binaries:

- `dhtgbot`
- `amagi`
- `tdlr`
- `aria2`

If you only want to upgrade the main program, add `-SkipDependencies`.

If you need options for remote execution, use environment variables:

```powershell
$env:DHTGBOT_INSTALL_VERSION = "v0.2.3"
$env:DHTGBOT_INSTALL_SKIP_DEPENDENCIES = "1"
$env:DHTGBOT_INSTALL_PROXY = "1"
irm https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install.ps1 | iex
```

### Linux / macOS

The release package includes:

- `scripts/install.sh`
- `scripts/uninstall.sh`
- `scripts/upgrade.sh`
- `scripts/install-systemd.sh` (Linux only)

Regular install:

```bash
bash ./scripts/install.sh
```

Remote execution:

```bash
curl -fsSL https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install.sh | bash
```

Upgrade an existing installation:

```bash
bash ./scripts/upgrade.sh
```

Uninstall an existing installation:

```bash
bash ./scripts/uninstall.sh
```

Remote upgrade:

```bash
curl -fsSL https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/upgrade.sh | bash
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
3. Download and install a prebuilt `aria2` package
4. Extract the `dhtgbot` release package into `./dhtgbot` under the current directory
5. Keep `./dhtgbot/config.example.yaml` as the reference file
6. Prompt you to copy it to `./dhtgbot/config.yaml` and continue the remaining configuration work

Notes:

- `install.sh` only uses existing binaries. If no prebuilt local `dhtgbot` is present, it falls back to the remote Release asset instead of running `cargo build`
- On Linux, `aria2` is downloaded from the prebuilt `abcfy2/aria2-static-build` GitHub Release: <https://github.com/abcfy2/aria2-static-build/releases>
- On macOS, `aria2` is installed with `brew install aria2`
- If `brew` is not available on macOS, the script fails fast and tells the user to install Homebrew first: <https://brew.sh/>
- Existing `amagi`, `tdlr`, or `aria2c` installations prompt for overwrite and default to keeping the current binary
- Remote `curl ... | bash` runs are non-interactive; if `amagi`, `tdlr`, or `aria2c` already exists in the environment, the installer keeps the existing binaries. Use `DHTGBOT_INSTALL_OVERWRITE=always` or `bash ./scripts/upgrade.sh` to replace them
- `install.sh` installs dependencies into environment command directories and writes those directories into the shell profile; when `fish` is detected it also writes a startup snippet under `~/.config/fish/conf.d/`, so `config.yaml` should use plain `amagi` / `tdlr` / `aria2c` commands in `start_command`
- After remote extraction, the original archive is deleted, so the project directory is not left with `*.tar.gz`
- `install.sh` does not install `dhtgbot` itself into `PATH` by default
- after `install.sh` finishes successfully, it opens an interactive shell inside the project directory; exiting that shell returns to the previous location
- if you need the old runtime-style behavior, use `bash ./scripts/install.sh --layout runtime`
- `upgrade.sh` upgrades `dhtgbot`, `amagi`, `tdlr`, and `aria2` by default and forces replacement of the existing binaries
- `upgrade.sh` first tries to detect an existing install from the current directory, the script directory, and `~/.local/share/dhtgbot`; you can also pass `--workspace-dir` or `--home-dir` explicitly
- `upgrade.sh` also tries to infer the existing install root from a `dhtgbot` command already available in `PATH`
- `uninstall.sh` / `uninstall.ps1` first remove `dhtgbot`, then ask one by one whether `amagi`, `tdlr`, and `aria2` should also be uninstalled; in non-interactive environments they are preserved by default
- `install-systemd.sh` first checks whether it is running from a `scripts/` directory inside an existing project/workspace
- when it is executed locally from an existing workspace, it reuses `../` as the `systemd` working directory and creates the background service from that local `dhtgbot` binary
- when it is executed remotely, or when no existing workspace is detected, it falls back to the runtime-layout install, places the main program in the service user's app directory, and writes the dependency command directories into the `systemd` service `PATH`

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
  downloads the latest stable GitHub Release: <https://github.com/haiyewei/dhtgbot/releases/latest>
- `DHTGBOT_INSTALL_VERSION=daily`
  downloads assets published by the `Daily Build` workflow under the `daily` tag: <https://github.com/haiyewei/dhtgbot/releases/tag/daily>
- `DHTGBOT_INSTALL_VERSION=v0.2.3`
  downloads assets from a specific tagged Release, for example: <https://github.com/haiyewei/dhtgbot/releases/tag/v0.2.3>

Dependencies follow the same pattern:

- `amagi` comes from the `amagi-rs` GitHub Release: <https://github.com/bandange/amagi-rs/releases>
- `tdlr` comes from the `tdlr` GitHub Release: <https://github.com/haiyewei/tdlr/releases>
- On Windows, `aria2` comes from the official `release-1.37.0` GitHub Release: <https://github.com/aria2/aria2/releases/tag/release-1.37.0>
- On Linux, `aria2` comes from the `abcfy2/aria2-static-build` GitHub Release `1.37.0`: <https://github.com/abcfy2/aria2-static-build/releases/tag/1.37.0>
- On macOS, `aria2` is installed through Homebrew: <https://formulae.brew.sh/formula/aria2>

## Docker

The repository now ships a complete container setup. The image includes:

- `dhtgbot`
- musl Linux `amagi`
- musl Linux `tdlr`
- `aria2`

The container base image is Alpine, and the Rust binary chain is aligned on `musl`:

- `dhtgbot`: downloaded from the GitHub Release asset `dhtgbot-*-unknown-linux-musl.tar.gz`
- `amagi`: downloaded from `*-unknown-linux-musl` release assets
- `tdlr`: downloaded from `*-unknown-linux-musl` release assets
- `aria2`: installed from Alpine packages

At runtime the container uses `/var/lib/dhtgbot` as its working directory, and the program ultimately reads `/var/lib/dhtgbot/config.yaml`. The entrypoint switches into that directory before starting `dhtgbot`.

Recommended initialization flow:

```bash
docker pull docker.io/haiyewei/dhtgbot:latest
docker compose run --rm dhtgbot init
```

That `init` step will:

- create `./.docker-data/config.yaml`
- reuse the file if it already exists without overwriting it
- print the runtime directory, config path, and next-step hints

Then edit this file on the host:

```bash
./.docker-data/config.yaml
```

At minimum, replace these placeholder values first:

- `bots.master.token`
- `bots.tdl.token` / `bots.xdl.token` when those bots are enabled
- `bots.tdl.forward.account` / `bots.xdl.account`
- `bots.xdl.twitter.cookies` when X/Twitter features are enabled

Start the service only after the config is ready:

```bash
docker compose up -d
```

Local image builds follow the same flow:

```bash
docker compose up -d --build
```

You can also use GHCR directly:

```bash
docker pull ghcr.io/haiyewei/dhtgbot:latest
docker run --rm -v "$PWD/.docker-data:/var/lib/dhtgbot" ghcr.io/haiyewei/dhtgbot:latest help
docker run --rm -v "$PWD/.docker-data:/var/lib/dhtgbot" ghcr.io/haiyewei/dhtgbot:latest init
```

The container also provides these helper commands:

- `help`: show container startup help
- `init`: initialize the runtime directory and create `config.yaml`
- `config-path`: print the in-container config path
- `show-config`: print the current runtime `config.yaml`
- `example-config`: print the bundled Docker config template

Notes:

- [compose.yaml](https://github.com/haiyewei/dhtgbot/blob/master/compose.yaml) stores runtime data in `./.docker-data`
- after `docker compose run --rm dhtgbot init`, edit `./.docker-data/config.yaml` directly on the host
- if `config.yaml` still contains template placeholders, the entrypoint prints setup guidance and refuses to start the main program
- the container exposes `4567`, `8787`, and `6800`
- the Docker-specific config template switches `amagi`, `tdlr`, and `aria2` to container-friendly listen flags
- the Dockerfile tracks the latest GitHub Release for `dhtgbot`, `amagi-rs`, and `tdlr` by default; override `DHTGBOT_VERSION` / `AMAGI_VERSION` / `TDLR_VERSION` at build time if you need a pinned version
- `dhtgbot` still talks to those services through `127.0.0.1`, so the internal behavior matches the local process model
- the Docker setup no longer depends on Debian / glibc
- the `Docker Publish` workflow syncs the repository `README.md` to the Docker Hub Overview: <https://hub.docker.com/r/haiyewei/dhtgbot>

## Configuration

After installation, the runtime directory includes:

- Windows: `%LOCALAPPDATA%\Programs\dhtgbot\app\config.example.yaml`
- Linux/macOS: `~/.local/share/dhtgbot/config.example.yaml`

Copy [config.example.yaml](https://github.com/haiyewei/dhtgbot/blob/master/config.example.yaml) to `config.yaml` before editing it.

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
- `bots.xdl.twitter.cookies` is forwarded to `amagi` on each request through the `X-Amagi-Twitter-Cookie` header, so it does not rely on a cookie bound at `amagi` startup
- `bots.tdl.forward.account` and `bots.xdl.account` should be the numeric `user_id` of a `tdlr` account because `account` is now sent through the structured HTTP API
- `dhtgbot` now talks to `tdlr service --http-bind ...` through `GET /v1/version`, `POST /v1/forwards`, and `POST /v1/uploads`, not the legacy `/execute` cliapi
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

- [Cargo CI](https://github.com/haiyewei/dhtgbot/actions/workflows/cargo.yml)
- [Daily Build](https://github.com/haiyewei/dhtgbot/actions/workflows/daily.yml)
- [Release](https://github.com/haiyewei/dhtgbot/actions/workflows/release.yml)
- [Docker Publish](https://github.com/haiyewei/dhtgbot/actions/workflows/docker.yml)

Release packages contain:

- `dhtgbot`
- `config.example.yaml`
- installer scripts
- Linux `systemd` installer

The installer flow supports both:

- local execution from a downloaded release package
- remote execution from GitHub Raw
- remote binary download from workflow-published assets

Docker images are published to:

- `docker.io/haiyewei/dhtgbot:latest`
- `docker.io/haiyewei/dhtgbot:vX.Y.Z`
- `docker.io/haiyewei/dhtgbot:sha-<commit>`
- `ghcr.io/haiyewei/dhtgbot:latest`
- `ghcr.io/haiyewei/dhtgbot:vX.Y.Z`
- `ghcr.io/haiyewei/dhtgbot:sha-<commit>`

Registry pages:

- Docker Hub: <https://hub.docker.com/r/haiyewei/dhtgbot>
- GHCR: <https://github.com/haiyewei/dhtgbot/pkgs/container/dhtgbot>

The `Docker Publish` workflow requires these GitHub Secrets:

- `DOCKERHUB_USERNAME`
- `DOCKERHUB_TOKEN`

`DOCKERHUB_TOKEN` should be a Docker Hub access token, not your account password.

GHCR does not require an extra custom secret. The workflow uses the built-in:

- `GITHUB_TOKEN`

The workflow must keep `packages: write` permission enabled.

## Status

This is an operational repository, not a generic SDK. The main focus is:

- behavior parity
- explicit configuration
- fixed runtime directory
- complete installation flow
- easier future extension for additional bots
