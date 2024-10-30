use std::sync::Arc;


pub mod types;
pub mod processors;
pub mod scheduler;
pub mod callback;
// mod tests;

// 重导出主要类型
pub use types::{
    Task, TaskType, TaskConfig, TaskParams, TaskStatus, TaskResult,
    TaskPriority, TranscribeParams, TranscribeResult, CallbackType
};

// 使用 storage 模块中的类型
pub use crate::storage::task::TaskStorage;

// 重导出处理器接口
pub use processors::TaskProcessor;
pub use processors::transcribe::TranscribeProcessor;

// 重导出调度器接口
pub use scheduler::{TaskManager, TaskScheduler};

// 提供便捷的构建方法
pub async fn create_scheduler(
    storage: impl TaskStorage,
    processors: Vec<Box<dyn TaskProcessor>>,
) -> anyhow::Result<TaskScheduler> {
    let mut task_manager = TaskManager::new(Arc::new(storage));
    
    for processor in processors {
        task_manager.register_processor(processor);
    }
    
    Ok(TaskScheduler::new(Arc::new(task_manager)))
}