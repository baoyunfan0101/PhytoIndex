use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::error::{CoreError, CoreResult};

const MAX_PAGE_LIMIT: usize = 500;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PhotoPageSection {
    Containers,
    Photos,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum PhotoCursor {
    DirectoryEntries {
        directory_id: i64,
        section: PhotoPageSection,
        name: String,
        item_id: i64,
    },
    TaxonEntries {
        taxon_id: Option<i64>,
        show_empty: bool,
        include_descendants: bool,
        section: PhotoPageSection,
        rank: i64,
        item_id: i64,
    },
    MappingStatus {
        status: String,
        photo_id: i64,
    },
    MappingStatusSearch {
        status: String,
        query: String,
        photo_id: i64,
    },
    FilenameSearch {
        query: String,
        photo_id: i64,
    },
    GeneralSearch {
        query: String,
        photo_id: i64,
    },
    MapPhotos {
        bounds: Option<[u64; 4]>,
        photo_id: i64,
    },
    TaxonSearch {
        query: String,
        match_level: i64,
        edit_distance: i64,
        sort_name: String,
        name_type_priority: i64,
        taxon_id: i64,
    },
    TaxonPhotos {
        taxon_id: i64,
        photo_id: i64,
    },
    Operations {
        operation_id: i64,
    },
}

pub(crate) fn photo_page_limit(limit: usize) -> usize {
    limit.clamp(1, MAX_PAGE_LIMIT)
}

pub(crate) fn encode_photo_cursor(cursor: &PhotoCursor) -> CoreResult<String> {
    let value = serde_json::to_vec(cursor)
        .map_err(|error| CoreError::InvalidArgument(error.to_string()))?;
    Ok(base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value))
}

pub(crate) fn decode_photo_cursor(value: Option<&str>) -> CoreResult<Option<PhotoCursor>> {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| invalid_photo_cursor())?;
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| invalid_photo_cursor())
}

pub(crate) fn invalid_photo_cursor() -> CoreError {
    CoreError::InvalidArgument("invalid photo cursor".into())
}
