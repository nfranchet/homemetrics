use anyhow::{Result, Context};
use log::{info, error, warn};

use crate::config::Config;
use crate::gmail_client::GmailClient;
use crate::database::Database;
use crate::slack_notifier::SlackNotifier;

/// Trait that defines the specific processing logic for each email type
pub trait EmailProcessingStrategy: Send {
    /// Search for emails to process (returns message IDs)
    fn search_emails<'a, 'b: 'a>(&'a self, gmail: &'b GmailClient) -> 
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>>> + Send + 'a>>;
    
    /// Process a single email and return the number of records processed
    fn process_single_email<'a, 'b: 'a, 'c: 'a>(
        &'a self,
        gmail: &'b GmailClient,
        database: Option<&'c Database>,
        slack: Option<&'c SlackNotifier>,
        message_id: &'a str,
        is_dry_run: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize>> + Send + 'a>>;
    
    /// Mark email as processed (labels, archive, etc.)
    fn mark_email_processed<'a, 'b: 'a>(
        &'a self,
        gmail: &'b GmailClient,
        message_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>>;
    
    /// Get the name of this processor (for logging)
    fn processor_name(&self) -> &str;
    
    /// Get the label name (for logging)
    fn label_name(&self) -> &str;
}

/// Base email processor that handles common logic
pub struct BaseEmailProcessor<S: EmailProcessingStrategy> {
    config: Config,
    database: Option<Database>,
    slack: Option<SlackNotifier>,
    strategy: S,
}

impl<S: EmailProcessingStrategy> BaseEmailProcessor<S> {
    pub async fn new(config: Config, strategy: S) -> Result<Self> {
        info!("Initializing {} email processor", strategy.processor_name());
        
        // Initialize database connection
        let database = Database::new(&config.database).await
            .context("Unable to initialize database")?;
        
        // Initialize Slack notifier if configured
        let slack = if let Some(slack_config) = &config.slack {
            match SlackNotifier::new(slack_config) {
                Ok(notifier) => {
                    info!("✅ Slack notifications enabled");
                    Some(notifier)
                },
                Err(e) => {
                    warn!("⚠️  Unable to initialize Slack notifier: {} - notifications disabled", e);
                    None
                }
            }
        } else {
            info!("ℹ️  Slack notifications not configured");
            None
        };
        
        Ok(BaseEmailProcessor {
            config,
            database: Some(database),
            slack,
            strategy,
        })
    }
    
    pub fn new_dry_run(config: Config, strategy: S) -> Result<Self> {
        info!("🧪 Initializing {} email processor in dry-run mode (without database)", strategy.processor_name());
        
        Ok(BaseEmailProcessor {
            config,
            database: None,
            slack: None,  // No Slack notifications in dry-run mode
            strategy,
        })
    }
    
    pub async fn process_emails(&self, limit: Option<usize>) -> Result<usize> {
        info!("Starting {} email processing", self.strategy.processor_name());
        self.process_emails_common(limit, false).await
    }
    
    pub async fn process_emails_dry_run(&self, limit: Option<usize>) -> Result<usize> {
        println!("\n{}", "=".repeat(80));
        println!("🧪 MODE DRY-RUN - {} ANALYSIS", self.strategy.processor_name().to_uppercase());
        println!("{}", "=".repeat(80));
        
        self.process_emails_common(limit, true).await
    }
    
    /// Common processing logic for both normal and dry-run modes
    async fn process_emails_common(&self, limit: Option<usize>, is_dry_run: bool) -> Result<usize> {
        // 1. Connect to Gmail API
        let gmail_client = GmailClient::new(&self.config.gmail).await
            .context("Unable to connect to Gmail API")?;
        
        // 2. Search for emails using strategy
        let message_ids = self.strategy.search_emails(&gmail_client).await
            .context("Error searching for emails")?;
        
        if message_ids.is_empty() {
            if is_dry_run {
                println!("❌ No emails found with label '{}'", self.strategy.label_name());
                println!("   Hint: Add the label '{}' to emails to process", self.strategy.label_name());
            } else {
                info!("No emails found with label '{}'", self.strategy.label_name());
            }
            return Ok(0);
        }
        
        if is_dry_run {
            println!("✅ Found {} email(s) matching criteria\n", message_ids.len());
        }
        
        let mut total_processed = 0;
        let mut total_records_saved = 0;
        
        // 3. Process each found email (with optional limit)
        let emails_to_process = if let Some(limit) = limit {
            message_ids.into_iter().take(limit).collect()
        } else {
            message_ids
        };
        
        for (index, message_id) in emails_to_process.iter().enumerate() {
            if is_dry_run {
                println!("📧 Email {}/{} (ID: {})", index + 1, emails_to_process.len(), message_id);
                println!("{}", "-".repeat(60));
            }
            
            match self.strategy.process_single_email(
                &gmail_client,
                self.database.as_ref(),
                self.slack.as_ref(),
                message_id,
                is_dry_run
            ).await {
                Ok(records_count) => {
                    total_processed += 1;
                    
                    if records_count == 0 {
                        // Special case: email skipped (no data extracted)
                        if is_dry_run {
                            println!("⚠️  Email {} analyzed but no data extracted\n", message_id);
                        } else {
                            warn!("Email {} processed but no data extracted", message_id);
                        }
                        continue; // Skip marking as processed if no data
                    }
                    
                    total_records_saved += records_count;
                    
                    // Mark email as processed (unless dry-run)
                    if !is_dry_run {
                        if let Err(e) = self.strategy.mark_email_processed(&gmail_client, message_id).await {
                            error!("Failed to mark email {} as processed: {}", message_id, e);
                        }
                    }
                    
                    if is_dry_run {
                        println!("✅ Email {} analyzed successfully ({} record(s))\n", message_id, records_count);
                    } else {
                        info!("Email {} processed successfully: {} record(s) saved", message_id, records_count);
                    }
                }
                Err(e) => {
                    if is_dry_run {
                        println!("❌ Error analyzing email {}: {}\n", message_id, e);
                    } else {
                        error!("Error processing email {}: {}", message_id, e);
                        
                        // Send error notification to Slack
                        if let Some(slack) = &self.slack {
                            let _ = slack.send_message(&format!(
                                "❌ Error processing {} email {}: {}",
                                self.strategy.processor_name(),
                                message_id,
                                e
                            )).await;
                        }
                    }
                }
            }
        }
        
        if is_dry_run {
            println!("{}", "=".repeat(80));
            println!("🏁 Analysis completed: {} emails analyzed out of {}", total_processed, emails_to_process.len());
            println!("📊 Total records: {}", total_records_saved);
            println!("{}", "=".repeat(80));
        } else {
            info!("Processing completed: {} emails processed, {} records saved", 
                  total_processed, total_records_saved);
        }
        
        Ok(total_processed)
    }
}
