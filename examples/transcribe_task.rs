use asr_rs::schedule::{
    TaskScheduler, TaskManager, TaskConfig, TaskParams, TaskType,
    TranscribeParams, TaskPriority, CallbackType
};
use asr_rs::storage::task::SqliteTaskStorage;
use asr_rs::asr::Asr;
use std::sync::Arc;
use std::path::PathBuf;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 创建存储
    let storage = SqliteTaskStorage::new("tasks.db").await?;
    
    // 创建ASR实例
    let asr = Arc::new(Asr::new("./models/ggml-large-v3.bin")?);
    
    // 创建处理器
    let processor = Arc::new(TranscribeProcessor::new(asr));
    
    // 创建任务管理器
    let mut task_manager = TaskManager::new(Arc::new(storage));
    task_manager.register_processor(processor).await;
    let task_manager = Arc::new(task_manager);

    // 创建调度器
    let mut scheduler = TaskScheduler::new(task_manager.clone());
    
    // 添加转写任务的worker
    scheduler.spawn_worker(TaskType::Transcribe);

    // 创建任务
    let config = TaskConfig {
        task_type: TaskType::Transcribe,
        input_path: PathBuf::from("./input/audio.wav"),
        callback_type: CallbackType::Http {
            url: "http://localhost:8080/callback".to_string(),
        },
        params: TaskParams::Transcribe(TranscribeParams {
            language: Some("zh".to_string()),
            speaker_diarization: true,
            emotion_recognition: false,
            filter_dirty_words: false,
        }),
        priority: TaskPriority::High,
        retry_count: 0,
        max_retries: 3,
        timeout: Some(300),
    };

    let task = task_manager.create_task(config).await?;
    println!("Created task: {}", task.id);

    // 运行调度器
    scheduler.run().await?;

    Ok(())
} 