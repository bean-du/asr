use axum::{
    http::{StatusCode, HeaderMap},
    Json,
    extract::State,
    routing::post,
    Router,
    response::IntoResponse,
};
use crate::utils::http::HttpResponse;
use crate::AppContext;
use tracing::{info, error};
use crate::auth::Permission;
use crate::utils::http::download_audio;
use std::path::PathBuf;
use std::sync::Arc;
use crate::schedule::TaskConfig;
use crate::schedule::TaskType;
use crate::schedule::CallbackType;
use crate::schedule::TaskPriority;
use crate::schedule::TaskParams;
use crate::schedule::TranscribeParams;
use serde::{Deserialize, Serialize};



pub fn transcribe_router(ctx: Arc<AppContext>) -> Router {
    Router::new()
        .route("/transcribe", post(transcribe))
        .with_state(ctx)
}


#[derive(Debug, Deserialize, Serialize)]
pub struct TranscribeRequest {
    pub audio_url: String,
    pub callback_url: String,
    pub language: Option<String>,
    // optional features
    pub speaker_diarization: bool,
    pub emotion_recognition: bool,
    pub filter_dirty_words: bool,
}

pub async fn transcribe(
    State(ctx): State<Arc<AppContext>>,
    headers: HeaderMap,
    Json(req): Json<TranscribeRequest>,
) -> impl IntoResponse {
    // validate api key
    let api_key = headers.get("Authorization")
        .and_then(|value| value.to_str().ok());

    if let Err(e) = ctx.auth.verify_api_key(api_key, Permission::Transcribe).await {
        let response = HttpResponse::new(
            401,
            "Authentication failed".to_string(),
            e.to_string()
        );
        return (StatusCode::UNAUTHORIZED, Json(response)).into_response();
    }

    // download audio file
    let dest = match download_audio(&req.audio_url, &PathBuf::from("./asr/data/")).await {
        Ok(dest) => dest,
        Err(e) => {
            error!("Failed to download audio: {}", e);
            let response = HttpResponse::new(
                500,
                "Failed to download audio".to_string(),
                e.to_string()
            );
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response();
        }
    };

    let task_config = TaskConfig{
        task_type: TaskType::Transcribe,
        input_path: dest,
        callback_type: CallbackType::Http { url: req.callback_url },
        params: TaskParams::Transcribe(TranscribeParams{
            language: req.language,
            speaker_diarization: req.speaker_diarization,
            emotion_recognition: req.emotion_recognition,
            filter_dirty_words: req.filter_dirty_words,
        }),
        priority: TaskPriority::Normal,
        retry_count: 0,
        max_retries: 3,
        timeout: None,
    };

    if let Err(e) = ctx.task_manager.create_task(task_config).await {
        error!("Failed to create task: {}", e);
        let response = HttpResponse::new(
            500,
            "Failed to create task".to_string(),
            e.to_string()
        );
        return (StatusCode::INTERNAL_SERVER_ERROR, Json(response)).into_response();
    }

    info!("Task added successfully: {}", req.audio_url);
    let response = HttpResponse::new(
        0,
        "Task added successfully".to_string(),
        req.audio_url
    );
    (StatusCode::OK, Json(response)).into_response()
}

