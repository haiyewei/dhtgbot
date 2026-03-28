use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use tracing::info;

use crate::bots;
use crate::config::AppConfig;
use crate::services::aria2::Aria2Client;
use crate::services::service_launcher::ensure_services_started;
use crate::services::task_queue::TaskQueue;
use crate::services::tdlr::TdlrClient;
use crate::services::twitter_bridge::TwitterBridge;
use crate::storage::{KvStore, bootstrap_store};

#[derive(Clone)]
pub struct AppContext {
    pub root: Arc<PathBuf>,
    pub config: Arc<AppConfig>,
    pub store: KvStore,
    pub work_queue: TaskQueue,
    pub aria2: Aria2Client,
    pub tdlr: TdlrClient,
    pub twitter_bridge: TwitterBridge,
}

pub async fn run() -> Result<()> {
    let context = build_context().await?;
    info!("loaded config and initialized sqlite store");
    bots::run_enabled(context).await
}

async fn build_context() -> Result<AppContext> {
    let root = Arc::new(std::env::current_dir()?);
    let config = Arc::new(AppConfig::load(root.join("config.yaml"))?);
    let store = KvStore::connect(&config.sqlite_path(&root)).await?;
    bootstrap_store(&store, &config).await?;
    let work_queue = TaskQueue::new("work_queue::serial");
    let aria2 = Aria2Client::new(&config.services.aria2);
    let tdlr = TdlrClient::new(&config.services.tdlr);
    let twitter_bridge =
        TwitterBridge::new(&config.services.amagi, config.bots.xdl.twitter.as_ref());

    ensure_services_started(&root, &config, &twitter_bridge, &tdlr, &aria2).await?;

    Ok(AppContext {
        root,
        config,
        store,
        work_queue,
        aria2,
        tdlr,
        twitter_bridge,
    })
}
