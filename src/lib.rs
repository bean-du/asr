pub mod asr;
pub mod auth;
pub mod schedule;
pub mod utils;
pub mod web;
pub mod storage;
pub mod audio;

use std::sync::Arc;
use auth::Auth;
use schedule::TaskManager;

pub struct AppContext {
    pub auth: Arc<Auth>,
    pub task_manager: Arc<TaskManager>,
}
