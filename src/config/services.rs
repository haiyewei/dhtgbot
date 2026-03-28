use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct HttpServiceConfig {
    pub base_url: String,
    pub start_command: Option<String>,
    #[serde(default = "default_startup_timeout_ms")]
    pub startup_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Aria2ServiceConfig {
    pub rpc_url: String,
    pub secret: Option<String>,
    pub start_command: Option<String>,
    #[serde(default = "default_startup_timeout_ms")]
    pub startup_timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServicesConfig {
    pub amagi: HttpServiceConfig,
    pub tdlr: HttpServiceConfig,
    pub aria2: Aria2ServiceConfig,
}

fn default_startup_timeout_ms() -> u64 {
    15_000
}
