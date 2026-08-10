use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use chrono::Local;
use serde_json::Value;
use tauri::{AppHandle, Emitter};
use uuid::Uuid;
use vividarium_core::taxonomy::{PreparedTaxonomyUpdate, TaxonomyPreviewResult};
use vividarium_core::{CoreError, Database, OperationProgress, OperationState, OperationsStatus};

static GLOBAL_STATE: OnceLock<AppState> = OnceLock::new();
const PROGRESS_EVENT_INTERVAL: Duration = Duration::from_millis(100);

struct ProgressEventThrottle {
    last_emitted_at: Option<Instant>,
    last_message: Option<String>,
    last_processed: Option<u64>,
    last_total: Option<u64>,
}

impl ProgressEventThrottle {
    fn new() -> Self {
        Self {
            last_emitted_at: None,
            last_message: None,
            last_processed: None,
            last_total: None,
        }
    }

    fn should_emit(&mut self, processed: u64, total: Option<u64>, message: &str) -> bool {
        let first = self.last_emitted_at.is_none();
        let phase_changed =
            self.last_message.as_deref() != Some(message) || self.last_total != total;
        let completed =
            total.is_some_and(|value| processed >= value) && self.last_processed != Some(processed);
        let interval_elapsed = self
            .last_emitted_at
            .is_some_and(|instant| instant.elapsed() >= PROGRESS_EVENT_INTERVAL);
        let emit = first || phase_changed || completed || interval_elapsed;
        if emit {
            self.last_emitted_at = Some(Instant::now());
            self.last_message = Some(message.into());
            self.last_processed = Some(processed);
            self.last_total = total;
        }
        emit
    }
}

#[derive(Clone)]
pub struct AppState {
    pub database: Database,
    pub thumbnail_dir: PathBuf,
    pub operations: OperationManager,
    pub taxonomy_sync: DeferredWork,
    photo_library_lifecycle: Arc<Mutex<()>>,
    formatted_update_preview: Arc<Mutex<Option<StagedFormattedUpdate>>>,
}

#[derive(Debug)]
struct StagedFormattedUpdate {
    preview_id: String,
    prepared: PreparedTaxonomyUpdate,
}

impl AppState {
    pub fn new(data_dir: PathBuf) -> Result<Self, vividarium_core::CoreError> {
        let database = Database::open(data_dir.join("metadata.db"))?;
        let thumbnail_dir = data_dir.join("thumbnails");
        if database.active_photo_library()?.is_some() {
            let _ = vividarium_core::photos::rebase_thumbnail_paths(&database, &thumbnail_dir);
        }
        Ok(Self {
            database,
            thumbnail_dir,
            operations: OperationManager::new(),
            taxonomy_sync: DeferredWork::new(),
            photo_library_lifecycle: Arc::new(Mutex::new(())),
            formatted_update_preview: Arc::new(Mutex::new(None)),
        })
    }

    pub fn lock_photo_library_lifecycle(&self) -> Result<MutexGuard<'_, ()>, String> {
        self.photo_library_lifecycle
            .lock()
            .map_err(|_| "photo library lifecycle lock is poisoned".to_string())
    }

    pub fn replace_formatted_update_preview(
        &self,
        prepared: PreparedTaxonomyUpdate,
    ) -> Result<(String, TaxonomyPreviewResult), CoreError> {
        let preview_id = Uuid::new_v4().to_string();
        let preview = prepared.preview_result().clone();
        let mut current = self.formatted_update_preview.lock().map_err(|_| {
            CoreError::Consistency("formatted update preview lock is poisoned".into())
        })?;
        *current = Some(StagedFormattedUpdate {
            preview_id: preview_id.clone(),
            prepared,
        });
        Ok((preview_id, preview))
    }

    pub fn take_formatted_update_preview(
        &self,
        preview_id: &str,
    ) -> Result<PreparedTaxonomyUpdate, CoreError> {
        let mut current = self.formatted_update_preview.lock().map_err(|_| {
            CoreError::Consistency("formatted update preview lock is poisoned".into())
        })?;
        if current.as_ref().map(|value| value.preview_id.as_str()) != Some(preview_id) {
            return Err(CoreError::InvalidArgument(
                "formatted update preview is no longer current; preview again".into(),
            ));
        }
        current
            .take()
            .map(|value| value.prepared)
            .ok_or_else(|| CoreError::Consistency("formatted update preview disappeared".into()))
    }

    pub fn clear_formatted_update_preview(&self) -> Result<(), CoreError> {
        let mut current = self.formatted_update_preview.lock().map_err(|_| {
            CoreError::Consistency("formatted update preview lock is poisoned".into())
        })?;
        *current = None;
        Ok(())
    }
}

pub fn set_global(state: AppState) -> Result<(), AppState> {
    GLOBAL_STATE.set(state)
}

pub fn global() -> Option<&'static AppState> {
    GLOBAL_STATE.get()
}

#[derive(Clone)]
pub struct DeferredWork {
    requested: Arc<AtomicBool>,
    worker_active: Arc<AtomicBool>,
}

impl DeferredWork {
    fn new() -> Self {
        Self {
            requested: Arc::new(AtomicBool::new(false)),
            worker_active: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn request(&self) -> bool {
        self.requested.store(true, Ordering::Release);
        self.worker_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    pub fn take_request(&self) -> bool {
        self.requested.swap(false, Ordering::AcqRel)
    }

    pub fn release_or_continue(&self) -> bool {
        self.worker_active.store(false, Ordering::Release);
        self.requested.load(Ordering::Acquire)
            && self
                .worker_active
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
    }
}

#[derive(Clone)]
pub struct OperationManager {
    states: Arc<Mutex<OperationsStatus>>,
}

impl OperationManager {
    fn new() -> Self {
        Self {
            states: Arc::new(Mutex::new(OperationsStatus::new())),
        }
    }

    pub fn status(&self) -> OperationsStatus {
        self.states
            .lock()
            .expect("operation state lock poisoned")
            .clone()
    }

    pub fn start<F>(
        &self,
        app: AppHandle,
        module: &'static str,
        operation: &'static str,
        callback: F,
    ) -> Result<OperationState, String>
    where
        F: FnOnce(&mut (dyn FnMut(u64, Option<u64>, &str) + Send)) -> Result<Value, String>
            + Send
            + 'static,
    {
        self.start_with_progress(app, module, operation, move |report| {
            let mut legacy_report = |processed: u64, total: Option<u64>, message: &str| {
                report(OperationProgress {
                    stage: message.into(),
                    current: Some(processed),
                    total,
                    statement_index: None,
                    statement_total: None,
                });
            };
            callback(&mut legacy_report)
        })
    }

    pub fn start_with_progress<F>(
        &self,
        app: AppHandle,
        module: &'static str,
        operation: &'static str,
        callback: F,
    ) -> Result<OperationState, String>
    where
        F: FnOnce(&mut (dyn FnMut(OperationProgress) + Send)) -> Result<Value, String>
            + Send
            + 'static,
    {
        let task_id = Uuid::new_v4().simple().to_string();
        let state = {
            let mut states = self.states.lock().map_err(|error| error.to_string())?;
            if let Some(blocked_by) = blocked_by(&states, module) {
                return Err(format!("{module} is blocked by {blocked_by}"));
            }
            let state = OperationState {
                module: module.into(),
                task_id: Some(task_id.clone()),
                operation: Some(operation.into()),
                running: true,
                started_at: Some(now()),
                finished_at: None,
                message: format!("{operation} running"),
                processed: 0,
                total: None,
                progress: None,
                result: None,
                error: None,
            };
            states.insert(task_id.clone(), state.clone());
            state
        };
        let manager = self.clone();
        tauri::async_runtime::spawn_blocking(move || {
            let progress_manager = manager.clone();
            let progress_app = app.clone();
            let progress_task_id = task_id.clone();
            let mut throttle = ProgressEventThrottle::new();
            let mut progress = move |progress: OperationProgress| {
                let processed = progress.current.unwrap_or(0);
                let emit = throttle.should_emit(processed, progress.total, &progress.stage);
                let current = progress_manager.update_progress(&progress_task_id, &progress, emit);
                if let Some(current) = current {
                    let _ = progress_app.emit("operation-progress", current);
                }
            };
            let result = callback(&mut progress);
            let finished = manager.finish(&task_id, result);
            if let Some(finished) = finished {
                let _ = app.emit("operation-progress", finished);
            }
        });
        Ok(state)
    }

    fn update_progress(
        &self,
        task_id: &str,
        progress: &OperationProgress,
        snapshot: bool,
    ) -> Option<OperationState> {
        let mut states = self.states.lock().ok()?;
        let state = states.get_mut(task_id)?;
        if !state.running {
            return None;
        }
        state.processed = progress.current.unwrap_or(0);
        state.total = progress.total;
        state.message = progress.stage.clone();
        state.progress = Some(progress.clone());
        snapshot.then(|| state.clone())
    }

    fn finish(&self, task_id: &str, result: Result<Value, String>) -> Option<OperationState> {
        let mut states = self.states.lock().ok()?;
        let state = states.get_mut(task_id)?;
        state.running = false;
        state.finished_at = Some(now());
        match result {
            Ok(result) => {
                state.message = "completed".into();
                state.result = Some(result);
                state.error = None;
            }
            Err(error) => {
                state.message = "failed".into();
                state.error = Some(error);
            }
        }
        let finished = state.clone();
        trim_finished_operations(&mut states, 50);
        Some(finished)
    }
}

fn trim_finished_operations(states: &mut OperationsStatus, limit: usize) {
    let mut finished = states
        .iter()
        .filter(|(_, state)| !state.running)
        .map(|(task_id, state)| (task_id.clone(), state.finished_at.clone()))
        .collect::<Vec<_>>();
    if finished.len() <= limit {
        return;
    }
    finished.sort_by(|left, right| left.1.cmp(&right.1));
    let remove_count = finished.len() - limit;
    for (task_id, _) in finished.into_iter().take(remove_count) {
        states.remove(&task_id);
    }
}

fn blocked_by(states: &BTreeMap<String, OperationState>, module: &str) -> Option<String> {
    states.values().find_map(|state| {
        let other = state.module.as_str();
        let taxonomy_import_conflict = matches!(module, "sql_import" | "direct_import")
            && matches!(other, "sql_import" | "direct_import");
        (state.running
            && (module == other
                || module == "mapping"
                || other == "mapping"
                || taxonomy_import_conflict))
            .then(|| other.to_string())
    })
}

fn now() -> String {
    Local::now().format("%Y-%m-%d %H:%M:%S%.6f").to_string()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn progress_throttle_emits_first_phase_changes_and_completion() {
        let mut throttle = ProgressEventThrottle::new();

        assert!(throttle.should_emit(0, None, "Reading"));
        assert!(!throttle.should_emit(1, None, "Reading"));
        assert!(throttle.should_emit(0, Some(100), "Importing"));
        assert!(!throttle.should_emit(1, Some(100), "Importing"));
        assert!(throttle.should_emit(100, Some(100), "Importing"));
        assert!(throttle.should_emit(100, None, "Committing"));
    }

    #[test]
    fn structured_progress_is_retained_in_operation_status() {
        let manager = OperationManager::new();
        {
            let mut states = manager.states.lock().unwrap();
            states.insert(
                "validate-1".into(),
                OperationState {
                    module: "sql_import".into(),
                    task_id: Some("validate-1".into()),
                    operation: Some("validate_sql_import".into()),
                    running: true,
                    started_at: Some(now()),
                    finished_at: None,
                    message: "running".into(),
                    processed: 0,
                    total: None,
                    progress: None,
                    result: None,
                    error: None,
                },
            );
        }
        let progress = OperationProgress {
            stage: "executing_sql".into(),
            current: Some(2),
            total: Some(7),
            statement_index: Some(2),
            statement_total: Some(7),
        };

        let state = manager
            .update_progress("validate-1", &progress, true)
            .unwrap();

        assert_eq!(state.message, "executing_sql");
        assert_eq!(state.processed, 2);
        assert_eq!(state.total, Some(7));
        assert_eq!(state.progress, Some(progress));
    }

    #[test]
    fn a_sql_import_operation_blocks_another_sql_import_operation() {
        let mut states = OperationsStatus::new();
        states.insert("task-1".into(), running_state("sql_import"));

        assert_eq!(
            blocked_by(&states, "sql_import").as_deref(),
            Some("sql_import")
        );
    }

    #[test]
    fn sql_and_direct_import_operations_block_each_other() {
        let mut states = OperationsStatus::new();
        states.insert("task-1".into(), running_state("sql_import"));
        assert_eq!(
            blocked_by(&states, "direct_import").as_deref(),
            Some("sql_import")
        );

        states.get_mut("task-1").unwrap().running = false;
        states.insert("task-2".into(), running_state("direct_import"));
        assert_eq!(
            blocked_by(&states, "sql_import").as_deref(),
            Some("direct_import")
        );
    }

    fn running_state(module: &str) -> OperationState {
        OperationState {
            module: module.into(),
            task_id: Some(format!("{module}-task")),
            operation: Some("test".into()),
            running: true,
            started_at: Some(now()),
            finished_at: None,
            message: "running".into(),
            processed: 0,
            total: None,
            progress: None,
            result: None,
            error: None,
        }
    }

    #[test]
    fn deferred_work_coalesces_requests_and_restarts_after_release() {
        let work = DeferredWork::new();

        assert!(work.request());
        assert!(!work.request());
        assert!(work.take_request());
        assert!(!work.take_request());
        assert!(!work.request());
        assert!(work.release_or_continue());
        assert!(work.take_request());
        assert!(!work.release_or_continue());
        assert!(work.request());
    }

    #[test]
    fn startup_tolerates_a_missing_active_photo_library() {
        let data_dir =
            std::env::temp_dir().join(format!("vividarium-state-{}", Uuid::new_v4().simple()));
        let root = data_dir.join("photos");
        fs::create_dir_all(&root).unwrap();
        let library_path = data_dir.join("library.db");
        {
            let database = Database::open(data_dir.join("metadata.db")).unwrap();
            database
                .register_photo_library(&root, &library_path, Some("Library"))
                .unwrap();
        }
        fs::remove_file(&library_path).unwrap();

        let state = AppState::new(data_dir.clone()).unwrap();

        assert!(state.database.active_photo_library().unwrap().is_some());
        assert!(!library_path.exists());
        fs::remove_dir_all(data_dir).unwrap();
    }
}
