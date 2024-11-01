use rubato::{SincFixedIn, SincInterpolationParameters, WindowFunction, Resampler};
use hound::{SampleFormat, WavReader};
use std::path::Path;
use std::process::Command;
use rayon::prelude::*;
use std::fs;
use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::Arc;
use anyhow::Result;
use tracing::{info, error};

pub enum AudioFormat {
    Wav,
    Aac,
    Amr,
    M4a,
    Ogg,
    Opus,
    Wma,
    Mp3,
    Flac,
}

/// 解析音频文件并进行预处理
/// 
/// 该函数读取音频文件，将其转换为WAV格式（如果需要），然后将其转换为单声道、归一化，并进行一系列预处理步骤
/// 
/// # 参数
/// * `path` - 音频文件的路径
/// 
/// # 返回值
/// * `Vec<f32>` - 处理后的音频样本（单声道，16kHz采样率）
/// 
/// # 处理步骤
/// 1. 确保文件为WAV格式
/// 2. 读取WAV文件
/// 3. 转换为单声道
/// 4. 归一化音频
/// 5. 进行语音活动检测
/// 6. 应用预加重
/// 7. 应用噪声门限
/// 8. 如果需要，重采样到16kHz
pub fn parse_audio_file(path: &Path, enable_noise_reduction: bool, noise_reduction_strength: f32) -> Result<Vec<f32>> {
    let wav_path = ensure_wav_format(path)?;
    let (samples, num_channels, sample_rate) = read_wav_file(&wav_path)?;
    
    // 如果转换了文件，删除临时的WAV文件
    if wav_path != path {
        if let Err(e) = fs::remove_file(&wav_path) {
            error!("Failed to remove temporary WAV file: {}", e);
            // 继续执行，不要因为清理临时文件失败而中断整个处理流程
        } else {
            info!("Removed temporary WAV file: {:?}", wav_path);
        }
    }

    let mono_samples = convert_to_mono(&samples, num_channels);
    let normalized_samples = normalize_audio(&mono_samples);
    let processed_samples = if enable_noise_reduction {
        spectral_noise_reduction(&normalized_samples, 2048, 0.75, noise_reduction_strength)
    } else {
        normalized_samples
    };
    let vad_samples = voice_activity_detection(&processed_samples, 1024, 0.005);
    let emphasized_samples = apply_pre_emphasis(&vad_samples, 0.97);
    let gated_samples = apply_noise_gate(&emphasized_samples, 0.01);
    
    if sample_rate != 16000 {
        Ok(resample_audio(&gated_samples, sample_rate))
    } else {
        info!("Sample rate is already 16000 Hz, no resampling needed.");
        Ok(gated_samples)
    }
}

/// 确保音频文件为WAV格式
/// 
/// 如果输入文件不是WAV格式，使用FFmpeg将其转换为WAV格式
/// 
/// # 参数
/// * `path` - 输入音频文件的路径
/// 
/// # 返回值
/// * `std::path::PathBuf` - WAV格式文件的路径（可能是原文件路径或新创建的WAV文件路径）
/// 
/// # 注意
/// 此函数依赖于系统中安装的FFmpeg
fn ensure_wav_format(path: &Path) -> Result<std::path::PathBuf> {
    if let Some(extension) = path.extension() {
        if extension.to_str().unwrap_or("").to_lowercase() == "wav" {
            return Ok(path.to_path_buf());
        }
    }

    let output_path = path.with_extension("wav");
    info!("Converting audio file to WAV format...");
    
    let status = Command::new("ffmpeg")
        .arg("-i")
        .arg(path)
        .arg("-acodec")
        .arg("pcm_s16le")
        .arg("-ar")
        .arg("44100")
        .arg(&output_path)
        .status()
        .map_err(|e| anyhow::anyhow!("Failed to execute ffmpeg: {}", e))?;

    if !status.success() {
        return Err(anyhow::anyhow!("FFmpeg conversion failed with status: {}", status));
    }

    Ok(output_path)
}

/// 读取WAV文件
/// 
/// 读取WAV文件并返回其样本数据、通道数和采样率
/// 
/// # 参数
/// * `path` - WAV文件的路径
/// 
/// # 返回值
/// * `(Vec<f32>, usize, u32)` - 包含样本数据、通道数和采样率的元组
/// 
/// # Panics
/// 如果文件格式不符合预期（非整数样本格式或非16位样本），函数会panic
fn read_wav_file(path: &Path) -> Result<(Vec<f32>, usize, u32)> {
    let mut reader = WavReader::open(path)
        .map_err(|e| anyhow::anyhow!("Failed to read WAV file: {}", e))?;
    
    let num_channels = reader.spec().channels as usize;
    let sample_rate = reader.spec().sample_rate;

    if reader.spec().sample_format != SampleFormat::Int {
        return Err(anyhow::anyhow!("Unsupported sample format: expected integer format"));
    }

    if reader.spec().bits_per_sample != 16 {
        return Err(anyhow::anyhow!("Unsupported bits per sample: expected 16 bits"));
    }

    info!("Original sample rate: {} Hz", sample_rate);

    let samples: Vec<f32> = reader
        .samples::<i16>()
        .map(|s| s.map(|val| val as f32))
        .collect::<std::result::Result<Vec<f32>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to read samples: {}", e))?;

    Ok((samples, num_channels, sample_rate))
}

/// 将多声道音频转换为单声道
/// 
/// 通过对每个采样的所有通道取平均值，将多声道音频转换为单声道
/// 
/// # 参数
/// * `samples` - 输入的音频样本
/// * `num_channels` - 输入音频的通道数
/// 
/// # 返回值
/// * `Vec<f32>` - 转换后的单声道音频样本
fn convert_to_mono(samples: &[f32], num_channels: usize) -> Vec<f32> {
    samples.par_chunks(num_channels)
        .map(|chunk| {
            chunk.iter().sum::<f32>() / num_channels as f32
        })
        .collect()
}

/// 归一化音频
/// 
/// 将音频样本归一化到[-1, 1]范围内
/// 
/// # 参数
/// * `samples` - 输入的音频样本
/// 
/// # 返回值
/// * `Vec<f32>` - 归一化后的音频样本
fn normalize_audio(samples: &[f32]) -> Vec<f32> {
    let max_abs = samples.par_iter().map(|&s| s.abs()).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap_or(1.0);
    samples.par_iter().map(|&s| s / max_abs).collect()
}

/// 应用预加重
/// 
/// 对音频样本应用预加滤波器，以增强高频成分
/// 
/// # 参数
/// * `samples` - 输入的音频样本
/// * `pre_emphasis` - 预加重系数（通常在0.95到0.97之间）
/// 
/// # 返回值
/// * `Vec<f32>` - 应用预加重后的音频样本
fn apply_pre_emphasis(samples: &[f32], pre_emphasis: f32) -> Vec<f32> {
    let mut emphasized_samples = vec![0.0; samples.len()];
    emphasized_samples[0] = samples[0];
    emphasized_samples.par_iter_mut().enumerate().skip(1).for_each(|(i, sample)| {
        *sample = samples[i] - pre_emphasis * samples[i-1];
    });
    emphasized_samples
}

/// 应用噪声门限
/// 
/// 将低于指定阈值的样设置为零，以减少背景噪声
/// 
/// # 参数
/// * `samples` - 输入的音频样本
/// * `noise_gate` - 噪声门限阈值
/// 
/// # 返回值
/// * `Vec<f32>` - 应用噪声门限后的音频样本
fn apply_noise_gate(samples: &[f32], noise_gate: f32) -> Vec<f32> {
    samples.par_iter()
        .map(|&s| if s.abs() < noise_gate { 0.0 } else { s })
        .collect()
}

/// 重采样音频
/// 
/// 将音频重采样到16kHz采样率
/// 
/// # 参数
/// * `samples` - 输入的音频样本
/// * `original_sample_rate` - 原始采样率
/// 
/// # 返回值
/// * `Vec<f32>` - 重采样后的音频样本（16kHz）
fn resample_audio(samples: &[f32], original_sample_rate: u32) -> Vec<f32> {
    println!("Resampling from {} Hz to 16000 Hz", original_sample_rate);

    let params = SincInterpolationParameters {
        sinc_len: 512,
        f_cutoff: 0.98,
        interpolation: rubato::SincInterpolationType::Cubic,
        oversampling_factor: 512,
        window: WindowFunction::BlackmanHarris2,
    };

    let mut resampler = SincFixedIn::<f32>::new(
        16000.0 / original_sample_rate as f64,
        2.0,
        params,
        samples.len(),
        1,
    )
    .expect("Failed to create resampler");

    let resampled = resampler
        .process(&[samples.to_vec()], None)
        .expect("Resampling failed");

    resampled[0].clone()
}

/// 语音活动检测
/// 
/// 检测音频中的语音活动，将能量低于阈值的部分设置为静音
/// 
/// # 参数
/// * `samples` - 输入的音频样本
/// * `frame_size` - 每个分析帧的大小
/// * `threshold` - 能量阈值
/// 
/// # 返回值
/// * `Vec<f32>` - 处理后的音频样本，静音部分被设置为0
pub fn voice_activity_detection(samples: &[f32], frame_size: usize, threshold: f32) -> Vec<f32> {
    samples.par_chunks(frame_size)
        .flat_map(|chunk| {
            let energy = chunk.par_iter().map(|&s| s * s).sum::<f32>() / frame_size as f32;
            if energy > threshold {
                chunk.to_vec()
            } else {
                vec![0.0; chunk.len()]
            }
        })
        .collect()
}

/// 使用维纳滤波进行降噪
///
/// # 参数
/// * `samples` - 输入的音频样本
/// * `frame_size` - FFT帧大小（建议使用2的幂，如1024或2048）
/// * `overlap` - 帧重叠率（通常为0.5或0.75）
///
/// # 返回值
/// * `Vec<f32>` - 降噪后的音频样本
pub fn spectral_noise_reduction(samples: &[f32], frame_size: usize, overlap: f32, strength: f32) -> Vec<f32> {
    let step_size = (frame_size as f32 * (1.0 - overlap)) as usize;
    let mut planner = FftPlanner::new();
    let fft = planner.plan_fft_forward(frame_size);
    let ifft = planner.plan_fft_inverse(frame_size);

    let frames = samples.windows(frame_size).step_by(step_size).collect::<Vec<_>>();
    let noise_power = estimate_noise_power(&frames, &fft);

    let processed_frames: Vec<Vec<Complex<f32>>> = frames.par_iter().map(|frame| {
        let mut fft_input: Vec<Complex<f32>> = frame.iter()
            .enumerate()
            .map(|(i, &s)| Complex::new(s * hann_window(i, frame_size), 0.0))
            .collect();

        fft.process(&mut fft_input);

        for (i, complex) in fft_input.iter_mut().enumerate() {
            let power = complex.norm_sqr();
            let noise = noise_power[i];
            let snr = power / (noise + 1e-10);
            let gain = 1.0 - (strength / (snr + 1.0)).min(1.0);  // 更温和的降噪
            *complex *= gain.sqrt();
        }

        ifft.process(&mut fft_input);
        fft_input
    }).collect();

    let mut output = vec![0.0; samples.len()];
    for (i, frame) in processed_frames.iter().enumerate() {
        let start = i * step_size;
        for (j, &complex) in frame.iter().enumerate() {
            if start + j < output.len() {
                output[start + j] += complex.re / (frame_size as f32);
            }
        }
    }

    // 应用平滑处理
    output = smooth_signal(&output, 5);

    remove_dc_offset(&mut output);

    // 全局增益控制
    let max_abs = output.iter().map(|&x| x.abs()).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap();
    let gain_factor = samples.iter().map(|&x| x.abs()).max_by(|a, b| a.partial_cmp(b).unwrap()).unwrap() / max_abs;
    output.iter_mut().for_each(|x| *x *= gain_factor);

    // 应用后处理均衡化
    apply_equalization(&mut output);

    output
}

fn estimate_noise_power(frames: &[&[f32]], fft: &Arc<dyn rustfft::Fft<f32>>) -> Vec<f32> {
    let frame_size = fft.len();
    let mut noise_power = vec![0.0; frame_size];
    let num_frames = frames.len().min(20);  // 使用前20帧或所有帧（如果少于20帧）

    for frame in frames.iter().take(num_frames) {
        let mut fft_input: Vec<Complex<f32>> = frame.iter()
            .enumerate()
            .map(|(i, &s)| Complex::new(s * hann_window(i, frame_size), 0.0))
            .collect();
        fft.process(&mut fft_input);
        
        for (i, complex) in fft_input.iter().enumerate() {
            noise_power[i] += complex.norm_sqr() / num_frames as f32;
        }
    }

    // 应用平滑处理到噪声功率谱
    smooth_signal(&noise_power, 7)  // 增加平滑窗口大小
}

fn hann_window(i: usize, size: usize) -> f32 {
    0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / (size - 1) as f32).cos())
}

fn smooth_signal(signal: &[f32], window_size: usize) -> Vec<f32> {
    let half_window = window_size / 2;
    signal.iter().enumerate().map(|(i, _)| {
        let start = i.saturating_sub(half_window);
        let end = (i + half_window + 1).min(signal.len());
        let sum: f32 = signal[start..end].iter().sum();
        sum / (end - start) as f32
    }).collect()
}

fn remove_dc_offset(samples: &mut [f32]) {
    let mean: f32 = samples.iter().sum::<f32>() / samples.len() as f32;
    samples.iter_mut().for_each(|s| *s -= mean);
}

// fn divide_into_subbands(spectrum: &[Complex<f32>], num_bands: usize) -> Vec<Vec<Complex<f32>>> {
//     let band_size = spectrum.len() / num_bands;
//     (0..num_bands)
//         .map(|i| spectrum[i * band_size..(i + 1) * band_size].to_vec())
//         .collect()
// }

// fn denoise_subband(subband: &[Complex<f32>], noise_power: &[f32]) -> Vec<Complex<f32>> {
//     subband.iter().zip(noise_power.iter()).map(|(&complex, &noise)| {
//         let power = complex.norm_sqr();
//         let snr = power / (noise + 1e-10);
//         let gain = (snr - 1.0).max(0.0) / snr;
//         complex * gain.sqrt()
//     }).collect()
// }

// fn merge_subbands(subbands: Vec<Vec<Complex<f32>>>) -> Vec<Complex<f32>> {
//     subbands.into_iter().flatten().collect()
// }

fn apply_equalization(samples: &mut [f32]) {
    let eq_curve = generate_eq_curve(samples.len());
    samples.iter_mut().zip(eq_curve.iter()).for_each(|(sample, &gain)| {
        *sample *= gain;
    });
}

fn generate_eq_curve(length: usize) -> Vec<f32> {
    // 这里我们生成一个简单的均衡曲线，稍微提升中频
    (0..length).map(|i| {
        let x = i as f32 / length as f32;
        1.0 + 0.2 * (std::f32::consts::PI * x).sin()
    }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{WavSpec, WavWriter};
    use std::fs;

    #[test]
    fn test_spectral_noise_reduction() -> Result<()> {
        let input_path = Path::new("./test/1.wav");
        let (samples, num_channels, sample_rate) = read_wav_file(input_path)?;

        println!("Original signal stats: min={}, max={}, mean={}", 
                 samples.iter().fold(f32::INFINITY, |a, &b| a.min(b)),
                 samples.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b)),
                 samples.iter().sum::<f32>() / samples.len() as f32);

        let denoised = spectral_noise_reduction(&samples, 2048, 0.55,0.55);

        let input_file_name = input_path.file_name().unwrap().to_str().unwrap();
        let output_file_name = format!("{}_denoised.wav", input_file_name.trim_end_matches(".wav"));
        let output_path = Path::new("./test").join(output_file_name);

        fs::create_dir_all("./test").unwrap();

        let spec = WavSpec {
            channels: num_channels as u16,
            sample_rate: sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };

        let mut writer = WavWriter::create(&output_path, spec).unwrap();

        let max_abs = denoised.iter().fold(0.0f32, |a, &b| a.max(b.abs()));
        for &sample in &denoised {
            let scaled_sample = (sample / max_abs * 32767.0) as i16;
            writer.write_sample(scaled_sample).unwrap();
        }

        writer.finalize().unwrap();

        assert!(output_path.exists());
        
        println!("Denoised audio saved to: {:?}", output_path);

        let min = denoised.iter().fold(f32::INFINITY, |a, &b| a.min(b));
        let max = denoised.iter().fold(f32::NEG_INFINITY, |a, &b| a.max(b));
        let mean = denoised.iter().sum::<f32>() / denoised.len() as f32;
        println!("Denoised stats - Min: {}, Max: {}, Mean: {}", min, max, mean);

        Ok(())
    }
}
