use async_trait::async_trait;
use anyhow::Result;
use chrono::{DateTime, Utc};
use sqlx::{SqlitePool, Row};
use tracing::info;

use super::TaskStorage;
use crate::schedule::types::{Task, TaskStatus};

pub struct SqliteTaskStorage {
    pool: SqlitePool,
}

impl SqliteTaskStorage {
    pub async fn new(database_url: &str) -> Result<Self> {
        info!("Initializing SQLite task storage at {}", database_url);
        let pool = sqlx::SqlitePool::connect(database_url).await?;
        
        // 创建任务表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                status TEXT NOT NULL,
                config TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                started_at TEXT,
                completed_at TEXT,
                result TEXT,
                error TEXT,
                priority INTEGER NOT NULL,
                retry_count INTEGER NOT NULL DEFAULT 0
            )
            "#,
        )
        .execute(&pool)
        .await?;

        Ok(Self { pool })
    }

    fn row_to_task(&self, row: sqlx::sqlite::SqliteRow) -> Result<Task> {
        let config: String = row.get("config");
        let config = serde_json::from_str(&config)?;
        
        let result: Option<String> = row.get("result");
        let result = result.map(|r| serde_json::from_str(&r)).transpose()?;

        Ok(Task {
            id: row.get("id"),
            status: serde_json::from_str(row.get("status"))?,
            config,
            created_at: DateTime::parse_from_rfc3339(row.get("created_at"))?.with_timezone(&Utc),
            updated_at: DateTime::parse_from_rfc3339(row.get("updated_at"))?.with_timezone(&Utc),
            started_at: row.get::<Option<String>, _>("started_at")
                .map(|t| DateTime::parse_from_rfc3339(&t))
                .transpose()?
                .map(|t| t.with_timezone(&Utc)),
            completed_at: row.get::<Option<String>, _>("completed_at")
                .map(|t| DateTime::parse_from_rfc3339(&t))
                .transpose()?
                .map(|t| t.with_timezone(&Utc)),
            result,
            error: row.get("error"),
        })
    }
}

#[async_trait]
impl TaskStorage for SqliteTaskStorage {
    async fn save_task(&self, task: &Task) -> Result<()> {
        let config = serde_json::to_string(&task.config)?;
        let result = task.result.as_ref().map(serde_json::to_string).transpose()?;
        let priority = task.config.priority.clone();
        
        sqlx::query(
            r#"
            INSERT INTO tasks 
            (id, status, config, created_at, updated_at, started_at, completed_at, result, error, priority, retry_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&task.id)
        .bind(serde_json::to_string(&task.status)?)
        .bind(&config)
        .bind(task.created_at.to_rfc3339())
        .bind(task.updated_at.to_rfc3339())
        .bind(task.started_at.map(|t| t.to_rfc3339()))
        .bind(task.completed_at.map(|t| t.to_rfc3339()))
        .bind(result)
        .bind(&task.error)
        .bind(priority as i32)
        .bind(task.config.retry_count)
        .execute(&self.pool)
        .await?;

        Ok(())
    }
    
    async fn get_all_tasks(&self) -> Result<Vec<Task>> {
        let rows = sqlx::query("SELECT * FROM tasks")
            .fetch_all(&self.pool)
            .await?;
            
        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(self.row_to_task(row)?);
        }
        Ok(tasks)
    }

    async fn get_pending_tasks(&self, limit: usize) -> Result<Vec<Task>> {
        let rows = sqlx::query(
            r#"
            SELECT * FROM tasks 
            WHERE status = 'Pending'
            ORDER BY 
                CASE priority 
                    WHEN 'Critical' THEN 1
                    WHEN 'High' THEN 2
                    WHEN 'Normal' THEN 3
                    WHEN 'Low' THEN 4
                END,
                created_at ASC
            LIMIT ?
            "#,
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(self.row_to_task(row)?);
        }

        Ok(tasks)
    }

    async fn get_task(&self, task_id: &str) -> Result<Option<Task>> {
        let row = sqlx::query("SELECT * FROM tasks WHERE id = ?")
            .bind(task_id)
            .fetch_optional(&self.pool)
            .await?;
            
        Ok(match row {
            Some(row) => Some(self.row_to_task(row)?),
            None => None,
        })
    }

    async fn update_task_status(&self, task_id: &str, status: TaskStatus) -> Result<()> {
        let status_str = serde_json::to_string(&status)?;
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            UPDATE tasks 
            SET status = ?, 
                updated_at = ?,
                started_at = CASE 
                    WHEN ? = 'Processing' AND started_at IS NULL 
                    THEN ? 
                    ELSE started_at 
                END,
                completed_at = CASE 
                    WHEN ? = 'Completed' 
                    THEN ? 
                    ELSE completed_at 
                END
            WHERE id = ?
            "#
        )
        .bind(&status_str)
        .bind(&now)
        .bind(&status_str)
        .bind(&now)
        .bind(&status_str)
        .bind(&now)
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn delete_task(&self, task_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM tasks WHERE id = ?")
            .bind(task_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn get_timed_out_tasks(&self) -> Result<Vec<Task>> {
        let processing_status = serde_json::to_string(&TaskStatus::Processing)?;
        
        let rows = sqlx::query(
            r#"
            SELECT * FROM tasks 
            WHERE status = ? 
            AND started_at IS NOT NULL 
            AND (strftime('%s', 'now') - strftime('%s', started_at)) > timeout
            "#
        )
        .bind(processing_status)
        .fetch_all(&self.pool)
        .await?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(self.row_to_task(row)?);
        }

        Ok(tasks)
    }

    async fn cleanup_old_tasks(&self, before: DateTime<Utc>) -> Result<u64> {
        let result = sqlx::query(
            r#"
            DELETE FROM tasks 
            WHERE (status = 'Completed' OR status LIKE 'Failed%')
            AND updated_at < ?
            "#
        )
        .bind(before.to_rfc3339())
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }

    async fn get_tasks_by_status(&self, status: TaskStatus) -> Result<Vec<Task>> {
        let status_str = serde_json::to_string(&status)?;
        
        let rows = sqlx::query(
            "SELECT * FROM tasks WHERE status = ? ORDER BY priority DESC, created_at ASC"
        )
        .bind(status_str)
        .fetch_all(&self.pool)
        .await?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(self.row_to_task(row)?);
        }

        Ok(tasks)
    }
} 