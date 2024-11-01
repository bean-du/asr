use sea_orm::entity::prelude::*;
use serde::{Serialize, Deserialize};
use chrono::{DateTime, Utc};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "tasks")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: String,
    pub status: String,  // 存储序列化后的 TaskStatus
    pub config: String,  // 存储序列化后的配置
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub result: Option<String>,  // 存储序列化后的结果
    pub error: Option<String>,
    pub priority: i32,
    pub retry_count: i32,
    pub max_retries: i32,
    pub timeout: Option<i64>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

// 
impl ActiveModelBehavior for ActiveModel {} 