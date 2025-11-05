use anyhow::Result;
use log::{info, warn, error};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{interval, Duration};

use crate::gmail_client::GmailClient;

/// Token refresh manager for Gmail OAuth2 tokens
/// 
/// Google OAuth2 access tokens expire after 1 hour.
/// This manager runs in the background and refreshes the token every 45 minutes
/// to ensure it never expires during operation.
pub struct TokenRefreshManager {
    gmail_client: Arc<Mutex<GmailClient>>,
    refresh_interval: Duration,
}

impl TokenRefreshManager {
    /// Create a new token refresh manager
    /// 
    /// # Arguments
    /// * `gmail_client` - The Gmail client to refresh tokens for
    /// * `refresh_interval_minutes` - How often to refresh (default: 45 minutes, max safe: 55 minutes)
    pub fn new(gmail_client: Arc<Mutex<GmailClient>>, refresh_interval_minutes: u64) -> Self {
        let refresh_interval = Duration::from_secs(refresh_interval_minutes * 60);
        
        info!(
            "🔐 Token refresh manager initialized (interval: {} minutes)",
            refresh_interval_minutes
        );
        
        TokenRefreshManager {
            gmail_client,
            refresh_interval,
        }
    }
    
    /// Start the background token refresh task
    /// 
    /// This spawns a tokio task that runs indefinitely, refreshing the token
    /// at the configured interval.
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run_refresh_loop().await;
        })
    }
    
    /// Run the token refresh loop
    async fn run_refresh_loop(&self) {
        let mut ticker = interval(self.refresh_interval);
        
        // Skip the first tick (happens immediately)
        ticker.tick().await;
        
        info!("🔄 Token refresh loop started");
        
        loop {
            ticker.tick().await;
            
            info!("⏰ Token refresh interval reached, refreshing token...");
            
            match self.refresh_token_safely().await {
                Ok(()) => {
                    info!("✅ Token refresh successful");
                }
                Err(e) => {
                    error!("❌ Token refresh failed: {}", e);
                    warn!("⚠️  Will retry at next interval");
                }
            }
        }
    }
    
    /// Safely refresh the token with error handling
    async fn refresh_token_safely(&self) -> Result<()> {
        let client = self.gmail_client.lock().await;
        
        info!("🔄 Refreshing Gmail OAuth2 token to keep it alive...");
        
        // Perform the refresh
        client.refresh_token().await?;
        
        info!("✅ Token refresh completed successfully");
        
        Ok(())
    }
}

/// Helper function to create and start a token refresh manager
/// 
/// # Arguments
/// * `gmail_client` - The Gmail client to manage
/// * `refresh_interval_minutes` - Optional custom interval (default: 45 minutes)
/// 
/// # Returns
/// A JoinHandle for the background task
pub fn start_token_refresh(
    gmail_client: Arc<Mutex<GmailClient>>,
    refresh_interval_minutes: Option<u64>,
) -> tokio::task::JoinHandle<()> {
    let interval = refresh_interval_minutes.unwrap_or(45);
    
    // Safety check: don't allow intervals > 55 minutes (tokens expire at 60)
    let safe_interval = if interval > 55 {
        warn!(
            "⚠️  Refresh interval {} minutes is too close to token expiry (60 min). Using 45 minutes instead.",
            interval
        );
        45
    } else {
        interval
    };
    
    let manager = TokenRefreshManager::new(gmail_client, safe_interval);
    manager.start()
}
