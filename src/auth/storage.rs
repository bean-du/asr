use std::sync::RwLock;
use std::collections::HashMap;
use chrono::Utc;
use super::types::{ApiKeyInfo, Permission, RateLimit, KeyStatus};
use super::stats::ApiKeyStats;

pub trait ApiKeyStorage: Send + Sync + 'static {
    fn get_key_info(&self, api_key: &str) -> Result<Option<ApiKeyInfo>, String>;
    fn set_key_info(&self, api_key: String, info: ApiKeyInfo) -> Result<(), String>;
    fn remove_key(&self, api_key: &str) -> Result<(), String>;
    fn list_keys(&self) -> Result<Vec<ApiKeyInfo>, String>;
    fn update_key_status(&self, api_key: &str, status: KeyStatus) -> Result<(), String>;
}

pub trait ApiKeyStatsStorage: Send + Sync + 'static {
    fn get_stats(&self, api_key: &str) -> Result<Option<ApiKeyStats>, String>;
    fn update_stats(&self, api_key: &str, stats: ApiKeyStats) -> Result<(), String>;
}

pub struct InMemoryApiKeyStorage {
    keys: RwLock<HashMap<String, ApiKeyInfo>>,
}

impl InMemoryApiKeyStorage {
    pub fn new() -> Self {
        let mut keys = HashMap::new();
        // 添加默认的测试 key
        keys.insert(
            "test-key-123".to_string(),
            ApiKeyInfo {
                key: "test-key-123".to_string(),
                name: "Test Key".to_string(),
                created_at: Utc::now(),
                expires_at: None,
                permissions: vec![Permission::Transcribe],
                rate_limit: RateLimit {
                    requests_per_minute: 60,
                    requests_per_hour: 1000,
                    requests_per_day: 10000,
                },
                status: KeyStatus::Active,
            },
        );
        Self {
            keys: RwLock::new(keys),
        }
    }
}

impl ApiKeyStorage for InMemoryApiKeyStorage {
    fn get_key_info(&self, api_key: &str) -> Result<Option<ApiKeyInfo>, String> {
        let keys = self.keys.read().map_err(|e| e.to_string())?;
        Ok(keys.get(api_key).cloned())
    }

    fn set_key_info(&self, api_key: String, info: ApiKeyInfo) -> Result<(), String> {
        let mut keys = self.keys.write().map_err(|e| e.to_string())?;
        keys.insert(api_key, info);
        Ok(())
    }

    fn remove_key(&self, api_key: &str) -> Result<(), String> {
        let mut keys = self.keys.write().map_err(|e| e.to_string())?;
        keys.remove(api_key);
        Ok(())
    }

    fn list_keys(&self) -> Result<Vec<ApiKeyInfo>, String> {
        let keys = self.keys.read().map_err(|e| e.to_string())?;
        Ok(keys.values().cloned().collect())
    }

    fn update_key_status(&self, api_key: &str, status: KeyStatus) -> Result<(), String> {
        let mut keys = self.keys.write().map_err(|e| e.to_string())?;
        if let Some(info) = keys.get_mut(api_key) {
            info.status = status;
            Ok(())
        } else {
            Err("API key not found".to_string())
        }
    }
}

pub struct InMemoryApiKeyStatsStorage {
    stats: RwLock<HashMap<String, ApiKeyStats>>,
}

impl InMemoryApiKeyStatsStorage {
    pub fn new() -> Self {
        Self {
            stats: RwLock::new(HashMap::new()),
        }
    }
}

impl ApiKeyStatsStorage for InMemoryApiKeyStatsStorage {
    fn get_stats(&self, api_key: &str) -> Result<Option<ApiKeyStats>, String> {
        let stats = self.stats.read().map_err(|e| e.to_string())?;
        Ok(stats.get(api_key).cloned())
    }

    fn update_stats(&self, api_key: &str, stats: ApiKeyStats) -> Result<(), String> {
        let mut storage = self.stats.write().map_err(|e| e.to_string())?;
        storage.insert(api_key.to_string(), stats);
        Ok(())
    }
} 