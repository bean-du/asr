#![allow(clippy::uninlined_format_args)]

use anyhow::Result;
use tracing::info;
use std::sync::Arc;
use std::net::SocketAddr;
use asr_rs::{
    asr::whisper::WhisperAsr, auth::Auth, schedule::{TaskManager, TaskScheduler}, utils::logger, AppContext
};
use asr_rs::storage::task::sqlite::SqliteTaskStorage;
use asr_rs::auth::storage::{InMemoryApiKeyStorage, InMemoryApiKeyStatsStorage};
use asr_rs::schedule::types::TaskType;
use std::fs;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志系统
    let _guard = logger::init("./logs".to_string())?;
    // 创建必要的目录
    fs::create_dir_all("./asr_data/database")?;
    fs::create_dir_all("./asr_data/data")?;

    info!("Starting ASR service...");

    // 初始化 ASR 模型
    info!("Initializing Whisper ASR model...");
    let _asr = WhisperAsr::new("./models/ggml-large-v3.bin".to_string())?;

    // 初始化 storage
    info!("Initializing Storage...");
    let api_key_storage = InMemoryApiKeyStorage::new();
    let api_key_stats_storage = InMemoryApiKeyStatsStorage::new();
    let storage = SqliteTaskStorage::new("./asr_data/database/storage.db").await?;
    
    // 初始化认证管理器
    info!("Initializing Auth Manager...");
    let auth_manager = Auth::new(Arc::new(api_key_storage), Arc::new(api_key_stats_storage));
    
    // 初始化任务管理器
    info!("Initializing Task Manager...");
    let task_manager = TaskManager::new(Arc::new(storage));


    // 创建应用上下文
    let ctx = Arc::new(AppContext {
        auth: Arc::new(auth_manager),
        task_manager: Arc::new(task_manager),
    });

    // 初始化调度器并启动
    info!("Initializing Scheduler...");
    let scheduler = TaskScheduler::new(ctx.task_manager.clone());
    scheduler.spawn_worker(TaskType::Transcribe).await;

    tokio::spawn(async move {
        let _ =scheduler.run().await;
    });

    // 配置服务器地址
    let addr = SocketAddr::from(([127, 0, 0, 1], 7200));
    info!("Starting HTTP server at http://{}", addr);

    // 启动 HTTP 服务器
    match asr_rs::web::start_server(ctx.clone(), addr).await {
        Ok(_) => info!("Server stopped gracefully"),
        Err(e) => {
            tracing::error!("Server error: {}", e);
            return Err(e);
        }
    }

    // 优雅关闭
    info!("Shutting down...");
    // ctx.task_manager.shutdown().await?;
    
    Ok(())
}

