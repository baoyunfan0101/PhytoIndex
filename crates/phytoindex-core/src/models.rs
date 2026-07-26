use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Photo {
    pub photo_id: i64,
    pub directory_id: i64,
    pub relative_path: String,
    pub filename: String,
    pub file_size: i64,
    pub modified_at_ns: i64,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhotoMetadata {
    pub photo_id: i64,
    pub captured_at: Option<String>,
    pub camera: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub longitude: Option<f64>,
    pub latitude: Option<f64>,
    pub exif_json: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NewPhoto {
    pub directory_id: i64,
    pub filename: String,
    pub file_size: i64,
    pub modified_at_ns: i64,
    pub thumbnail_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhotoLibrary {
    pub root_path: String,
    pub root_directory_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhotoDirectory {
    pub directory_id: i64,
    pub parent_directory_id: Option<i64>,
    pub name: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PhotoPage<T> {
    pub items: Vec<T>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PhotoDirectoryItem {
    Directory { directory: PhotoDirectory },
    Photo { photo: Photo },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DirectoryEntryCounts {
    pub directory_count: i64,
    pub file_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MappingMetadata {
    pub mapped_photo_count: i64,
    pub unmatched_photo_count: i64,
    pub ambiguous_photo_count: i64,
    pub processing_photo_count: i64,
    pub mapping_taxa_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OperationState {
    pub module: String,
    pub task_id: Option<String>,
    pub operation: Option<String>,
    pub running: bool,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub message: String,
    pub processed: u64,
    pub total: Option<u64>,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

pub type OperationsStatus = BTreeMap<String, OperationState>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OperationInputTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoSyncResult {
    pub directory_id: i64,
    pub inserted: usize,
    pub unchanged: usize,
    pub updated: usize,
    pub deleted: usize,
    pub directories_inserted: usize,
    pub directories_deleted: usize,
}
