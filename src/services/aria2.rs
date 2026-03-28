use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use reqwest::Client;
use serde::Deserialize;
use serde_json::{Value, json};
use tracing::info;

use crate::config::Aria2ServiceConfig;

#[derive(Debug, Clone)]
pub struct Aria2Client {
    rpc_url: String,
    secret: Option<String>,
    client: Client,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse<T> {
    result: Option<T>,
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Debug, Deserialize)]
struct TellStatusResult {
    status: String,
    #[serde(rename = "errorMessage")]
    error_message: Option<String>,
}

impl Aria2Client {
    pub fn new(config: &Aria2ServiceConfig) -> Self {
        Self {
            rpc_url: config.rpc_url.trim_end_matches('/').to_string(),
            secret: config.secret.clone(),
            client: Client::builder()
                .connect_timeout(Duration::from_millis(800))
                .timeout(Duration::from_secs(30))
                .build()
                .expect("aria2 http client should build"),
        }
    }

    pub fn rpc_url(&self) -> &str {
        &self.rpc_url
    }

    pub async fn health(&self) -> bool {
        self.call::<Value>("aria2.getVersion", Vec::new())
            .await
            .is_ok()
    }

    pub async fn download(
        &self,
        url: &str,
        output_dir: &str,
        filename: &str,
        connections: usize,
    ) -> Result<()> {
        let source_host = reqwest::Url::parse(url)
            .ok()
            .and_then(|parsed| parsed.host_str().map(str::to_string))
            .unwrap_or_else(|| String::from("-"));
        info!(
            service = "aria2",
            rpc_url = %self.rpc_url,
            filename,
            output_dir,
            source_host = %source_host,
            connections,
            "starting aria2 download"
        );
        let started_at = Instant::now();
        let gid = self
            .call::<String>(
                "aria2.addUri",
                vec![
                    json!([url]),
                    json!({
                        "dir": output_dir,
                        "out": filename,
                        "split": connections.to_string(),
                        "max-connection-per-server": connections.to_string(),
                    }),
                ],
            )
            .await?;
        info!(
            service = "aria2",
            rpc_url = %self.rpc_url,
            gid = %gid,
            filename,
            "aria2 job accepted"
        );

        let deadline = Instant::now() + Duration::from_secs(60 * 30);
        loop {
            if Instant::now() >= deadline {
                bail!("aria2 download timed out for gid {gid}");
            }

            let status = self
                .call::<TellStatusResult>(
                    "aria2.tellStatus",
                    vec![json!(gid), json!(["status", "errorMessage"])],
                )
                .await?;

            match status.status.as_str() {
                "complete" => {
                    info!(
                        service = "aria2",
                        rpc_url = %self.rpc_url,
                        gid = %gid,
                        filename,
                        elapsed_ms = started_at.elapsed().as_millis() as u64,
                        "aria2 download completed"
                    );
                    return Ok(());
                }
                "error" | "removed" => {
                    bail!(
                        "aria2 download failed: {}",
                        status
                            .error_message
                            .unwrap_or_else(|| format!("status={}", status.status))
                    );
                }
                _ => tokio::time::sleep(Duration::from_millis(500)).await,
            }
        }
    }

    async fn call<T>(&self, method: &str, extra_params: Vec<Value>) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let mut params = Vec::new();
        if let Some(secret) = &self.secret {
            params.push(json!(format!("token:{secret}")));
        }
        params.extend(extra_params);

        let body = json!({
            "jsonrpc": "2.0",
            "id": method,
            "method": method,
            "params": params,
        });

        let response = self
            .client
            .post(&self.rpc_url)
            .json(&body)
            .send()
            .await
            .with_context(|| format!("failed to call aria2 rpc: {}", self.rpc_url))?;

        let status = response.status();
        let text = response
            .text()
            .await
            .with_context(|| format!("failed to read aria2 rpc response: {}", self.rpc_url))?;
        if !status.is_success() {
            bail!("aria2 rpc returned HTTP {}: {}", status, text);
        }

        let payload: JsonRpcResponse<T> = serde_json::from_str(&text)
            .with_context(|| format!("failed to decode aria2 rpc response: {}", text))?;

        if let Some(error) = payload.error {
            bail!("aria2 rpc error {}: {}", error.code, error.message);
        }

        payload
            .result
            .context("aria2 rpc response did not include result")
    }
}
