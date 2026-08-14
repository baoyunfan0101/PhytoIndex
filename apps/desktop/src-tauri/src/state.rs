use std::collections::{BTreeMap, HashMap, VecDeque};
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
use vividarium_core::{
    BackgroundTaskState, CoreError, Database, OperationProgress, OperationState, OperationsStatus,
};

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
    pub background_tasks: BackgroundTaskScheduler,
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
        let operations = OperationManager::new();
        Ok(Self {
            database,
            thumbnail_dir,
            background_tasks: BackgroundTaskScheduler::new(operations.clone()),
            operations,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackgroundTaskKind {
    PhotoScan,
    MetadataIndex,
    PhotoMapping,
}

impl BackgroundTaskKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::PhotoScan => "photo_scan",
            Self::MetadataIndex => "metadata_index",
            Self::PhotoMapping => "photo_mapping",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BackgroundTaskKey {
    pub kind: BackgroundTaskKind,
    pub scope: String,
}

impl BackgroundTaskKey {
    pub fn new(kind: BackgroundTaskKind, scope: impl Into<String>) -> Self {
        Self {
            kind,
            scope: scope.into(),
        }
    }
}

type BackgroundCallback = Box<
    dyn FnOnce(&mut (dyn FnMut(u64, Option<u64>, &str) + Send)) -> Result<Value, String>
        + Send
        + 'static,
>;

struct PendingBackgroundWork {
    key: BackgroundTaskKey,
    module: &'static str,
    operation: &'static str,
    callback: BackgroundCallback,
}

struct ScheduledBackgroundWork {
    task_id: String,
    pending: PendingBackgroundWork,
}

#[derive(Default)]
struct BackgroundQueue {
    queued: VecDeque<ScheduledBackgroundWork>,
    task_ids: HashMap<BackgroundTaskKey, String>,
    reruns: HashMap<BackgroundTaskKey, PendingBackgroundWork>,
}

impl BackgroundQueue {
    fn push(&mut self, work: ScheduledBackgroundWork) -> Result<(), String> {
        if let Some(existing) = self.task_ids.get(&work.pending.key) {
            return Err(existing.clone());
        }
        self.task_ids
            .insert(work.pending.key.clone(), work.task_id.clone());
        self.queued.push_back(work);
        Ok(())
    }

    fn complete(&mut self, key: &BackgroundTaskKey) -> Option<PendingBackgroundWork> {
        self.task_ids.remove(key);
        self.reruns.remove(key)
    }
}

#[derive(Clone)]
pub struct BackgroundTaskScheduler {
    queue: Arc<Mutex<BackgroundQueue>>,
    worker_active: Arc<AtomicBool>,
    operations: OperationManager,
}

impl BackgroundTaskScheduler {
    fn new(operations: OperationManager) -> Self {
        Self {
            queue: Arc::new(Mutex::new(BackgroundQueue::default())),
            worker_active: Arc::new(AtomicBool::new(false)),
            operations,
        }
    }

    pub fn enqueue<F>(
        &self,
        app: AppHandle,
        key: BackgroundTaskKey,
        module: &'static str,
        operation: &'static str,
        rerun_if_running: bool,
        callback: F,
    ) -> Result<OperationState, String>
    where
        F: FnOnce(&mut (dyn FnMut(u64, Option<u64>, &str) + Send)) -> Result<Value, String>
            + Send
            + 'static,
    {
        let pending = PendingBackgroundWork {
            key: key.clone(),
            module,
            operation,
            callback: Box::new(callback),
        };
        let state = {
            let mut queue = self.queue.lock().map_err(|error| error.to_string())?;
            if let Some(task_id) = queue.task_ids.get(&key).cloned() {
                let existing = self
                    .operations
                    .operation(&task_id)
                    .ok_or_else(|| format!("background task registry lost task {task_id}"))?;
                if existing.is_active() {
                    if rerun_if_running && existing.running {
                        queue.reruns.insert(key, pending);
                    }
                    return Ok(existing);
                }
                queue.reruns.insert(key, pending);
                return Ok(existing);
            }
            let state = self.operations.queue_background(module, operation, &key)?;
            let task_id = state
                .task_id
                .clone()
                .ok_or_else(|| "queued background task has no task id".to_string())?;
            queue.push(ScheduledBackgroundWork { task_id, pending })?;
            state
        };
        let _ = app.emit("operation-progress", state.clone());
        self.start_worker(app);
        Ok(state)
    }

    fn start_worker(&self, app: AppHandle) {
        if self
            .worker_active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let scheduler = self.clone();
        tauri::async_runtime::spawn_blocking(move || scheduler.run(app));
    }

    fn run(&self, app: AppHandle) {
        loop {
            let work = self
                .queue
                .lock()
                .ok()
                .and_then(|mut queue| queue.queued.pop_front());
            let Some(work) = work else {
                self.worker_active.store(false, Ordering::Release);
                let has_more = self
                    .queue
                    .lock()
                    .is_ok_and(|queue| !queue.queued.is_empty());
                if has_more
                    && self
                        .worker_active
                        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok()
                {
                    continue;
                }
                break;
            };
            while self
                .operations
                .blocked_by_other(work.pending.module, &work.task_id)
                .is_some()
            {
                std::thread::sleep(Duration::from_millis(25));
            }
            self.operations
                .run_queued(app.clone(), &work.task_id, work.pending.callback);
            let next = {
                let mut queue = match self.queue.lock() {
                    Ok(queue) => queue,
                    Err(_) => break,
                };
                queue.complete(&work.pending.key)
            };
            if let Some(pending) = next {
                let key = pending.key.clone();
                let state =
                    match self
                        .operations
                        .queue_background(pending.module, pending.operation, &key)
                    {
                        Ok(state) => state,
                        Err(_) => continue,
                    };
                let Some(task_id) = state.task_id.clone() else {
                    continue;
                };
                if let Ok(mut queue) = self.queue.lock() {
                    let _ = queue.push(ScheduledBackgroundWork { task_id, pending });
                }
                let _ = app.emit("operation-progress", state);
            }
        }
    }
}

pub fn set_global(state: AppState) -> Result<(), AppState> {
    GLOBAL_STATE.set(state)
}

pub fn global() -> Option<&'static AppState> {
    GLOBAL_STATE.get()
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
                task_kind: None,
                task_scope: None,
                state: BackgroundTaskState::Running,
                operation: Some(operation.into()),
                running: true,
                started_at: Some(now()),
                finished_at: None,
                message: format!("{operation} running"),
                completed: 0,
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
        state.completed = state.processed;
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
                state.state = BackgroundTaskState::Completed;
                state.message = "completed".into();
                state.result = Some(result);
                state.error = None;
            }
            Err(error) => {
                state.state = BackgroundTaskState::Failed;
                state.message = "failed".into();
                state.error = Some(error);
            }
        }
        let finished = state.clone();
        trim_finished_operations(&mut states, 50);
        Some(finished)
    }

    fn operation(&self, task_id: &str) -> Option<OperationState> {
        self.states.lock().ok()?.get(task_id).cloned()
    }

    fn queue_background(
        &self,
        module: &'static str,
        operation: &'static str,
        key: &BackgroundTaskKey,
    ) -> Result<OperationState, String> {
        let task_id = Uuid::new_v4().simple().to_string();
        let state = OperationState {
            module: module.into(),
            task_id: Some(task_id.clone()),
            task_kind: Some(key.kind.as_str().into()),
            task_scope: Some(key.scope.clone()),
            state: BackgroundTaskState::Queued,
            operation: Some(operation.into()),
            running: false,
            started_at: None,
            finished_at: None,
            message: "queued".into(),
            completed: 0,
            processed: 0,
            total: None,
            progress: None,
            result: None,
            error: None,
        };
        self.states
            .lock()
            .map_err(|error| error.to_string())?
            .insert(task_id, state.clone());
        Ok(state)
    }

    fn blocked_by_other(&self, module: &str, task_id: &str) -> Option<String> {
        let states = self.states.lock().ok()?;
        blocked_by_excluding(&states, module, Some(task_id))
    }

    fn run_queued(&self, app: AppHandle, task_id: &str, callback: BackgroundCallback) {
        let Some(running) = self.mark_running(task_id) else {
            return;
        };
        let _ = app.emit("operation-progress", running);
        let progress_manager = self.clone();
        let progress_app = app.clone();
        let progress_task_id = task_id.to_string();
        let mut throttle = ProgressEventThrottle::new();
        let mut progress = move |processed: u64, total: Option<u64>, message: &str| {
            let value = OperationProgress {
                stage: message.into(),
                current: Some(processed),
                total,
                statement_index: None,
                statement_total: None,
            };
            let emit = throttle.should_emit(processed, total, message);
            let current = progress_manager.update_progress(&progress_task_id, &value, emit);
            if let Some(current) = current {
                let _ = progress_app.emit("operation-progress", current);
            }
        };
        let result = callback(&mut progress);
        if let Some(finished) = self.finish(task_id, result) {
            let _ = app.emit("operation-progress", finished);
        }
    }

    fn mark_running(&self, task_id: &str) -> Option<OperationState> {
        let mut states = self.states.lock().ok()?;
        let state = states.get_mut(task_id)?;
        state.state = BackgroundTaskState::Running;
        state.running = true;
        state.started_at = Some(now());
        state.message = format!("{} running", state.operation.as_deref().unwrap_or("task"));
        Some(state.clone())
    }
}

fn trim_finished_operations(states: &mut OperationsStatus, limit: usize) {
    let mut finished = states
        .iter()
        .filter(|(_, state)| {
            matches!(
                state.state,
                BackgroundTaskState::Completed | BackgroundTaskState::Failed
            )
        })
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
    blocked_by_excluding(states, module, None)
}

fn blocked_by_excluding(
    states: &BTreeMap<String, OperationState>,
    module: &str,
    excluded_task_id: Option<&str>,
) -> Option<String> {
    states.values().find_map(|state| {
        if state.task_id.as_deref() == excluded_task_id {
            return None;
        }
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
                    task_kind: None,
                    task_scope: None,
                    state: BackgroundTaskState::Running,
                    operation: Some("validate_sql_import".into()),
                    running: true,
                    started_at: Some(now()),
                    finished_at: None,
                    message: "running".into(),
                    completed: 0,
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
    fn duplicate_background_task_keys_are_coalesced() {
        let key = BackgroundTaskKey::new(BackgroundTaskKind::MetadataIndex, "library-a");
        let mut queue = BackgroundQueue::default();
        queue.push(scheduled_work("task-1", key.clone())).unwrap();

        assert_eq!(
            queue.push(scheduled_work("task-2", key)).unwrap_err(),
            "task-1"
        );
        assert_eq!(queue.queued.len(), 1);
    }

    #[test]
    fn background_task_state_moves_from_queued_to_running_to_completed() {
        let manager = OperationManager::new();
        let key = BackgroundTaskKey::new(BackgroundTaskKind::PhotoScan, "library-a");
        let queued = manager
            .queue_background("photos", "photo_scan", &key)
            .unwrap();
        let task_id = queued.task_id.as_deref().unwrap();
        assert_eq!(queued.state, BackgroundTaskState::Queued);
        assert!(!queued.running);

        let running = manager.mark_running(task_id).unwrap();
        assert_eq!(running.state, BackgroundTaskState::Running);
        assert!(running.running);

        let completed = manager.finish(task_id, Ok(Value::Null)).unwrap();
        assert_eq!(completed.state, BackgroundTaskState::Completed);
        assert!(!completed.running);
    }

    #[test]
    fn completed_task_key_can_be_registered_again_for_new_work() {
        let key = BackgroundTaskKey::new(BackgroundTaskKind::PhotoMapping, "library-a");
        let mut queue = BackgroundQueue::default();
        queue.push(scheduled_work("task-1", key.clone())).unwrap();
        queue.queued.pop_front();
        assert!(queue.complete(&key).is_none());
        queue.push(scheduled_work("task-2", key)).unwrap();

        assert_eq!(queue.queued.len(), 1);
        assert_eq!(
            queue.task_ids.values().next().map(String::as_str),
            Some("task-2")
        );
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
            task_kind: None,
            task_scope: None,
            state: BackgroundTaskState::Running,
            operation: Some("test".into()),
            running: true,
            started_at: Some(now()),
            finished_at: None,
            message: "running".into(),
            completed: 0,
            processed: 0,
            total: None,
            progress: None,
            result: None,
            error: None,
        }
    }

    fn scheduled_work(task_id: &str, key: BackgroundTaskKey) -> ScheduledBackgroundWork {
        ScheduledBackgroundWork {
            task_id: task_id.into(),
            pending: PendingBackgroundWork {
                key,
                module: "photos",
                operation: "test",
                callback: Box::new(|_| Ok(Value::Null)),
            },
        }
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
