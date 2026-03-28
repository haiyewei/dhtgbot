use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::Mutex;

#[derive(Clone)]
pub(super) struct RestoreState {
    pub(super) step: RestoreStep,
    pub(super) zip_path: Option<PathBuf>,
    pub(super) zip_password: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RestoreStep {
    AwaitingZip,
    AwaitingZipPassword,
    AwaitingImportPassword,
}

#[derive(Clone)]
pub(super) struct MasterRuntime {
    pub(super) pending_backups: Arc<Mutex<HashMap<u64, ()>>>,
    pub(super) pending_restores: Arc<Mutex<HashMap<u64, RestoreState>>>,
}

impl MasterRuntime {
    pub(super) fn new() -> Self {
        Self {
            pending_backups: Arc::new(Mutex::new(HashMap::new())),
            pending_restores: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
