use super::*;
use crate::storage::task::SqliteTaskStorage;
use crate::schedule::processors::transcribe::TranscribeProcessor;
use crate::schedule::scheduler::{TaskScheduler, TaskManager};
use crate::schedule::types::*;
use std::sync::Arc;
use tokio::time::sleep;
use std::time::Duration;
use anyhow::Result;
use crate::asr::whisper::WhisperAsr;
use std::path::PathBuf;
use tracing::error;

// 测试辅助函数：创建测试环境
async fn setup_test_environment() -> Result<(Arc<TaskScheduler>, Arc<TaskManager>)> {
    // 创建存储
    let storage = Arc::new(SqliteTaskStorage::new("file::memory:").await?);
    
    // 创建ASR实例
    let asr = Arc::new(WhisperAsr::new("./models/ggml-large-v3.bin".to_string())?);
    
    // 创建处理器
    let processor = Box::new(TranscribeProcessor::new(asr));
    
    // 创建任务管理器
    let mut task_manager = TaskManager::new(storage);
    task_manager.register_processor(processor);
    let task_manager = Arc::new(task_manager);
    
    // 创建调度器
    let  scheduler = TaskScheduler::new(task_manager.clone());
    
    // 添加转写任务的worker
    scheduler.spawn_worker(TaskType::Transcribe);
    
    Ok((Arc::new(scheduler), task_manager))
}

// 创建测试任务配置
fn create_test_task_config(priority: TaskPriority, input_path: PathBuf) -> TaskConfig {
    TaskConfig {
        task_type: TaskType::Transcribe,
        input_path,
        callback_type: CallbackType::Http {
            url: "http://localhost:8080/callback".to_string(),
        },
        params: TaskParams::Transcribe(TranscribeParams {
            language: Some("zh".to_string()),
            speaker_diarization: true,
            emotion_recognition: false,
            filter_dirty_words: false,
        }),
        priority,
        retry_count: 0,
        max_retries: 3,
        timeout: Some(300),
    }
}

#[tokio::test]
async fn test_complete_task_lifecycle() -> Result<()> {
    // 1. 设置测试环境
    let (scheduler, task_manager) = setup_test_environment().await?;
    
    // 2. 创建测试任务
    let config = create_test_task_config(
        TaskPriority::High,
        PathBuf::from("./test_data/test.wav"),
    );
    let task = task_manager.create_task(config).await?;
    
    // 3. 验证任务创建
    assert_eq!(task.status, TaskStatus::Pending);
    
    // 4. 启动调度器
    let _scheduler_handle = tokio::spawn({
        let scheduler = scheduler.clone();
        async move {
            if let Err(e) = scheduler.run().await {
                error!("Scheduler error: {}", e);
            }
        }
    });
    
    // 5. 等待任务处理
    let mut completed = false;
    for _ in 0..10 {
        if let Some(task) = task_manager.get_task(&task.id).await? {
            if matches!(task.status, TaskStatus::Completed) {
                completed = true;
                break;
            }
        }
        sleep(Duration::from_secs(1)).await;
    }
    
    assert!(completed, "Task should be completed");
    
    // 6. 验证任务结果
    let completed_task = task_manager.get_task(&task.id).await?.unwrap();
    assert!(completed_task.result.is_some());
    
    Ok(())
}

#[tokio::test]
async fn test_priority_based_scheduling() -> Result<()> {
    let (scheduler, task_manager) = setup_test_environment().await?;
    
    // 创建不同优先级的任务
    let priorities = vec![
        TaskPriority::Low,
        TaskPriority::Critical,
        TaskPriority::Normal,
        TaskPriority::High,
    ];
    
    let mut task_ids = Vec::new();
    
    // 创建任务
    for (i, priority) in priorities.iter().enumerate() {
        let config = create_test_task_config(
            priority.clone(),
            PathBuf::from(format!("./test_data/test{}.wav", i)),
        );
        let task = task_manager.create_task(config).await?;
        task_ids.push((task.id, priority.clone()));
    }
    
    // 启动调度器
    let _scheduler_handle = tokio::spawn({
        let scheduler = scheduler.clone();
        async move {
            if let Err(e) = scheduler.run().await {
                error!("Scheduler error: {}", e);
            }
        }
    });
    
    // 验证处理顺序
    let mut processed_order = Vec::new();
    for _ in 0..task_ids.len() {
        let mut found = false;
        for _ in 0..10 {
            for (task_id, _) in &task_ids {
                if let Some(task) = task_manager.get_task(task_id).await? {
                    if matches!(task.status, TaskStatus::Completed) && !processed_order.contains(&task.id) {
                        processed_order.push(task.id.clone());
                        found = true;
                        break;
                    }
                }
            }
            if found {
                break;
            }
            sleep(Duration::from_millis(100)).await;
        }
    }
    
    // 验证优先级顺序
    let expected_order = vec![
        TaskPriority::Critical,
        TaskPriority::High,
        TaskPriority::Normal,
        TaskPriority::Low,
    ];
    
    for (i, task_id) in processed_order.iter().enumerate() {
        let task = task_manager.get_task(task_id).await?.unwrap();
        assert_eq!(task.config.priority, expected_order[i]);
    }
    
    Ok(())
}

#[tokio::test]
async fn test_error_handling_and_retry() -> Result<()> {
    let (scheduler, task_manager) = setup_test_environment().await?;
    
    // 创建一个会失败的任务（使用不存在的文件）
    let config = create_test_task_config(
        TaskPriority::Normal,
        PathBuf::from("non_existent.wav"),
    );
    let task = task_manager.create_task(config).await?;
    
    // 启动调度器
    let _scheduler_handle = tokio::spawn({
        let scheduler = scheduler.clone();
        async move {
            if let Err(e) = scheduler.run().await {
                error!("Scheduler error: {}", e);
            }
        }
    });
    
    // 等待重试和最终失败
    let mut final_status = None;
    for _ in 0..10 {
        if let Some(task) = task_manager.get_task(&task.id).await? {
            if let TaskStatus::Failed(_) = task.status {
                final_status = Some(task.status);
                break;
            }
        }
        sleep(Duration::from_secs(1)).await;
    }
    
    assert!(matches!(final_status, Some(TaskStatus::Failed(_))));
    
    Ok(())
}

#[tokio::test]
async fn test_task_timeout() -> Result<()> {
    let (scheduler, task_manager) = setup_test_environment().await?;
    
    // 创建一个短超时的任务
    let mut config = create_test_task_config(
        TaskPriority::Normal,
        PathBuf::from("./test_data/test.wav"),
    );
    config.timeout = Some(1); // 1秒超时
    
    let task = task_manager.create_task(config).await?;
    
    // 启动调度器
    let _scheduler_handle = tokio::spawn({
        let scheduler = scheduler.clone();
        async move {
            if let Err(e) = scheduler.run().await {
                error!("Scheduler error: {}", e);
            }
        }
    });
    
    // 等待任务超时
    sleep(Duration::from_secs(2)).await;
    
    let task_status = task_manager.get_task(&task.id).await?.unwrap().status;
    assert!(matches!(task_status, TaskStatus::TimedOut));
    
    Ok(())
} 