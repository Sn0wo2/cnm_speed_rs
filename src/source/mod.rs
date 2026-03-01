use crate::app::types::Args;
use crate::speedtest::types::{ActiveTestHandle, ProgressEvent, RuntimeConfig};
use anyhow::Result;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

pub mod cmcc;
pub mod cmcc_types;

#[derive(Debug, Clone)]
pub struct SourceSelection {
    pub base_url: String,
    pub label: String,
    pub prefetched_ip: String,
}

#[async_trait::async_trait]
pub trait SpeedSource: Send + Sync {
    async fn detect(&self, args: &Args, tx: &mpsc::Sender<ProgressEvent>) -> Result<SourceSelection>;

    fn spawn_test(
        &self,
        selection: &SourceSelection,
        cfg: RuntimeConfig,
        node_id_override: Option<String>,
        tx: mpsc::Sender<ProgressEvent>,
        prefetched_ip: Option<String>,
    ) -> ActiveTestHandle;

    async fn run_test(
        &self,
        selection: &SourceSelection,
        cfg: RuntimeConfig,
        node_id_override: Option<String>,
        tx: mpsc::Sender<ProgressEvent>,
        stop: Arc<AtomicBool>,
        prefetched_ip: Option<String>,
    );
}

pub fn default_source() -> Arc<dyn SpeedSource> {
    Arc::new(cmcc::CmccSource::new())
}

type SelectionSlot = Arc<Mutex<Option<SourceSelection>>>;
type ActiveHandleSlot = Arc<Mutex<Option<ActiveTestHandle>>>;

#[derive(Clone)]
pub struct SourceRuntime {
    source: Arc<dyn SpeedSource>,
    selection_slot: SelectionSlot,
    active_handle: ActiveHandleSlot,
    tx: mpsc::Sender<ProgressEvent>,
}

impl SourceRuntime {
    pub fn new(tx: mpsc::Sender<ProgressEvent>) -> Self {
        Self {
            source: default_source(),
            selection_slot: Arc::new(Mutex::new(None)),
            active_handle: Arc::new(Mutex::new(None)),
            tx,
        }
    }

    pub fn bootstrap_detection(&self, args: Args) {
        let source = Arc::clone(&self.source);
        let selection_slot = Arc::clone(&self.selection_slot);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let selection = match source.detect(&args, &tx).await {
                Ok(v) => v,
                Err(err) => {
                    let _ = tx.send(ProgressEvent::Status(format!(
                        "Detect failed: {:#}",
                        err.context("source.detect")
                    ))).await;
                    return;
                }
            };

            {
                let mut slot = selection_slot.lock().unwrap();
                *slot = Some(selection.clone());
            }

            let _ = tx.send(ProgressEvent::ServerSelected {
                base_url: selection.base_url.clone(),
                province_label: selection.label.clone(),
            }).await;

            let init_source = Arc::clone(&source);
            let init_selection = selection.clone();
            let init_tx = tx.clone();
            let init_ip = selection.prefetched_ip;
            tokio::spawn(async move {
                init_source.run_test(
                    &init_selection,
                    RuntimeConfig {
                        duration_sec: 0,
                        ..Default::default()
                    },
                    None,
                    init_tx,
                    Arc::new(AtomicBool::new(false)),
                    Some(init_ip),
                ).await;
            });
        });
    }

    pub fn spawn_test(&self, runtime_cfg: RuntimeConfig, node: Option<String>) {
        let selection_opt = {
            let slot = self.selection_slot.lock().unwrap();
            slot.clone()
        };

        if let Some(selection) = selection_opt {
            let handle =
                self.source
                    .spawn_test(&selection, runtime_cfg, node, self.tx.clone(), None);
            let mut active = self.active_handle.lock().unwrap();
            *active = Some(handle);
        } else {
            let tx = self.tx.clone();
            tokio::spawn(async move {
                let _ = tx.send(ProgressEvent::Status(
                    "Server not ready yet (still detecting)...".into(),
                )).await;
            });
        }
    }

    pub fn is_ready(&self) -> bool {
        let slot = self.selection_slot.lock().unwrap();
        slot.is_some()
    }

    pub fn stop_test(&self) {
        let mut active = self.active_handle.lock().unwrap();
        if let Some(handle) = active.as_ref() {
            handle.stop();
        }
        *active = None;
    }
}
