use async_trait::async_trait;
use anyhow::Result;
use std::sync::Arc;
use tracing::{info, warn};

use crate::asr::{whisper::WhisperAsr, AsrParams, AsrEngine};
use crate::schedule::types::{
    Task, TaskType, TaskResult, TaskParams, TranscribeParams,
    TranscribeResult, TranscribeSegment
};
use super::TaskProcessor;

#[derive(Clone)]
pub struct TranscribeProcessor {
    asr: Arc<WhisperAsr>,
}

impl TranscribeProcessor {
    pub fn new(asr: Arc<WhisperAsr>) -> Self {
        Self { asr }
    }

    async fn process_audio(&self, task: &Task, params: &TranscribeParams) -> Result<TranscribeResult> {
        info!("Processing audio file: {}", task.config.input_path.display());

        // set asr params
        let mut asr_params = AsrParams::new();
        asr_params.set_language(params.language.clone());
        asr_params.set_speaker_diarization(params.speaker_diarization);
        asr_params.set_emotion_recognition(params.emotion_recognition);
        asr_params.set_filter_dirty_words(params.filter_dirty_words);

        // process audio file
        let audio = crate::audio::parse_audio_file(&task.config.input_path, true, 0.75)?;
        let asr_result = self.asr.transcribe(audio, asr_params).await?;

        // convert result format
        Ok(TranscribeResult {
            text: asr_result.full_text,
            segments: asr_result.segments.into_iter().map(|s| TranscribeSegment {
                text: s.text,
                speaker_id: Some(s.speaker_id),
                start_time: s.start,
                end_time: s.end,
            }).collect(),
        })
    }
}

#[async_trait]
impl TaskProcessor for TranscribeProcessor {
    fn task_type(&self) -> TaskType {
        TaskType::Transcribe
    }

    async fn process(&self, task: &Task) -> Result<TaskResult> {
        let params = match &task.config.params {
            TaskParams::Transcribe(p) => p,
            _ => return Err(anyhow::anyhow!("Invalid task params")),
        };

        info!("Processing transcribe task {} with params: {:?}", task.id, params);

        match self.process_audio(task, params).await {
            Ok(result) => {
                info!("Successfully processed task {}", task.id);
                Ok(TaskResult::Transcribe(result))
            }
            Err(e) => {
                warn!("Failed to process task {}: {}", task.id, e);
                Err(e)
            }
        }
    }

    fn validate_params(&self, params: &TaskParams) -> Result<()> {
        match params {
            TaskParams::Transcribe(p) => {
                // validate language parameter
                if let Some(lang) = &p.language {
                    if !["zh", "en", "ja"].contains(&lang.as_str()) {
                        return Err(anyhow::anyhow!("Unsupported language: {}", lang));
                    }
                }

                // validate input file - get from TaskConfig
                if let TaskParams::Transcribe(_) = params {
                    // note: validation should be done when creating task, because we cannot access TaskConfig here
                    // we only validate language parameter here
                    Ok(())
                } else {
                    Err(anyhow::anyhow!("Invalid task params type"))
                }
            }
            _ => Err(anyhow::anyhow!("Invalid task params type")),
        }
    }

    async fn cancel(&self, task: &Task) -> Result<()> {
        // ASR does not support canceling ongoing tasks
        warn!("Cancel operation is not supported for task {}", task.id);
        Ok(())
    }

    async fn cleanup(&self, task: &Task) -> Result<()> {
        // clean up temporary file
        if task.config.input_path.exists() {
            info!("Cleaning up temporary file: {}", task.config.input_path.display());
            if let Err(e) = std::fs::remove_file(&task.config.input_path) {
                warn!("Failed to remove temporary file: {}", e);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schedule::types::TaskConfig;
    use std::path::PathBuf;
    use crate::schedule::types::{CallbackType, TaskParams, TaskPriority, TaskStatus};
    use chrono::Utc;
    use crate::schedule::types::TranscribeParams;
    use crate::asr::whisper::WhisperAsr;

    #[tokio::test]
    async fn test_transcribe_processor() -> Result<()> {
        let test_file = PathBuf::from("./test/1.wav");

        // create processor
        let asr = Arc::new(WhisperAsr::new("./models/ggml-large-v3.bin".to_string())?);
        let processor = TranscribeProcessor::new(asr);

        // create test task
        let task = Task {
            id: "test-task".to_string(),
            status: TaskStatus::Pending,
            config: TaskConfig {
                task_type: TaskType::Transcribe,
                input_path: test_file.clone(),
                callback_type: CallbackType::Http { url: "http://localhost:8000/callback".to_string() },
                params: TaskParams::Transcribe(TranscribeParams {
                    language: Some("zh".to_string()),
                    speaker_diarization: true,
                    emotion_recognition: false,
                    filter_dirty_words: false,
                }),
                priority: TaskPriority::Normal,
                retry_count: 0,
                max_retries: 3,
                timeout: Some(300),
            },
            created_at: Utc::now(),
            updated_at: Utc::now(),
            started_at: None,
            completed_at: None,
            result: None,
            error: None,
        };

        // validate params
        processor.validate_params(&task.config.params)?;

        // process task
        let result = processor.process(&task).await?;

        // validate result
        match result {
            TaskResult::Transcribe(result) => {
                assert!(!result.text.is_empty());
                assert!(!result.segments.is_empty());
            }
            _ => panic!("Unexpected result type"),
        }

        // clean up test file
        processor.cleanup(&task).await?;
        assert!(!test_file.exists());

        Ok(())
    }
} 