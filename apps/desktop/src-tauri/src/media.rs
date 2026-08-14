use std::fs;

use tauri::http::{Request, Response, StatusCode};

use crate::state;

pub fn handle(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    match response(request.uri().path()) {
        Ok(response) => response,
        Err((status, message)) => Response::builder()
            .status(status)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(message.into_bytes())
            .expect("valid media error response"),
    }
}

fn response(path: &str) -> Result<Response<Vec<u8>>, (StatusCode, String)> {
    let state = state::global().ok_or((
        StatusCode::SERVICE_UNAVAILABLE,
        "application state is not ready".into(),
    ))?;
    let (resource, library_uuid, photo_id) = parse_media_path(path)?;
    let active_library_uuid = state
        .database
        .active_photo_library()
        .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()))?
        .map(|library| library.library_uuid)
        .ok_or((
            StatusCode::NOT_FOUND,
            "no active photo library is registered".into(),
        ))?;
    if library_uuid != active_library_uuid {
        return Err((
            StatusCode::CONFLICT,
            "media request does not belong to the active photo library".into(),
        ));
    }
    let file = match resource.as_str() {
        "photo" => vividarium_core::photos::photo_file_path_for_library(
            &state.database,
            &library_uuid,
            photo_id,
        ),
        "thumbnail" => vividarium_core::photos::get_or_create_thumbnail_for_library(
            &state.database,
            &library_uuid,
            photo_id,
            &state.thumbnail_dir,
        ),
        _ => return Err((StatusCode::NOT_FOUND, "unknown media resource".into())),
    }
    .map_err(|error| (StatusCode::NOT_FOUND, error.to_string()))?;
    let content_type = mime_guess::from_path(&file)
        .first_or_octet_stream()
        .essence_str()
        .to_string();
    let body = fs::read(file).map_err(|error| (StatusCode::NOT_FOUND, error.to_string()))?;
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", content_type)
        .header("Cache-Control", "public, max-age=31536000, immutable")
        .body(body)
        .expect("valid media response"))
}

fn parse_media_path(path: &str) -> Result<(String, String, i64), (StatusCode, String)> {
    let decoded_path = percent_encoding::percent_decode_str(path).decode_utf8_lossy();
    let parts = decoded_path
        .trim_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    if parts.len() != 3 || parts[1].is_empty() {
        return Err((StatusCode::BAD_REQUEST, "invalid media URL".into()));
    }
    let photo_id = parts[2]
        .parse::<i64>()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid photo id".into()))?;
    if photo_id <= 0 {
        return Err((StatusCode::BAD_REQUEST, "invalid photo id".into()));
    }
    Ok((parts[0].to_string(), parts[1].to_string(), photo_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_paths_require_a_library_identity() {
        assert_eq!(
            parse_media_path("/thumbnail/library-a/42").unwrap(),
            ("thumbnail".into(), "library-a".into(), 42)
        );
        assert!(parse_media_path("/thumbnail/42").is_err());
        assert!(parse_media_path("/photo/library-a/0").is_err());
    }
}
