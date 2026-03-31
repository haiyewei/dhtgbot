# dhtgbot-rs

[English](https://github.com/haiyewei/dhtgbot/blob/master/README.en.md)

[GitHub 仓库](https://github.com/haiyewei/dhtgbot) | [Releases](https://github.com/haiyewei/dhtgbot/releases) | [Docker Hub](https://hub.docker.com/r/haiyewei/dhtgbot) | [GHCR](https://github.com/haiyewei/dhtgbot/pkgs/container/dhtgbot)

`dhtgbot-rs` 是一个纯 Rust 的 Telegram Bot 项目，用来协调多个机器人和外部服务，处理以下几类工作：

- `master`：管理、备份、恢复、调试
- `tdl`：监听消息并把符合规则的链接转发给 `tdlr`
- `xdl`：查询 X/Twitter 内容、监听点赞、跟踪作者、下载媒体并通过 `tdlr` 上传

项目本身是一个单二进制程序 `dhtgbot`，但运行时依赖三个外部组件：

- `amagi`：提供 Twitter/X 抓取桥接服务
- `tdlr`：负责 Telegram 侧上传和转发
- `aria2`：负责媒体下载

## 特性

- 纯 Rust 重写，根目录即 Rust 项目
- 配置驱动，不硬编码本机绝对路径
- 统一的串行全局任务队列
- SQLite 存储，数据模型已恢复为旧版 `key/value + {"value": ...}` 形式
- `master` 支持数据库备份与恢复
- 支持将 `amagi`、`tdlr`、`aria2` 作为外部命令启动
- 发布包自带安装脚本

## 目录

```text
.
├── src/                   Rust 源码
├── docker/                容器入口脚本
├── scripts/               安装脚本
├── .github/workflows/     CI / daily / release
├── config.example.yaml    示例配置
├── config.example.docker.yaml Docker 示例配置
├── Dockerfile             容器镜像构建文件
├── compose.yaml           本地 / 远程镜像运行示例
└── config.yaml            实际运行配置（从示例复制后本地维护，不提交）
```

## 安装

### Windows

发布包内置：

- `scripts/install.ps1`

它会执行以下操作：

1. 下载并安装 `amagi`
2. 下载并安装 `tdlr`
3. 下载并安装 `aria2` 1.37.0
4. 下载或安装 `dhtgbot`
5. 创建应用目录、示例配置、启动入口，并把命令加入用户 `PATH`

如果环境里已经存在 `amagi`、`tdlr` 或 `aria2c`，脚本会先下载，再询问是否覆盖，默认不覆盖。

示例：

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

远程执行：

```powershell
irm https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install.ps1 | iex
```

如果需要给远程执行传参，使用环境变量：

```powershell
$env:DHTGBOT_INSTALL_VERSION = "v0.2.2"
$env:DHTGBOT_INSTALL_SKIP_DEPENDENCIES = "1"
$env:DHTGBOT_INSTALL_PROXY = "1"
irm https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install.ps1 | iex
```

### Linux / macOS

发布包内置：

- `scripts/install.sh`
- `scripts/install-systemd.sh`（仅 Linux）

普通安装：

```bash
bash ./scripts/install.sh
```

远程执行：

```bash
curl -fsSL https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install.sh | bash
```

Linux 安装为 `systemd` 服务：

```bash
bash ./scripts/install-systemd.sh
```

远程执行：

```bash
curl -fsSL https://raw.githubusercontent.com/haiyewei/dhtgbot/master/scripts/install-systemd.sh | bash
```

`install.sh` 会：

1. 下载并安装 `amagi`
2. 下载并安装 `tdlr`
3. 下载 `aria2` 1.37.0 源码包并安装到用户环境
4. 把 `dhtgbot` 发布包解压到当前目录下的 `./dhtgbot`
5. 保留 `./dhtgbot/config.example.yaml` 作为参考
6. 提示复制为 `./dhtgbot/config.yaml` 并继续配置主程序和下属软件

说明：

- Linux/macOS 上的 `aria2` 当前走源码安装，因此需要基础编译环境
- 已存在的 `amagi`、`tdlr`、`aria2c` 会先下载，再询问是否覆盖，默认不覆盖
- 远程安装解压完成后会删除原始压缩包，不会把 `*.tar.gz` 留在项目目录里
- `install.sh` 默认不会把 `dhtgbot` 自己装进 `PATH`
- `install.sh` 成功结束后会打开一个位于项目目录中的交互 shell；退出该 shell 会回到原来的位置
- 如果需要旧的“运行时安装”行为，可使用 `bash ./scripts/install.sh --layout runtime`
- `install-systemd.sh` 会先判断自己是否位于现有项目目录的 `scripts/` 下
- 如果是本地现有目录执行，它会直接复用 `../` 作为 `systemd` 的工作目录，并用该目录里的 `dhtgbot` 二进制创建后台服务
- 如果是远程执行或未检测到现有目录，它才会继续走运行时布局安装，把主程序安装到服务用户的应用目录中

### 覆盖策略

安装脚本支持环境变量：

```text
DHTGBOT_INSTALL_OVERWRITE=prompt   # 默认，交互提问
DHTGBOT_INSTALL_OVERWRITE=always   # 总是覆盖
DHTGBOT_INSTALL_OVERWRITE=never    # 永不覆盖
```

### 远程二进制来源

安装脚本会根据版本参数下载对应 workflow 发布的二进制：

- 默认：`latest`
  读取最新正式 Release：<https://github.com/haiyewei/dhtgbot/releases/latest>
- `DHTGBOT_INSTALL_VERSION=daily`
  读取 `Daily Build` 工作流发布的 `daily` tag 产物：<https://github.com/haiyewei/dhtgbot/releases/tag/daily>
- `DHTGBOT_INSTALL_VERSION=v0.2.2`
  读取指定 tag 的 Release 产物，例如：<https://github.com/haiyewei/dhtgbot/releases/tag/v0.2.2>

依赖程序也采用同样思路：

- `amagi` 从 `amagi-rs` 的 GitHub Release 下载：<https://github.com/bandange/amagi-rs/releases>
- `tdlr` 从 `tdlr` 的 GitHub Release 下载：<https://github.com/haiyewei/tdlr/releases>
- `aria2` 使用官方 GitHub Release `release-1.37.0`：<https://github.com/aria2/aria2/releases/tag/release-1.37.0>

## Docker

仓库现在提供完整容器方案，镜像内直接包含：

- `dhtgbot`
- `amagi` musl Linux 版
- `tdlr` musl Linux 版
- `aria2`

容器基础镜像使用 Alpine，容器里的 Rust 二进制链路统一走 `musl`：

- `dhtgbot`：从 GitHub Release 下载 `dhtgbot-*-unknown-linux-musl.tar.gz`
- `amagi`：下载 `*-unknown-linux-musl` 发布包
- `tdlr`：下载 `*-unknown-linux-musl` 发布包
- `aria2`：使用 Alpine 仓库版本

容器运行时默认工作目录为 `/var/lib/dhtgbot`，程序最终读取的是 `/var/lib/dhtgbot/config.yaml`。入口脚本会先切换到该目录，再启动 `dhtgbot`。

推荐初始化流程：

```bash
docker pull docker.io/haiyewei/dhtgbot:latest
docker compose run --rm dhtgbot init
```

上面的 `init` 会：

- 创建 `./.docker-data/config.yaml`
- 如果文件已存在则直接复用，不覆盖
- 打印运行目录、配置路径和下一步提示

然后在宿主机编辑：

```bash
./.docker-data/config.yaml
```

最少需要先替换这些占位值：

- `bots.master.token`
- `bots.tdl.token` / `bots.xdl.token`（对应 bot 启用时）
- `bots.tdl.forward.account` / `bots.xdl.account`
- `bots.xdl.twitter.cookies`（启用 X/Twitter 相关功能时）

配置完成后再启动：

```bash
docker compose up -d
```

本地构建镜像也是同样流程：

```bash
docker compose up -d --build
```

也可以直接使用 GHCR：

```bash
docker pull ghcr.io/haiyewei/dhtgbot:latest
docker run --rm -v "$PWD/.docker-data:/var/lib/dhtgbot" ghcr.io/haiyewei/dhtgbot:latest help
docker run --rm -v "$PWD/.docker-data:/var/lib/dhtgbot" ghcr.io/haiyewei/dhtgbot:latest init
```

容器额外提供这些辅助命令：

- `help`：显示容器启动帮助
- `init`：初始化运行目录并生成 `config.yaml`
- `config-path`：输出容器内配置路径
- `show-config`：输出当前运行中的 `config.yaml`
- `example-config`：输出镜像内置的 Docker 配置模板

说明：

- [compose.yaml](https://github.com/haiyewei/dhtgbot/blob/master/compose.yaml) 默认把运行数据挂载到仓库下的 `./.docker-data`
- `docker compose run --rm dhtgbot init` 之后，应直接在宿主机修改 `./.docker-data/config.yaml`
- 如果 `config.yaml` 仍保留模板占位值，入口脚本会先提示如何初始化和修改配置，再拒绝启动主程序
- 容器内默认暴露 `4567`、`8787`、`6800`
- Docker 专用配置模板把 `amagi` / `tdlr` / `aria2` 改为容器内可对外监听的参数
- Dockerfile 默认跟随 `dhtgbot`、`amagi-rs` 与 `tdlr` 的最新 GitHub Release；如需固定版本，可在构建时覆盖 `DHTGBOT_VERSION` / `AMAGI_VERSION` / `TDLR_VERSION`
- `dhtgbot` 自己仍然通过 `127.0.0.1` 访问这些服务，所以程序行为与本机模式一致
- 当前 Docker 方案不再依赖 Debian / glibc
- `Docker Publish` 工作流会把当前仓库的 `README.md` 同步到 Docker Hub Overview：<https://hub.docker.com/r/haiyewei/dhtgbot>

## 配置

安装后会准备以下运行目录：

- Windows: `%LOCALAPPDATA%\Programs\dhtgbot\app\config.example.yaml`
- Linux/macOS: `~/.local/share/dhtgbot/config.example.yaml`

复制 [config.example.yaml](https://github.com/haiyewei/dhtgbot/blob/master/config.example.yaml) 为 `config.yaml` 后再开始修改。

最重要的字段有：

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

注意：

- `bots.xdl.twitter.cookies` 建议用单引号包裹整段 Cookie
- `bots.xdl.twitter.cookies` 会在每次请求 `amagi` 时通过 `X-Amagi-Twitter-Cookie` 请求头转发，不依赖 `amagi` 启动时预先绑定
- `bots.tdl.forward.account` 和 `bots.xdl.account` 应填写 `tdlr` 账号的数字 `user_id`，因为当前通过 `tdlr` 的结构化 HTTP API 传递 `account`
- `dhtgbot` 当前通过 `GET /v1/version`、`POST /v1/forwards`、`POST /v1/uploads` 与 `tdlr service --http-bind ...` 交互，不再使用旧的 `/execute` `cliapi`
- `start_command` 应填写环境中的命令，而不是绝对路径
- 程序启动时固定从当前工作目录读取 `config.yaml`
- 安装脚本生成的启动入口会自动切换到应用目录运行

## Bot 说明

### master

主要命令：

- `/help`
- `/backup`
- `/backup_status`
- `/restore`
- `/restore_cancel`
- `/restore_nozip`
- `/echo`
- `/mdata`

恢复链路目前是：

- 上传 ZIP
- 输入 ZIP 密码，或使用 `/restore_nozip`
- 输入导入密码
- 从 ZIP 内读取 `.sql`
- 覆盖导入 SQLite

### tdl

用于监听指定群组/话题中的消息，并把符合规则的链接交给 `tdlr` 处理。

主要命令：

- `/help`
- `/version`
- `/forward`

### xdl

用于 X/Twitter 相关能力。

主要命令：

- `/profile`
- `/tweet`
- `/tweets`
- `/search`
- `/tweetdl`
- `/tweet_like_dl`
- `/author_track`

支持：

- 单条 tweet 查询
- 媒体下载
- 点赞轮询
- 作者追踪
- 通过 `tdlr` 上传媒体

## 数据与兼容

当前存储层已经回到旧版数据库模型：

- 表结构：`bot_xxx(key, value)`
- 值结构：`{"value": ...}`

启动时会自动创建缺失表，并对已知的旧/中间格式数据做规范化处理。

## 开发

```bash
cargo fmt
cargo test --locked --workspace
cargo run
```

程序启动时会：

1. 读取 `config.yaml`
2. 初始化 SQLite
3. 启动或连接 `amagi`、`tdlr`、`aria2`
4. 启动已启用的 bots

## 发布

仓库包含三套工作流：

- [Cargo CI](https://github.com/haiyewei/dhtgbot/actions/workflows/cargo.yml)
- [Daily Build](https://github.com/haiyewei/dhtgbot/actions/workflows/daily.yml)
- [Release](https://github.com/haiyewei/dhtgbot/actions/workflows/release.yml)
- [Docker Publish](https://github.com/haiyewei/dhtgbot/actions/workflows/docker.yml)

发布包中会包含：

- `dhtgbot`
- `config.example.yaml`
- 安装脚本
- Linux `systemd` 安装脚本

安装脚本本身既支持：

- 从 release 包中本地执行
- 从 GitHub Raw 远程执行
- 再按 workflow 发布结果去下载远程二进制

Docker 镜像发布到：

- `docker.io/haiyewei/dhtgbot:latest`
- `docker.io/haiyewei/dhtgbot:vX.Y.Z`
- `docker.io/haiyewei/dhtgbot:sha-<commit>`
- `ghcr.io/haiyewei/dhtgbot:latest`
- `ghcr.io/haiyewei/dhtgbot:vX.Y.Z`
- `ghcr.io/haiyewei/dhtgbot:sha-<commit>`

镜像仓库页面：

- Docker Hub: <https://hub.docker.com/r/haiyewei/dhtgbot>
- GHCR: <https://github.com/haiyewei/dhtgbot/pkgs/container/dhtgbot>

`Docker Publish` 工作流需要配置这两个 GitHub Secrets：

- `DOCKERHUB_USERNAME`
- `DOCKERHUB_TOKEN`

其中 `DOCKERHUB_TOKEN` 应使用 Docker Hub 的 Access Token，而不是账号密码。

GHCR 不需要额外自定义 Secret，工作流直接使用内置：

- `GITHUB_TOKEN`

前提是工作流保留 `packages: write` 权限。

## 当前状态

这是一个面向实际运行的工程仓库，而不是通用 SDK。当前更关注：

- 行为迁移正确
- 配置明确
- 运行目录固定
- 安装链路完整
- 后续便于继续扩展新的 bot 能力
