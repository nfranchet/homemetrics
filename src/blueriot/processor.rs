use anyhow::{Result, Context};
use log::debug;

use crate::config::Config;
use crate::gmail_client::GmailClient;
use crate::database::Database;
use crate::slack_notifier::SlackNotifier;
use crate::email::{EmailProcessingStrategy, BaseEmailProcessor};
use super::extractor;

/// Blue Riot specific processing strategy
pub struct BlueRiotStrategy;

impl EmailProcessingStrategy for BlueRiotStrategy {
    fn search_emails<'a, 'b: 'a>(&'a self, gmail: &'b GmailClient) -> 
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(gmail.search_pool_emails())
    }
    
    fn process_single_email<'a, 'b: 'a, 'c: 'a>(
        &'a self,
        gmail: &'b GmailClient,
        database: Option<&'c Database>,
        slack: Option<&'c SlackNotifier>,
        message_id: &'a str,
        is_dry_run: bool,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<usize>> + Send + 'a>> {
        Box::pin(async move {
            debug!("Processing Blue Riot email ID: {}", message_id);
            
            // Fetch email metadata
            let (subject, _from) = gmail.fetch_email_metadata(message_id).await?;
            debug!("Email subject: {}", subject);
            
            // Fetch complete email content
            let email = gmail.fetch_email_complete(message_id).await?;
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
            
            if is_dry_run {
                println!("\n📧 Email: {}", subject);
                println!("📅 Date: {}", email.date);
                println!("📄 Text content (first 500 chars):\n{}\n", 
                         &text_content.chars().take(500).collect::<String>());
            }
            
            // Extract pool metrics from email text
            let pool_reading = extractor::extract_pool_metrics(&text_content, email.date)
                .context("Failed to extract pool metrics from email")?;
            
            if is_dry_run {
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
                if let Some(db) = database {
                    db.save_pool_reading(&pool_reading, message_id).await?;
                    
                    // Send Slack notification
                    if let Some(slack) = slack {
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
            }
            
            // Return 1 to indicate one record processed (the pool reading)
            Ok(1)
        })
    }
    
    fn mark_email_processed<'a, 'b: 'a>(
        &'a self,
        gmail: &'b GmailClient,
        message_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(gmail.mark_pool_email_as_processed(message_id))
    }
    
    fn processor_name(&self) -> &str {
        "Blue Riot"
    }
    
    fn label_name(&self) -> &str {
        "homemetrics/todo/blueriot"
    }
}

/// Blue Riot email processor (wrapper around BaseEmailProcessor)
pub struct BlueRiotEmailProcessor {
    base: BaseEmailProcessor<BlueRiotStrategy>,
}

impl BlueRiotEmailProcessor {
    pub async fn new(config: &Config, dry_run: bool) -> Result<Self> {
        let processor = if dry_run {
            BlueRiotEmailProcessor {
                base: BaseEmailProcessor::new_dry_run(config.clone(), BlueRiotStrategy)?,
            }
        } else {
            BlueRiotEmailProcessor {
                base: BaseEmailProcessor::new(config.clone(), BlueRiotStrategy).await?,
            }
        };
        Ok(processor)
    }
    
    pub async fn process_emails(&self, limit: Option<usize>) -> Result<()> {
        self.base.process_emails(limit).await?;
        Ok(())
    }
}
