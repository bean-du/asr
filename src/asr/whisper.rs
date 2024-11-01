use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};
use anyhow::Result;
use crate::asr::{AsrEngine, AsrParams, TranscribeResult, TranscribeSegment};

pub struct WhisperAsr {
    whisper_ctx: WhisperContext,
}

impl WhisperAsr {
    pub fn new(model_path: String) -> Result<Self> {
        match WhisperContext::new_with_params(&model_path, WhisperContextParameters::default()) {
            Ok(whisper_ctx) => Ok(Self { whisper_ctx }),
            Err(e) => Err(anyhow::anyhow!("failed to open whisper model: {}", e)),
        }
    }

    fn build_params(&self, ap: AsrParams) -> FullParams {
        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });

        // 启用说话人分离
        params.set_tdrz_enable(ap.speaker_diarization);

        // 设置单段模式。如果设为true，会将音频分成多个段落进行识别
        params.set_single_segment(ap.single_segment);

        // 设置采样温度。较低的值会使输出更加确定，较高的值会增加随机性
        params.set_temperature(0.3);

        // 设置使用的线程数，提高并行处理能力
        params.set_n_threads(8);

        // 设置打印进度
        params.set_print_progress(true);

        // 设置音频上下文大小，提高识别准确度
        // params.set_audio_ctx(600);

        // 禁用翻译功能。如果设为true，会将识别结果翻译为英语
        params.set_translate(false);

        // 启用打印特殊标记。这可能包括非语音声音、停顿等
        params.set_print_special(false);

        // 启用打印进度。在处理过程中会显示进度信息
        params.set_print_progress(true);

        // 启用实时打印。识别结果会实时输出，而不是等待全部处理完成
        params.set_print_realtime(true);

        // 禁用无上下文模式。启用上下文可以提高长音频的识别准确度
        params.set_no_context(false);

        // 禁用单段模式。允许将音频分成多个段落进行识别
        params.set_single_segment(false);

        // 启用制空白。这可以减少输出中的无意义空白
        params.set_suppress_blank(true);

        // 启用抑制非语音标记。这可以过滤掉一些非语音的声音
        params.set_suppress_non_speech_tokens(true);

        // 设置处理的音频长度（毫秒）。0表示处理整个音频
        params.set_duration_ms(0);

        // 设置初始时间戳的最大值。这可以影响分段的起始时间
        params.set_max_initial_ts(1.0);
       
        params
    }
}

#[async_trait::async_trait]
impl AsrEngine for WhisperAsr {
    async fn transcribe(&self, audio: Vec<f32>, user_params: AsrParams) -> Result<TranscribeResult> {
        let mut state = self.whisper_ctx.create_state()?;
        let lan = user_params.language.clone().unwrap_or("zh".to_string());
        let mut params = self.build_params(user_params);
        params.set_language(Some(lan.as_str()));

        state.full(params, &audio)?;
        let num_segments = state.full_n_segments()?;

        let mut segments = Vec::new();
        let mut full_text = String::new();
        let mut current_speaker = 0;

        for i in 0..num_segments {
            let text = state.full_get_segment_text(i)?;
            let start = state.full_get_segment_t0(i)?;
            let end = state.full_get_segment_t1(i)?;
            
            if i > 0 && state.full_get_segment_speaker_turn_next(i - 1) {
                current_speaker += 1;
            }

            segments.push(TranscribeSegment {
                text: text.clone(),
                speaker_id: current_speaker,
                start: start as f64,
                end: end as f64,
            });

            full_text.push_str(&text);
        }

        Ok(TranscribeResult {
            segments,
            full_text,
        })
    }

}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use crate::audio::parse_audio_file;
    use crate::utils::logger;

    use anyhow::Result;

    #[tokio::test]
    async fn test_transcribe() -> Result<()> {
        let _guard = logger::init("./logs".to_string())?;
        // env::set_var("GGML_METAL_PATH_RESOURCES", "/Users/douxiangbin/Documents/projects/whisper.cpp/ggml/src");
        // 设置音频文件和Whisper模型的路径
        let audio_path = Path::new("./test/2.wav");
        let whisper_path = Path::new("./models/ggml-large-v3.bin");
        // let whisper_path = Path::new("./models/ggml-large-v3-turbo.bin");
    
        // 检查文件是否存在
        if !audio_path.exists() {
            panic!("audio file doesn't exist");
        }
        if !whisper_path.exists() {
            panic!("whisper file doesn't exist");
        }
    
        let enable_noise_reduction = true;  // 默认不启用降噪
        let noise_reduction_strength = 0.55;  // 降噪强度，范围可以是0.0到1.0
    
        let processed_audio = parse_audio_file(&audio_path, enable_noise_reduction, noise_reduction_strength)?;
    
        let asr = WhisperAsr::new(whisper_path.to_string_lossy().to_string())?;
        let mut params = AsrParams::new();
        params.set_language(Some("zh".to_string()));
        params.set_speaker_diarization(true);

        let result = asr.transcribe(processed_audio, params).await?;
        println!("{:?}", result);
        println!("{}", result.full_text);
    
        Ok(())
    }
}