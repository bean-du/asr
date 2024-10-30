use async_trait::async_trait;
use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::schedule::types::{Task, TaskStatus};
pub mod sqlite;

pub use sqlite::SqliteTaskStorage;

#[async_trait]
pub trait TaskStorage: Send + Sync + 'static {
    async fn save_task(&self, task: &Task) -> Result<()>;
    async fn get_task(&self, task_id: &str) -> Result<Option<Task>>;
    async fn update_task_status(&self, task_id: &str, status: TaskStatus) -> Result<()>;
    async fn delete_task(&self, task_id: &str) -> Result<()>;
    
    // 获取待处理的任务，按优先级排序
    async fn get_pending_tasks(&self, limit: usize) -> Result<Vec<Task>>;
    
    // 获取特定状态的任务
    async fn get_tasks_by_status(&self, status: TaskStatus) -> Result<Vec<Task>>;
    
    // 获取所有任务
    async fn get_all_tasks(&self) -> Result<Vec<Task>>;
    
    // 获取超时的任务
    async fn get_timed_out_tasks(&self) -> Result<Vec<Task>>;
    
    // 清理已完成的旧任务
    async fn cleanup_old_tasks(&self, before: DateTime<Utc>) -> Result<u64>;
} 