mod app;
mod bots;
mod config;
mod logging;
mod services;
mod storage;

use anyhow::Result;

pub use app::AppContext;

#[tokio::main]
async fn main() -> Result<()> {
    logging::init();
    app::run().await
}
