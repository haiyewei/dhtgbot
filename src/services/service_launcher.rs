use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use tokio::process::Command;
use tracing::{info, warn};

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
    let amagi_env = amagi_start_env(config);

    if needs_amagi {
        ensure_http_service(root, "amagi", &config.services.amagi, &amagi_env, || {
            twitter.health()
        })
        .await?;
        warn_if_amagi_missing_twitter_cookie(twitter, !amagi_env.is_empty()).await;
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

fn amagi_start_env(config: &AppConfig) -> Vec<(String, String)> {
    config
        .bots
        .xdl
        .twitter
        .as_ref()
        .and_then(|twitter| twitter.cookies())
        .map(|cookies| vec![(String::from("AMAGI_TWITTER_COOKIE"), cookies.to_string())])
        .unwrap_or_default()
}

async fn warn_if_amagi_missing_twitter_cookie(twitter: &TwitterBridge, cookie_configured: bool) {
    if !cookie_configured {
        return;
    }

    match twitter.twitter_cookie_bound().await {
        Ok(Some(true)) => {
            info!(
                service = "amagi",
                platform = "twitter",
                "twitter cookie is bound"
            );
        }
        Ok(Some(false)) => {
            warn!(
                service = "amagi",
                platform = "twitter",
                "twitter cookie is configured in xdl but the running amagi service has no twitter cookie bound"
            );
        }
        Ok(None) => {
            warn!(
                service = "amagi",
                platform = "twitter",
                "amagi root metadata did not report twitter cookie status"
            );
        }
        Err(error) => {
            warn!(
                ?error,
                service = "amagi",
                platform = "twitter",
                "failed to verify twitter cookie status from amagi root metadata"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::amagi_start_env;
    use crate::config::AppConfig;

    fn parse_config(yaml: &str) -> AppConfig {
        serde_yaml::from_str(yaml).unwrap()
    }

    #[test]
    fn amagi_start_env_uses_xdl_twitter_cookies() {
        let config = parse_config(
            r#"
bots:
  master:
    name: master
    token: t1
    enabled: false
    admins: []
    backup:
  tdl:
    name: tdl
    token: t2
    enabled: false
    forward:
  xdl:
    name: xdl
    token: t3
    enabled: true
    account:
    twitter:
      cookies: "auth_token=abc; ct0=def; twid=u%3D1"
    tweetdl:
    like_dl:
    author_track:
services:
  amagi:
    base_url: http://127.0.0.1:4567
    start_command: ""
    startup_timeout_ms: 1000
  tdlr:
    base_url: http://127.0.0.1:8787
    start_command: ""
    startup_timeout_ms: 1000
  aria2:
    rpc_url: http://127.0.0.1:6800/jsonrpc
    secret:
    start_command: ""
    startup_timeout_ms: 1000
database:
  type: sqlite
  path: ./data/bot.sqlite
"#,
        );

        assert_eq!(
            amagi_start_env(&config),
            vec![(
                String::from("AMAGI_TWITTER_COOKIE"),
                String::from("auth_token=abc; ct0=def; twid=u%3D1"),
            )]
        );
    }
}
