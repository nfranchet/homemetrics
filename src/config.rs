use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub gmail: GmailConfig,
    pub database: DatabaseConfig,
    pub data_dir: String,
    pub scheduler: SchedulerConfig,
    pub slack: Option<SlackConfig>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub schedule_times: Vec<String>, // Format: "HH:MM" (e.g., ["02:00", "14:00"])
}

#[derive(Debug, Deserialize, Clone)]
pub struct GmailConfig {
    pub credentials_path: String,
    pub token_cache_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SlackConfig {
    pub bot_token: String,
    pub channel_id: String,
}

impl Config {
    pub fn new() -> Result<Self> {
        // Check that essential variables are defined
        Self::check_required_env_vars()?;
        
        // Configuration loaded from environment variables
        Ok(Config {
            gmail: GmailConfig {
                credentials_path: std::env::var("GMAIL_CREDENTIALS_PATH")
                    .expect("GMAIL_CREDENTIALS_PATH must be defined"),
                token_cache_path: std::env::var("GMAIL_TOKEN_CACHE_PATH")
                    .unwrap_or_else(|_| "./gmail-token-cache.json".to_string()),
            },
            database: DatabaseConfig {
                host: std::env::var("DB_HOST")
                    .unwrap_or_else(|_| "localhost".to_string()),
                port: std::env::var("DB_PORT")
                    .unwrap_or_else(|_| "5432".to_string())
                    .parse()
                    .unwrap_or(5432),
                database: std::env::var("DB_NAME")
                    .unwrap_or_else(|_| "homemetrics".to_string()),
                username: std::env::var("DB_USERNAME")
                    .unwrap_or_else(|_| "postgres".to_string()),
                password: std::env::var("DB_PASSWORD")
                    .expect("DB_PASSWORD must be defined"),
            },
            data_dir: std::env::var("DATA_DIR")
                .unwrap_or_else(|_| "./data".to_string()),
            scheduler: SchedulerConfig {
                enabled: std::env::var("SCHEDULER_ENABLED")
                    .unwrap_or_else(|_| "false".to_string())
                    .parse()
                    .unwrap_or(false),
                schedule_times: std::env::var("SCHEDULER_TIMES")
                    .unwrap_or_else(|_| "02:00".to_string())
                    .split(',')
                    .map(|s| s.trim().to_string())
                    .collect(),
            },
            slack: match (std::env::var("SLACK_BOT_TOKEN"), std::env::var("SLACK_CHANNEL_ID")) {
                (Ok(bot_token), Ok(channel_id)) => Some(SlackConfig {
                    bot_token,
                    channel_id,
                }),
                _ => {
                    log::warn!("SLACK_BOT_TOKEN or SLACK_CHANNEL_ID not defined - Slack notifications disabled");
                    None
                }
            },
        })
    }
    
    fn check_required_env_vars() -> Result<()> {
        let required_vars = [
            "GMAIL_CREDENTIALS_PATH",
        ];
        
        let mut missing_vars = Vec::new();
        
        for var in &required_vars {
            if std::env::var(var).is_err() {
                missing_vars.push(*var);
            }
        }
        
        if !missing_vars.is_empty() {
            anyhow::bail!(
                "Missing environment variables: {}\n\
                 \n\
                 💡 Solutions:\n\
                 1. Create a .env file with your credentials:\n\
                    cp .env.example .env\n\
                    # Then edit .env with your values\n\
                 \n\
                 2. Or set variables manually:\n\
                    export GMAIL_CREDENTIALS_PATH=/path/to/client_credentials.json\n\
                    export GMAIL_TOKEN_CACHE_PATH=./gmail-token-cache.json\n\
                    cargo run -- --dry-run\n\
                 \n\
                 3. See GMAIL_API_MIGRATION.md for more information",
                missing_vars.join(", ")
            );
        }
        
        Ok(())
    }
}