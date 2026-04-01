use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tokio::process::Command;
use tracing::info;

use crate::config::{AppConfig, Aria2ServiceConfig, HttpServiceConfig};

use super::aria2::Aria2Client;
use super::tdlr::TdlrClient;
use super::twitter_bridge::TwitterBridge;

pub async fn ensure_services_started(
    root: &Path,
    config: &AppConfig,
    twitter: &TwitterBridge,
    tdlr: &TdlrClient,
    aria2: &Aria2Client,
) -> Result<()> {
    let needs_tdlr = config.bots.tdl.base.enabled || config.bots.xdl.base.enabled;
    let needs_amagi = config.bots.xdl.base.enabled;
    let needs_aria2 = config.bots.xdl.base.enabled
        && (config.bots.xdl.tweetdl.is_some()
            || config.bots.xdl.like_dl.is_some()
            || config.bots.xdl.author_track.is_some());

    if needs_amagi {
        ensure_http_service(root, "amagi", &config.services.amagi, &[], || {
            twitter.health()
        })
        .await?;
    }

    if needs_tdlr {
        ensure_http_service(root, "tdlr", &config.services.tdlr, &[], || tdlr.health()).await?;
    }

    if needs_aria2 {
        ensure_aria2_service(root, &config.services.aria2, aria2).await?;
    }

    Ok(())
}

async fn ensure_http_service<F, Fut>(
    root: &Path,
    name: &str,
    config: &HttpServiceConfig,
    env_overrides: &[(String, String)],
    health_check: F,
) -> Result<()>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    if health_check().await {
        info!(service = name, base_url = %config.base_url, "service is healthy");
        return Ok(());
    }

    let command = config
        .start_command
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .with_context(|| {
            format!(
                "service '{}' is unavailable at {} and start_command is not configured",
                name, config.base_url
            )
        })?;

    let env_keys = env_overrides
        .iter()
        .map(|(key, _)| key.as_str())
        .collect::<Vec<_>>();
    info!(service = name, command = %command, env_keys = ?env_keys, "starting service");
    spawn_start_command(root, command, env_overrides).await?;
    wait_until_healthy(
        Duration::from_millis(config.startup_timeout_ms),
        health_check,
        || {
            format!(
                "service '{}' did not become healthy at {}",
                name, config.base_url
            )
        },
    )
    .await
}

async fn ensure_aria2_service(
    root: &Path,
    config: &Aria2ServiceConfig,
    aria2: &Aria2Client,
) -> Result<()> {
    if aria2.health().await {
        info!(service = "aria2", rpc_url = %config.rpc_url, "service is healthy");
        return Ok(());
    }

    let command = config
        .start_command
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .with_context(|| {
            format!(
                "service 'aria2' is unavailable at {} and start_command is not configured",
                config.rpc_url
            )
        })?;

    info!(service = "aria2", command = %command, "starting service");
    spawn_start_command(root, command, &[]).await?;
    wait_until_healthy(
        Duration::from_millis(config.startup_timeout_ms),
        || aria2.health(),
        || {
            format!(
                "service 'aria2' did not become healthy at {}",
                config.rpc_url
            )
        },
    )
    .await
}

async fn wait_until_healthy<F, Fut, E>(
    timeout: Duration,
    health_check: F,
    error_message: E,
) -> Result<()>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = bool>,
    E: Fn() -> String,
{
    let deadline = Instant::now() + timeout;
    loop {
        if health_check().await {
            return Ok(());
        }
        if Instant::now() >= deadline {
            bail!("{}", error_message());
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn spawn_start_command(
    root: &Path,
    command: &str,
    env_overrides: &[(String, String)],
) -> Result<()> {
    let mut process = if cfg!(windows) {
        let mut cmd = Command::new("cmd");
        cmd.arg("/C").arg(command);
        cmd
    } else {
        let mut cmd = Command::new("sh");
        cmd.arg("-lc").arg(command);
        cmd
    };

    process
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    for (key, value) in env_overrides {
        process.env(key, value);
    }

    process
        .spawn()
        .with_context(|| format!("failed to spawn start_command: {command}"))?;
    Ok(())
}
