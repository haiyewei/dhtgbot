use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

use teloxide::prelude::Bot;
use tokio::sync::Mutex;

use crate::AppContext;

#[derive(Clone)]
pub(super) struct AuthorMonitor {
    running: Arc<AtomicBool>,
    task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

impl AuthorMonitor {
    fn new() -> Self {
        Self {
            running: Arc::new(AtomicBool::new(false)),
            task: Arc::new(Mutex::new(None)),
        }
    }

    pub(super) async fn start_like(&self, bot: Bot, context: AppContext) {
        if self.running.swap(true, AtomicOrdering::SeqCst) {
            return;
        }

        let running = self.running.clone();
        let task = tokio::spawn(async move {
            super::monitor::like_monitor_loop(bot, context, running).await;
        });
        *self.task.lock().await = Some(task);
    }

    pub(super) async fn start_author(&self, bot: Bot, context: AppContext) {
        if self.running.swap(true, AtomicOrdering::SeqCst) {
            return;
        }

        let running = self.running.clone();
        let task = tokio::spawn(async move {
            super::monitor::author_monitor_loop(bot, context, running).await;
        });
        *self.task.lock().await = Some(task);
    }

    pub(super) async fn stop(&self) {
        self.running.store(false, AtomicOrdering::SeqCst);
        if let Some(task) = self.task.lock().await.take() {
            task.abort();
        }
    }

    pub(super) fn is_running(&self) -> bool {
        self.running.load(AtomicOrdering::SeqCst)
    }
}

#[derive(Clone)]
pub(super) struct XdlRuntime {
    pub(super) like_monitor: AuthorMonitor,
    pub(super) author_monitor: AuthorMonitor,
}

impl XdlRuntime {
    pub(super) fn new() -> Self {
        Self {
            like_monitor: AuthorMonitor::new(),
            author_monitor: AuthorMonitor::new(),
        }
    }
}
