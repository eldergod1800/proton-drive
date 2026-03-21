use crate::config::SyncPair;
use notify::{RecommendedWatcher, RecursiveMode, Watcher, recommended_watcher};
use std::path::PathBuf;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub enum SyncEvent {
    LocalChanged { local_path: PathBuf, pair_remote: String },
    LocalDeleted { local_path: PathBuf, pair_remote: String },
}

pub struct SyncEngine {
    pairs: Vec<SyncPair>,
    tx: Sender<SyncEvent>,
    watchers: Vec<RecommendedWatcher>,
}

impl SyncEngine {
    pub fn new(pairs: Vec<SyncPair>, tx: Sender<SyncEvent>) -> Self {
        Self { pairs, tx, watchers: Vec::new() }
    }

    pub async fn start(&mut self) -> anyhow::Result<()> {
        for pair in &self.pairs {
            let tx_clone = self.tx.clone();
            let remote_clone = pair.remote.clone();
            let local_root = PathBuf::from(&pair.local);

            let watcher = recommended_watcher(move |res: notify::Result<notify::Event>| {
                match res {
                    Ok(event) => {
                        use notify::EventKind;
                        for path in &event.paths {
                            if path.is_file() {
                                let sync_event = match event.kind {
                                    EventKind::Remove(_) => SyncEvent::LocalDeleted {
                                        local_path: path.clone(),
                                        pair_remote: remote_clone.clone(),
                                    },
                                    _ => SyncEvent::LocalChanged {
                                        local_path: path.clone(),
                                        pair_remote: remote_clone.clone(),
                                    },
                                };
                                // Best-effort send — if channel is full, skip
                                let _ = tx_clone.blocking_send(sync_event);
                            }
                        }
                    }
                    Err(e) => tracing::warn!("watch error: {:?}", e),
                }
            })?;

            let mut watcher = watcher;
            watcher.watch(&local_root, RecursiveMode::Recursive)?;
            self.watchers.push(watcher);
        }
        Ok(())
    }
}
