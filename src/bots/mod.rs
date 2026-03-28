use anyhow::Result;

use crate::AppContext;

mod chat;
pub mod common;
pub mod master;
mod shared;
pub mod tdl;
pub mod xdl;

pub async fn run_enabled(context: AppContext) -> Result<()> {
    let mut tasks = Vec::new();

    if context.config.bots.master.base.enabled {
        tasks.push(tokio::spawn(master::run(context.clone())));
    }
    if context.config.bots.tdl.base.enabled {
        tasks.push(tokio::spawn(tdl::run(context.clone())));
    }
    if context.config.bots.xdl.base.enabled {
        tasks.push(tokio::spawn(xdl::run(context.clone())));
    }

    for task in tasks {
        task.await??;
    }

    Ok(())
}
