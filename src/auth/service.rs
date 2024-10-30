use std::sync::Arc;
use uuid::Uuid;
use chrono::{Duration, Utc};
use governor::{
    Quota, RateLimiter, 
    clock::DefaultClock,
    state::{InMemoryState, NotKeyed},
};
use std::num::NonZeroU32;
use std::collections::HashMap;
use tokio::sync::Mutex;

use super::error::AuthError;
use super::stats::{ApiKeyStats, ApiKeyUsageReport, UsageSummary};
use super::storage::{ApiKeyStorage, ApiKeyStatsStorage};
use super::types::{ApiKeyInfo, Permission, RateLimit, KeyStatus};
use tracing::info;

type DirectRateLimiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

pub struct Auth {
    key_storage: Arc<dyn ApiKeyStorage>,
    stats_storage: Arc<dyn ApiKeyStatsStorage>,
    rate_limiters: Arc<Mutex<HashMap<String, Arc<DirectRateLimiter>>>>,
}

impl Auth {
    pub fn new(
        key_storage: Arc<dyn ApiKeyStorage>,
        stats_storage: Arc<dyn ApiKeyStatsStorage>,
    ) -> Self {
        Self {
            key_storage,
            stats_storage,
            rate_limiters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn new_with_memory_storage() -> Self {
        use super::storage::{InMemoryApiKeyStorage, InMemoryApiKeyStatsStorage};
        Self {
            key_storage: Arc::new(InMemoryApiKeyStorage::new()),
            stats_storage: Arc::new(InMemoryApiKeyStatsStorage::new()),
            rate_limiters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn verify_api_key(&self, api_key: Option<&str>, required_permission: Permission) -> Result<(), AuthError> {
        info!("Verifying API key: {:?}", api_key);
        let api_key = api_key.ok_or(AuthError::MissingApiKey)?;
        let api_key = match api_key.split(" ").last() {
            Some(key) => key,
            None => return Err(AuthError::InvalidApiKey),
        };

        let key_info = self.key_storage
            .get_key_info(api_key)?
            .ok_or(AuthError::InvalidApiKey)?;

        // check key status
        match key_info.status {
            KeyStatus::Suspended => return Err(AuthError::KeySuspended),
            KeyStatus::Expired => return Err(AuthError::KeyExpired),
            KeyStatus::Active => {}
        }

        // check expiration time
        if let Some(expires_at) = key_info.expires_at {
            if expires_at < Utc::now() {
                return Err(AuthError::KeyExpired);
            }
        }

        // check permissions
        if !key_info.permissions.contains(&required_permission) {
            return Err(AuthError::InsufficientPermissions);
        }

        // check rate limit
        let mut limiters = self.rate_limiters.lock().await;
        let limiter = limiters.entry(api_key.to_string())
            .or_insert_with(|| {
                Arc::new(RateLimiter::direct(
                    Quota::per_minute(NonZeroU32::new(key_info.rate_limit.requests_per_minute).unwrap())
                ))
            });

        if let Err(_) = limiter.check() {
            return Err(AuthError::RateLimitExceeded);
        }

        // update stats
        self.update_key_stats(api_key).await?;

        Ok(())
    }

    pub fn create_api_key(
        &self,
        name: String,
        permissions: Vec<Permission>,
        rate_limit: RateLimit,
        expires_in_days: Option<i64>,
    ) -> Result<ApiKeyInfo, String> {
        let key = format!("key-{}", Uuid::new_v4());
        let expires_at = expires_in_days.map(|days| Utc::now() + Duration::days(days));

        let key_info = ApiKeyInfo {
            key: key.clone(),
            name,
            created_at: Utc::now(),
            expires_at,
            permissions,
            rate_limit,
            status: KeyStatus::Active,
        };

        self.key_storage.set_key_info(key, key_info.clone())?;
        Ok(key_info)
    }

    pub fn revoke_api_key(&self, api_key: &str) -> Result<(), String> {
        self.key_storage.update_key_status(api_key, KeyStatus::Suspended)
    }

    async fn update_key_stats(&self, api_key: &str) -> Result<(), String> {
        let mut stats = self.stats_storage
            .get_stats(api_key)?
            .unwrap_or_else(ApiKeyStats::new);
        
        stats.update();
        self.stats_storage.update_stats(api_key, stats)
    }

    pub fn get_key_stats(&self, api_key: &str) -> Result<ApiKeyStats, String> {
        // check if api key exists
        if self.key_storage.get_key_info(api_key)?.is_none() {
            return Err("API key not found".to_string());
        }

        // get stats
        Ok(self.stats_storage
            .get_stats(api_key)?
            .unwrap_or_else(ApiKeyStats::new))
    }

    pub fn get_key_usage_report(&self, api_key: &str) -> Result<ApiKeyUsageReport, String> {
        let stats = self.get_key_stats(api_key)?;
        let key_info = self.key_storage
            .get_key_info(api_key)?
            .ok_or_else(|| "API key not found".to_string())?;

        Ok(ApiKeyUsageReport {
            key_info: key_info.clone(),
            stats: stats.clone(),
            usage_summary: UsageSummary {
                average_daily_requests: stats.total_requests as f64 / 30.0,
                peak_daily_requests: stats.requests_per_day.values().max().copied().unwrap_or(0),
                days_until_expiry: key_info.expires_at
                    .map(|exp| (exp - Utc::now()).num_days())
                    .unwrap_or(-1),
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;
    use std::time::Duration;

    async fn setup_test_auth() -> Auth {
        Auth::new_with_memory_storage()
    }

    #[tokio::test]
    async fn test_api_key_basic_lifecycle() {
        let auth = setup_test_auth().await;

        // 1. create api key
        let key_info = auth.create_api_key(
            "Test Key".to_string(),
            vec![Permission::Transcribe],
            RateLimit {
                requests_per_minute: 60,
                requests_per_hour: 1000,
                requests_per_day: 10000,
            },
            Some(30),
        ).unwrap();

        // 2. validate basic info
        assert_eq!(key_info.name, "Test Key");
        assert!(key_info.key.starts_with("key-"));
        assert_eq!(key_info.permissions, vec![Permission::Transcribe]);
        assert_eq!(key_info.status, KeyStatus::Active);

        // 3. validate api key
        assert!(auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await.is_ok());

        // 4. revoke api key
        auth.revoke_api_key(&key_info.key).unwrap();
        assert!(auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await.is_err());
    }

    #[tokio::test]
    async fn test_api_key_permissions() {
        let auth = setup_test_auth().await;

        // create api key with multiple permissions
        let key_info = auth.create_api_key(
            "Multi-Permission Key".to_string(),
            vec![Permission::Transcribe, Permission::SpeakerDiarization],
            RateLimit {
                requests_per_minute: 60,
                requests_per_hour: 1000,
                requests_per_day: 10000,
            },
            None,
        ).unwrap();

        // test allowed permissions
        assert!(auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await.is_ok());
        assert!(auth.verify_api_key(Some(&key_info.key), Permission::SpeakerDiarization).await.is_ok());

        // test unauthorized permissions
        assert!(matches!(
            auth.verify_api_key(Some(&key_info.key), Permission::Admin).await,
            Err(AuthError::InsufficientPermissions)
        ));
    }

    #[tokio::test]
    async fn test_api_key_expiration() {
        let auth = setup_test_auth().await;

        // create a key that will expire in 1 second
        let key_info = auth.create_api_key(
            "Expiring Key".to_string(),
            vec![Permission::Transcribe],
            RateLimit {
                requests_per_minute: 60,
                requests_per_hour: 1000,
                requests_per_day: 10000,
            },
            Some(0), // 0 days expiration, expires immediately
        ).unwrap();

        // validate key has expired
        sleep(Duration::from_secs(1)).await;
        assert!(matches!(
            auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await,
            Err(AuthError::KeyExpired)
        ));

        // create a key with a long expiration time
        let key_info = auth.create_api_key(
            "Valid Key".to_string(),
            vec![Permission::Transcribe],
            RateLimit {
                requests_per_minute: 60,
                requests_per_hour: 1000,
                requests_per_day: 10000,
            },
            Some(30), // 30 days expiration
        ).unwrap();

        // validate key is available
        assert!(auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await.is_ok());
    }

    #[tokio::test]
    async fn test_rate_limiting() {
        let auth = setup_test_auth().await;

        // create a key with a low rate limit
        let key_info = auth.create_api_key(
            "Rate Limited Key".to_string(),
            vec![Permission::Transcribe],
            RateLimit {
                requests_per_minute: 2, // only allow 2 requests per minute
                requests_per_hour: 1000,
                requests_per_day: 10000,
            },
            None,
        ).unwrap();

        // first request should succeed
        assert!(auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await.is_ok());
        
        // add a short delay
        sleep(Duration::from_millis(100)).await;
        
        // second request should succeed
        assert!(auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await.is_ok());
        
        // add a short delay
        sleep(Duration::from_millis(100)).await;
        
        // third request should fail (exceed rate limit)
        let result = auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await;
        assert!(matches!(result, Err(AuthError::RateLimitExceeded)));

        // wait for rate limit to reset (wait 65 seconds to ensure reset)
        sleep(Duration::from_secs(65)).await;

        // request after reset should succeed
        assert!(auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await.is_ok());
    }

    #[tokio::test]
    async fn test_stats_and_usage_report() {
        let auth = setup_test_auth().await;

        // create api key
        let key_info = auth.create_api_key(
            "Stats Test Key".to_string(),
            vec![Permission::Transcribe],
            RateLimit {
                requests_per_minute: 60,
                requests_per_hour: 1000,
                requests_per_day: 10000,
            },
            Some(30),
        ).unwrap();

        // simulate some requests
        for _ in 0..5 {
            auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await.unwrap();
        }

        // validate stats
        let stats = auth.get_key_stats(&key_info.key).unwrap();
        assert_eq!(stats.total_requests, 5);
        assert_eq!(stats.requests_today, 5);

        // validate usage report
        let report = auth.get_key_usage_report(&key_info.key).unwrap();
        assert_eq!(report.stats.total_requests, 5);
        assert!(report.usage_summary.average_daily_requests > 0.0);
        assert_eq!(report.usage_summary.peak_daily_requests, 5);
        assert!(report.usage_summary.days_until_expiry > 0);
    }

    #[tokio::test]
    async fn test_invalid_api_keys() {
        let auth = setup_test_auth().await;

        // test empty api key
        assert!(matches!(
            auth.verify_api_key(None, Permission::Transcribe).await,
            Err(AuthError::MissingApiKey)
        ));

        // test invalid api key
        assert!(matches!(
            auth.verify_api_key(Some("invalid-key"), Permission::Transcribe).await,
            Err(AuthError::InvalidApiKey)
        ));

        // test revoked api key
        let key_info = auth.create_api_key(
            "Revoked Key".to_string(),
            vec![Permission::Transcribe],
            RateLimit {
                requests_per_minute: 60,
                requests_per_hour: 1000,
                requests_per_day: 10000,
            },
            None,
        ).unwrap();

        auth.revoke_api_key(&key_info.key).unwrap();
        assert!(matches!(
            auth.verify_api_key(Some(&key_info.key), Permission::Transcribe).await,
            Err(AuthError::KeySuspended)
        ));
    }
} 