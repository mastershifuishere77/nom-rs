use crate::types::{DerivationId, Host, StorePath, STORE_PREFIX};
use crossbeam_channel::{unbounded, Receiver, Sender};
use notify::{Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type Subscriptions = Arc<Mutex<HashMap<StorePath, Vec<(Host, DerivationId)>>>>;

pub struct StoreWatcher {
    _watcher: Option<RecommendedWatcher>,
    subscriptions: Subscriptions,
    pub event_receiver: Receiver<(Host, DerivationId)>,
    event_sender: Sender<(Host, DerivationId)>,
}

impl Default for StoreWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl StoreWatcher {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        let subscriptions: Subscriptions = Arc::new(Mutex::new(HashMap::new()));

        let subs_clone = Arc::clone(&subscriptions);
        let tx_clone = tx.clone();

        let mut watcher = RecommendedWatcher::new(
            move |res: notify::Result<Event>| {
                if let Ok(event) = res {
                    match event.kind {
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any => {
                            for path in event.paths {
                                if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                                    if let Some(sp) = StorePath::parse(file_name) {
                                        let mut subs = subs_clone.lock().unwrap();
                                        if let Some(payloads) = subs.remove(&sp) {
                                            for payload in payloads {
                                                let _ = tx_clone.send(payload);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            },
            Config::default(),
        );

        let store_path = Path::new(STORE_PREFIX);
        let watcher_opt = match &mut watcher {
            Ok(w) => match w.watch(store_path, RecursiveMode::NonRecursive) {
                Ok(_) => watcher.ok(),
                Err(_) => None,
            },
            Err(_) => None,
        };

        StoreWatcher {
            _watcher: watcher_opt,
            subscriptions,
            event_receiver: rx,
            event_sender: tx,
        }
    }

    pub fn subscribe(&self, path: StorePath, payload: (Host, DerivationId)) {
        let full_path = PathBuf::from(path.to_store_path_string());
        let mut subs = self.subscriptions.lock().unwrap();
        if full_path.exists() {
            let _ = self.event_sender.send(payload);
        } else {
            subs.entry(path).or_default().push(payload);
        }
    }
}
