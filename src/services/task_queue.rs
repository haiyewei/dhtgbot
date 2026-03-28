use std::collections::{BTreeMap, HashSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

type JobFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

#[derive(Debug, Clone)]
pub struct TaskLabel {
    bot: &'static str,
    action: &'static str,
    subqueue: String,
    item: String,
}

impl TaskLabel {
    pub fn new(bot: &'static str, action: &'static str, item: impl Into<String>) -> Self {
        Self {
            bot,
            action,
            subqueue: format!("{bot}.{action}"),
            item: item.into(),
        }
    }

    fn item(&self) -> &str {
        &self.item
    }

    fn subqueue(&self) -> &str {
        &self.subqueue
    }
}

#[derive(Clone)]
pub struct TaskQueue {
    sender: mpsc::UnboundedSender<Job>,
    state: Arc<QueueState>,
    label: &'static str,
}

struct Job {
    seq: u64,
    key: Option<String>,
    label: TaskLabel,
    enqueued_at: Instant,
    future: JobFuture,
}

struct QueueState {
    stats: Mutex<QueueStats>,
}

struct QueueStats {
    next_seq: u64,
    pending_jobs: usize,
    subqueues: BTreeMap<String, usize>,
    pending_keys: HashSet<String>,
}

struct QueueSnapshot {
    total_pending: usize,
    subqueue_pending: usize,
    subqueues: String,
}

enum PrepareEnqueue {
    Accepted { seq: u64, snapshot: QueueSnapshot },
    Duplicate { snapshot: QueueSnapshot },
}

impl TaskQueue {
    pub fn new(label: &'static str) -> Self {
        let (sender, mut receiver) = mpsc::unbounded_channel::<Job>();
        let state = Arc::new(QueueState {
            stats: Mutex::new(QueueStats {
                next_seq: 1,
                pending_jobs: 0,
                subqueues: BTreeMap::new(),
                pending_keys: HashSet::new(),
            }),
        });
        let worker_state = state.clone();

        tokio::spawn(async move {
            while let Some(job) = receiver.recv().await {
                let wait_ms = job.enqueued_at.elapsed().as_millis() as u64;
                let running = worker_state.snapshot(job.label.subqueue());
                info!(
                    queue = label,
                    seq = job.seq,
                    subqueue = job.label.subqueue(),
                    bot = job.label.bot,
                    action = job.label.action,
                    item = %job.label.item(),
                    total_pending = running.total_pending,
                    subqueue_pending = running.subqueue_pending,
                    subqueues = %running.subqueues,
                    wait_ms,
                    "task started"
                );

                let started_at = Instant::now();
                job.future.await;

                let run_ms = started_at.elapsed().as_millis() as u64;
                let remaining = worker_state.finish(job.label.subqueue(), job.key.as_deref());
                info!(
                    queue = label,
                    seq = job.seq,
                    subqueue = job.label.subqueue(),
                    bot = job.label.bot,
                    action = job.label.action,
                    item = %job.label.item(),
                    total_pending = remaining.total_pending,
                    subqueue_pending = remaining.subqueue_pending,
                    subqueues = %remaining.subqueues,
                    run_ms,
                    "task finished"
                );
            }
        });

        Self {
            sender,
            state,
            label,
        }
    }

    pub fn enqueue<F>(&self, label: TaskLabel, future: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let _ = self.enqueue_internal(label, None, future);
    }

    pub fn enqueue_unique<F>(&self, label: TaskLabel, key: impl Into<String>, future: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        self.enqueue_internal(label, Some(key.into()), future)
    }

    pub fn enqueue_unique_result<F, T>(
        &self,
        label: TaskLabel,
        key: impl Into<String>,
        future: F,
    ) -> Option<oneshot::Receiver<T>>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (sender, receiver) = oneshot::channel();
        let enqueued = self.enqueue_internal(label, Some(key.into()), async move {
            let result = future.await;
            let _ = sender.send(result);
        });
        enqueued.then_some(receiver)
    }

    pub async fn run<F, T>(
        &self,
        label: TaskLabel,
        future: F,
    ) -> Result<T, oneshot::error::RecvError>
    where
        F: Future<Output = T> + Send + 'static,
        T: Send + 'static,
    {
        let (sender, receiver) = oneshot::channel();
        self.enqueue(label, async move {
            let result = future.await;
            let _ = sender.send(result);
        });
        receiver.await
    }

    fn enqueue_internal<F>(&self, label: TaskLabel, key: Option<String>, future: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let prepare = self.state.prepare_enqueue(label.subqueue(), key.as_deref());
        let (seq, snapshot) = match prepare {
            PrepareEnqueue::Accepted { seq, snapshot } => (seq, snapshot),
            PrepareEnqueue::Duplicate { snapshot } => {
                info!(
                    queue = self.label,
                    subqueue = label.subqueue(),
                    bot = label.bot,
                    action = label.action,
                    item = %label.item(),
                    total_pending = snapshot.total_pending,
                    subqueue_pending = snapshot.subqueue_pending,
                    subqueues = %snapshot.subqueues,
                    "task already queued or running, skipped enqueue"
                );
                return false;
            }
        };

        info!(
            queue = self.label,
            seq,
            subqueue = label.subqueue(),
            bot = label.bot,
            action = label.action,
            item = %label.item(),
            total_pending = snapshot.total_pending,
            subqueue_pending = snapshot.subqueue_pending,
            subqueues = %snapshot.subqueues,
            "task enqueued"
        );

        if let Err(error) = self.sender.send(Job {
            seq,
            key: key.clone(),
            label: label.clone(),
            enqueued_at: Instant::now(),
            future: Box::pin(future),
        }) {
            let rolled_back = self
                .state
                .rollback_enqueue(label.subqueue(), key.as_deref());
            error!(
                queue = self.label,
                seq,
                subqueue = label.subqueue(),
                bot = label.bot,
                action = label.action,
                item = %label.item(),
                total_pending = rolled_back.total_pending,
                subqueue_pending = rolled_back.subqueue_pending,
                subqueues = %rolled_back.subqueues,
                reason = %error,
                "failed to enqueue task"
            );
            return false;
        }

        true
    }
}

impl QueueState {
    fn prepare_enqueue(&self, subqueue: &str, key: Option<&str>) -> PrepareEnqueue {
        let mut stats = self
            .stats
            .lock()
            .expect("queue stats lock should not panic");
        if let Some(key) = key {
            if stats.pending_keys.contains(key) {
                return PrepareEnqueue::Duplicate {
                    snapshot: QueueSnapshot {
                        total_pending: stats.pending_jobs,
                        subqueue_pending: stats
                            .subqueues
                            .get(subqueue)
                            .copied()
                            .unwrap_or_default(),
                        subqueues: format_subqueues(&stats.subqueues),
                    },
                };
            }
        }
        let seq = stats.next_seq;
        stats.next_seq += 1;
        stats.pending_jobs += 1;

        let subqueue_pending = {
            let count = stats.subqueues.entry(subqueue.to_string()).or_insert(0);
            *count += 1;
            *count
        };
        if let Some(key) = key {
            stats.pending_keys.insert(key.to_string());
        }

        PrepareEnqueue::Accepted {
            seq,
            snapshot: QueueSnapshot {
                total_pending: stats.pending_jobs,
                subqueue_pending,
                subqueues: format_subqueues(&stats.subqueues),
            },
        }
    }

    fn rollback_enqueue(&self, subqueue: &str, key: Option<&str>) -> QueueSnapshot {
        let mut stats = self
            .stats
            .lock()
            .expect("queue stats lock should not panic");
        stats.pending_jobs = stats.pending_jobs.saturating_sub(1);
        let subqueue_pending = decrement_subqueue(&mut stats.subqueues, subqueue);
        if let Some(key) = key {
            stats.pending_keys.remove(key);
        }

        QueueSnapshot {
            total_pending: stats.pending_jobs,
            subqueue_pending,
            subqueues: format_subqueues(&stats.subqueues),
        }
    }

    fn snapshot(&self, subqueue: &str) -> QueueSnapshot {
        let stats = self
            .stats
            .lock()
            .expect("queue stats lock should not panic");
        QueueSnapshot {
            total_pending: stats.pending_jobs,
            subqueue_pending: stats.subqueues.get(subqueue).copied().unwrap_or_default(),
            subqueues: format_subqueues(&stats.subqueues),
        }
    }

    fn finish(&self, subqueue: &str, key: Option<&str>) -> QueueSnapshot {
        let mut stats = self
            .stats
            .lock()
            .expect("queue stats lock should not panic");
        stats.pending_jobs = stats.pending_jobs.saturating_sub(1);
        let subqueue_pending = decrement_subqueue(&mut stats.subqueues, subqueue);
        if let Some(key) = key {
            stats.pending_keys.remove(key);
        }

        QueueSnapshot {
            total_pending: stats.pending_jobs,
            subqueue_pending,
            subqueues: format_subqueues(&stats.subqueues),
        }
    }
}

fn decrement_subqueue(subqueues: &mut BTreeMap<String, usize>, subqueue: &str) -> usize {
    let Some(count) = subqueues.get_mut(subqueue) else {
        return 0;
    };

    *count = count.saturating_sub(1);
    let remaining = *count;
    if remaining == 0 {
        subqueues.remove(subqueue);
    }
    remaining
}

fn format_subqueues(subqueues: &BTreeMap<String, usize>) -> String {
    if subqueues.is_empty() {
        return String::from("-");
    }

    subqueues
        .iter()
        .map(|(subqueue, count)| format!("{subqueue}={count}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::sync::{Mutex, oneshot};

    use super::{TaskLabel, TaskQueue};

    #[tokio::test]
    async fn run_returns_future_result() {
        let queue = TaskQueue::new("task_queue::test");
        let value = queue
            .run(TaskLabel::new("test", "job", "42"), async { 42usize })
            .await
            .unwrap();
        assert_eq!(value, 42);
    }

    #[tokio::test]
    async fn executes_jobs_in_fifo_order() {
        let queue = TaskQueue::new("task_queue::fifo");
        let order = Arc::new(Mutex::new(Vec::new()));
        let mut receivers = Vec::new();

        for value in [1usize, 2, 3] {
            let order = order.clone();
            let (sender, receiver) = oneshot::channel();
            receivers.push(receiver);

            queue.enqueue(
                TaskLabel::new("test", "fifo", value.to_string()),
                async move {
                    order.lock().await.push(value);
                    let _ = sender.send(());
                },
            );
        }

        for receiver in receivers {
            receiver.await.unwrap();
        }

        assert_eq!(*order.lock().await, vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn never_runs_jobs_in_parallel() {
        let queue = TaskQueue::new("task_queue::serial");
        let current = Arc::new(AtomicUsize::new(0));
        let max_seen = Arc::new(AtomicUsize::new(0));
        let mut receivers = Vec::new();

        for index in 0..4usize {
            let current = current.clone();
            let max_seen = max_seen.clone();
            let (sender, receiver) = oneshot::channel();
            receivers.push(receiver);

            queue.enqueue(
                TaskLabel::new("test", "serial", index.to_string()),
                async move {
                    let running = current.fetch_add(1, Ordering::SeqCst) + 1;
                    max_seen.fetch_max(running, Ordering::SeqCst);
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    current.fetch_sub(1, Ordering::SeqCst);
                    let _ = sender.send(());
                },
            );
        }

        for receiver in receivers {
            receiver.await.unwrap();
        }

        assert_eq!(max_seen.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn skips_duplicate_key_while_job_is_pending() {
        let queue = TaskQueue::new("task_queue::dedupe");
        let order = Arc::new(Mutex::new(Vec::new()));
        let (first_sender, first_receiver) = oneshot::channel();

        let first_order = order.clone();
        assert!(queue.enqueue_unique(
            TaskLabel::new("test", "dedupe", "same"),
            "same-key",
            async move {
                first_order.lock().await.push("first");
                let _ = first_sender.send(());
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            },
        ));

        let second_order = order.clone();
        assert!(!queue.enqueue_unique(
            TaskLabel::new("test", "dedupe", "same"),
            "same-key",
            async move {
                second_order.lock().await.push("second");
            },
        ));

        first_receiver.await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(80)).await;
        assert_eq!(*order.lock().await, vec!["first"]);
    }
}
