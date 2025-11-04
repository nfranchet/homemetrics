use anyhow::{Result, Context};
use log::{info, debug, error};

use crate::config::Config;
use crate::gmail_client::GmailClient;
use crate::pool_extractor;
use crate::database::Database;
use crate::slack_notifier::SlackNotifier;

pub struct PoolEmailProcessor {
    gmail: GmailClient,
    database: Option<Database>,
    slack: Option<SlackNotifier>,
    dry_run: bool,
}

impl PoolEmailProcessor {
    pub async fn new(config: &Config, dry_run: bool) -> Result<Self> {
        info!("Initializing pool email processor (Blue Riot)");
        
        let gmail = GmailClient::new(&config.gmail).await?;
        
        let database = if dry_run {
            info!("🧪 DRY-RUN MODE - No database connection");
            None
        } else {
            Some(Database::new(&config.database).await?)
        };
        
        let slack = match &config.slack {
            Some(slack_config) => {
                let notifier = SlackNotifier::new(slack_config)?;
                info!("✅ Slack notifications enabled");
                Some(notifier)
            }
            None => {
                info!("ℹ️  Slack notifications disabled");
                None
            }
        };
        
        Ok(PoolEmailProcessor {
            gmail,
            database,
            slack,
            dry_run,
        })
    }
    
    pub async fn process_emails(&mut self, limit: Option<usize>) -> Result<()> {
        info!("Starting Blue Riot pool email processing");
        
        if self.dry_run {
            println!("\n================================================================================");
            println!("🧪 MODE DRY-RUN - POOL METRICS ANALYSIS (BLUE RIOT)");
            println!("================================================================================\n");
        }
        
        // Search for emails with label homemetrics/todo/blueriot
        let message_ids = self.gmail.search_pool_emails().await?;
        
        if message_ids.is_empty() {
            info!("No Blue Riot emails to process");
            return Ok(());
        }
        
        info!("Found {} Blue Riot email(s) to process", message_ids.len());
        
        let messages_to_process = if let Some(limit) = limit {
            info!("Limiting to {} email(s)", limit);
            &message_ids[..message_ids.len().min(limit)]
        } else {
            &message_ids[..]
        };
        
        for (index, message_id) in messages_to_process.iter().enumerate() {
            info!("Processing Blue Riot email {}/{}: {}", index + 1, messages_to_process.len(), message_id);
            
            match self.process_single_email(message_id).await {
                Ok(_) => info!("✅ Email {} processed successfully", message_id),
                Err(e) => {
                    error!("❌ Error processing email {}: {:?}", message_id, e);
                    if let Some(slack) = &self.slack {
                        let _ = slack.send_message(&format!("Error processing Blue Riot email {}: {}", message_id, e)).await;
                    }
                }
            }
        }
        
        info!("Blue Riot email processing completed");
        Ok(())
    }
    
    async fn process_single_email(&mut self, message_id: &str) -> Result<()> {
        // Fetch email metadata
        let (subject, _from) = self.gmail.fetch_email_metadata(message_id).await?;
        debug!("Email subject: {}", subject);
        
        // Fetch complete email content
        let email = self.gmail.fetch_email_complete(message_id).await?;
        debug!("Email date: {}", email.date);
        
        // Parse email to extract text body
        let parsed_email = mail_parser::MessageParser::default()
            .parse(&email.content)
            .context("Failed to parse email")?;
        
                // Extract text from email body
        let mut text_content = String::new();
        
        // Try to get text/plain body first
        if let Some(text_body) = parsed_email.body_text(0) {
            text_content.push_str(&text_body);
        }
        
        // If no text/plain, try text/html
        if text_content.is_empty() {
            if let Some(html_body) = parsed_email.body_html(0) {
                // Simple HTML stripping (remove tags)
                let html_str = html_body.to_string()
                    .replace("<br>", "\n")
                    .replace("<BR>", "\n")
                    .replace("</p>", "\n")
                    .replace("</P>", "\n");
                // Remove all HTML tags
                let tag_regex = regex::Regex::new(r"<[^>]+>").unwrap();
                text_content = tag_regex.replace_all(&html_str, "").to_string();
            }
        }
        
        // Fallback to raw content if still empty
        if text_content.is_empty() {
            text_content = String::from_utf8_lossy(&email.content).to_string();
        }
        
        if self.dry_run {
            println!("\n📧 Email: {}", subject);
            println!("📅 Date: {}", email.date);
            println!("📄 Text content (first 500 chars):\n{}\n", 
                     &text_content.chars().take(500).collect::<String>());
        }
        
        // Extract pool metrics from email text
        let pool_reading = pool_extractor::extract_pool_metrics(&text_content, email.date)
            .context("Failed to extract pool metrics from email")?;
        
        if self.dry_run {
            println!("🏊 Pool Metrics Extracted:");
            if let Some(temp) = pool_reading.temperature {
                println!("   🌡️  Temperature: {:.1}°C", temp);
            }
            if let Some(ph) = pool_reading.ph {
                println!("   🧪 pH: {:.2}", ph);
            }
            if let Some(orp) = pool_reading.orp {
                println!("   ⚡ ORP: {} mV", orp);
            }
            println!();
        } else {
            // Save to database
            if let Some(db) = &self.database {
                db.save_pool_reading(&pool_reading, message_id).await?;
                
                // Send Slack notification
                if let Some(slack) = &self.slack {
                    let mut metrics = Vec::new();
                    if let Some(temp) = pool_reading.temperature {
                        metrics.push(format!("🌡️ {}°C", temp));
                    }
                    if let Some(ph) = pool_reading.ph {
                        metrics.push(format!("🧪 pH {:.2}", ph));
                    }
                    if let Some(orp) = pool_reading.orp {
                        metrics.push(format!("⚡ {} mV", orp));
                    }
                    
                    let message = format!(
                        "🏊 New pool reading: {}\nFrom: {}",
                        metrics.join(" | "),
                        subject
                    );
                    
                    let _ = slack.send_message(&message).await;
                }
            }
            
            // Mark email as processed:
            // 1. Mark as read
            // 2. Remove from inbox
            // 3. Replace label homemetrics/todo/blueriot with homemetrics/done/blueriot
            self.gmail.mark_pool_email_as_processed(message_id).await?;
        }
        
        Ok(())
    }
}
