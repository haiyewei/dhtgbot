use std::time::Duration;
use std::time::Instant;

use anyhow::{Context, Result, bail};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::config::HttpServiceConfig;

const VERSION_PATH: &str = "/v1/version";
const UPLOADS_PATH: &str = "/v1/uploads";
const FORWARDS_PATH: &str = "/v1/forwards";

#[derive(Debug, Clone)]
pub struct ProcessOutput {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone)]
pub struct TdlrVersion {
    pub version: String,
    pub rustc: String,
    pub target: TdlrTarget,
}

#[derive(Debug, Clone)]
pub struct TdlrTarget {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone)]
pub struct TdlrClient {
    base_url: String,
    client: Client,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct UploadRequest {
    path: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    chat: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    topic: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<Vec<i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thumb_map: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "is_false")]
    group: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    rm: bool,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
struct ForwardRequest {
    from: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    topic: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    account: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct VersionResponse {
    ok: bool,
    version: String,
    rustc: String,
    target: VersionTarget,
}

#[derive(Debug, Deserialize)]
struct VersionTarget {
    os: String,
    arch: String,
}

#[derive(Debug, Deserialize)]
struct OperationResponse {
    ok: bool,
    #[serde(default)]
    exit_code: Option<i32>,
    #[serde(default)]
    stdout: Option<String>,
    #[serde(default)]
    stderr: Option<String>,
    #[serde(default)]
    error: Option<String>,
}

impl TdlrClient {
    pub fn new(config: &HttpServiceConfig) -> Self {
        Self {
            base_url: config.base_url.trim_end_matches('/').to_string(),
            client: Client::builder()
                .connect_timeout(Duration::from_millis(800))
                .timeout(Duration::from_secs(60 * 30))
                .build()
                .expect("tdlr http client should build"),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    pub async fn health(&self) -> bool {
        let url = format!("{}/health", self.base_url);
        self.client
            .get(url)
            .send()
            .await
            .map(|response| response.status().is_success())
            .unwrap_or(false)
    }

    pub async fn version(&self) -> Result<TdlrVersion> {
        info!(
            service = "tdlr",
            base_url = %self.base_url,
            operation = "version",
            path = VERSION_PATH,
            "sending tdlr request"
        );
        let started_at = Instant::now();
        let (status, payload) = self.get_json::<VersionResponse>(VERSION_PATH).await?;
        if !status.is_success() || !payload.ok {
            bail!("tdlr version request failed with HTTP {}", status);
        }

        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        info!(
            service = "tdlr",
            base_url = %self.base_url,
            operation = "version",
            path = VERSION_PATH,
            http_status = %status,
            elapsed_ms,
            "tdlr request completed"
        );
        Ok(TdlrVersion {
            version: payload.version,
            rustc: payload.rustc,
            target: TdlrTarget {
                os: payload.target.os,
                arch: payload.target.arch,
            },
        })
    }

    pub async fn forward(
        &self,
        link: &str,
        target_chat_id: &str,
        target_topic_id: Option<i32>,
        account: Option<&str>,
    ) -> Result<ProcessOutput> {
        let request = build_forward_request(link, target_chat_id, target_topic_id, account)?;
        let detail = format!(
            "link={},chat_id={},topic={},account={}",
            link,
            target_chat_id,
            target_topic_id
                .filter(|topic_id| *topic_id > 0)
                .map(|topic_id| topic_id.to_string())
                .unwrap_or_else(|| String::from("-")),
            request
                .account
                .map(|account_id| account_id.to_string())
                .unwrap_or_else(|| String::from("-"))
        );
        self.run_operation("forward", FORWARDS_PATH, &detail, &request)
            .await
    }

    pub async fn upload(
        &self,
        files: &[String],
        thumb_map: &[String],
        chat_id: &str,
        topic_id: Option<i32>,
        caption: Option<&str>,
        is_group: bool,
        rm: bool,
        account: Option<&str>,
    ) -> Result<ProcessOutput> {
        let request = build_upload_request(
            files, thumb_map, chat_id, topic_id, caption, is_group, rm, account,
        )?;
        let detail = format!(
            "files={},thumb_map={},chat_id={},topic={},account={}",
            request.path.len(),
            request.thumb_map.as_ref().map(Vec::len).unwrap_or_default(),
            chat_id,
            request
                .topic
                .map(|topic_id| topic_id.to_string())
                .unwrap_or_else(|| String::from("-")),
            request
                .account
                .as_ref()
                .and_then(|accounts| accounts.first())
                .map(|account_id| account_id.to_string())
                .unwrap_or_else(|| String::from("-"))
        );
        self.run_operation("upload", UPLOADS_PATH, &detail, &request)
            .await
    }

    async fn run_operation<T>(
        &self,
        operation: &str,
        path: &str,
        detail: &str,
        request: &T,
    ) -> Result<ProcessOutput>
    where
        T: Serialize,
    {
        info!(
            service = "tdlr",
            base_url = %self.base_url,
            operation,
            detail = %detail,
            path,
            "sending tdlr request"
        );
        let started_at = Instant::now();
        let (status, payload) = self
            .post_json::<T, OperationResponse>(path, request)
            .await?;

        let stdout = payload.stdout.unwrap_or_default().trim().to_string();
        let stderr = payload
            .stderr
            .or(payload.error)
            .unwrap_or_default()
            .trim()
            .to_string();
        let code = payload
            .exit_code
            .unwrap_or_else(|| if payload.ok { 0 } else { 1 });

        if !status.is_success() && code == 0 {
            bail!(
                "tdlr service returned HTTP {} with an empty error code",
                status
            );
        }

        let elapsed_ms = started_at.elapsed().as_millis() as u64;
        if code == 0 {
            info!(
                service = "tdlr",
                base_url = %self.base_url,
                operation,
                detail = %detail,
                path,
                http_status = %status,
                exit_code = code,
                elapsed_ms,
                "tdlr request completed"
            );
        } else {
            warn!(
                service = "tdlr",
                base_url = %self.base_url,
                operation,
                detail = %detail,
                path,
                http_status = %status,
                exit_code = code,
                elapsed_ms,
                stderr = %stderr,
                "tdlr request returned non-zero exit code"
            );
        }

        Ok(ProcessOutput {
            code,
            stdout,
            stderr,
        })
    }

    async fn get_json<T>(&self, path: &str) -> Result<(StatusCode, T)>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("failed to call tdlr service: {url}"))?;
        decode_json_response(response, &url).await
    }

    async fn post_json<B, T>(&self, path: &str, body: &B) -> Result<(StatusCode, T)>
    where
        B: Serialize,
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("failed to call tdlr service: {url}"))?;
        decode_json_response(response, &url).await
    }
}

async fn decode_json_response<T>(response: reqwest::Response, url: &str) -> Result<(StatusCode, T)>
where
    T: for<'de> Deserialize<'de>,
{
    let status = response.status();
    let text = response
        .text()
        .await
        .with_context(|| format!("failed to read tdlr service response: {url}"))?;
    let payload = serde_json::from_str(&text).with_context(|| {
        format!(
            "failed to decode tdlr service response (status {}): {}",
            status, text
        )
    })?;
    Ok((status, payload))
}

fn build_upload_request(
    files: &[String],
    thumb_map: &[String],
    chat_id: &str,
    topic_id: Option<i32>,
    caption: Option<&str>,
    is_group: bool,
    rm: bool,
    account: Option<&str>,
) -> Result<UploadRequest> {
    Ok(UploadRequest {
        path: files.to_vec(),
        chat: Some(chat_id.to_string()),
        topic: normalize_topic_id(topic_id),
        account: parse_account_user_id(account)?.map(|account_id| vec![account_id]),
        caption: caption.map(str::to_owned),
        thumb_map: (!thumb_map.is_empty()).then(|| thumb_map.to_vec()),
        group: is_group,
        rm,
    })
}

fn build_forward_request(
    link: &str,
    target_chat_id: &str,
    target_topic_id: Option<i32>,
    account: Option<&str>,
) -> Result<ForwardRequest> {
    Ok(ForwardRequest {
        from: vec![link.to_string()],
        to: Some(target_chat_id.to_string()),
        topic: normalize_topic_id(target_topic_id),
        account: parse_account_user_id(account)?,
    })
}

fn normalize_topic_id(topic_id: Option<i32>) -> Option<i32> {
    topic_id.filter(|value| *value > 0)
}

fn parse_account_user_id(account: Option<&str>) -> Result<Option<i64>> {
    let Some(account) = account.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };

    let account_id = account.parse::<i64>().with_context(|| {
        format!("tdlr account must be a numeric user id for the HTTP API: {account}")
    })?;
    Ok(Some(account_id))
}

fn is_false(value: &bool) -> bool {
    !*value
}

#[cfg(test)]
mod tests {
    use super::{build_forward_request, build_upload_request, parse_account_user_id};

    #[test]
    fn build_upload_request_includes_thumb_map_entries() {
        let request = build_upload_request(
            &[String::from("a.mp4"), String::from("b.jpg")],
            &[String::from("a.mp4=a.jpg")],
            "-1001",
            Some(42),
            Some("caption"),
            true,
            false,
            Some("123456789"),
        )
        .unwrap();

        assert_eq!(
            request,
            super::UploadRequest {
                path: vec![String::from("a.mp4"), String::from("b.jpg")],
                chat: Some(String::from("-1001")),
                topic: Some(42),
                account: Some(vec![123456789]),
                caption: Some(String::from("caption")),
                thumb_map: Some(vec![String::from("a.mp4=a.jpg")]),
                group: true,
                rm: false,
            }
        );
    }

    #[test]
    fn build_forward_request_uses_structured_http_fields() {
        let request =
            build_forward_request("https://t.me/test/1", "-1001", Some(7), Some("42")).unwrap();

        assert_eq!(
            request,
            super::ForwardRequest {
                from: vec![String::from("https://t.me/test/1")],
                to: Some(String::from("-1001")),
                topic: Some(7),
                account: Some(42),
            }
        );
    }

    #[test]
    fn parse_account_user_id_trims_and_accepts_numeric_values() {
        assert_eq!(
            parse_account_user_id(Some(" 123456789 ")).unwrap(),
            Some(123456789)
        );
    }

    #[test]
    fn parse_account_user_id_ignores_blank_values() {
        assert_eq!(parse_account_user_id(Some("   ")).unwrap(), None);
        assert_eq!(parse_account_user_id(None).unwrap(), None);
    }

    #[test]
    fn parse_account_user_id_rejects_non_numeric_values() {
        let error = parse_account_user_id(Some("@alice")).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("tdlr account must be a numeric user id")
        );
    }
}
