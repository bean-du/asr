use anyhow::Result;
use serde::{Serialize, Deserialize};
use async_trait::async_trait;

pub mod whisper;    

#[derive(Debug, Clone)]
pub struct AsrParams {
    pub language: Option<String>,
    pub single_segment: bool,
    pub speaker_diarization: bool,
    pub emotion_recognition: bool,
    pub filter_dirty_words: bool,
}

impl AsrParams {
    pub fn new() -> Self {
        Self {
            language: None,
            single_segment: false,
            speaker_diarization: false,
            emotion_recognition: false,
            filter_dirty_words: false,
        }
    }

    pub fn set_language(&mut self, language: Option<String>) -> &Self {
        self.language = language;
        self
    }

    pub fn set_single_segment(&mut self, single_segment: bool) -> &Self {
        self.single_segment = single_segment;
        self
    }

    pub fn set_speaker_diarization(&mut self, speaker_diarization: bool) -> &Self {
        self.speaker_diarization = speaker_diarization;
        self
    }

    pub fn set_emotion_recognition(&mut self, emotion_recognition: bool) -> &Self {
        self.emotion_recognition = emotion_recognition;
        self
    }

    pub fn set_filter_dirty_words(&mut self, filter_dirty_words: bool) -> &Self {
        self.filter_dirty_words = filter_dirty_words;
        self
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscribeSegment {
    pub text: String,
    pub speaker_id: usize,    
    pub start: f64,    
    pub end: f64,      
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TranscribeResult {
    pub segments: Vec<TranscribeSegment>,
    pub full_text: String,
}

#[async_trait]
pub trait AsrEngine: Send + Sync {
    async fn transcribe(&self, audio: Vec<f32>, params: AsrParams) -> Result<TranscribeResult>;
}