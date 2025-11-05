use anyhow::Result;
use log::{debug, info};

use crate::config::Config;
use crate::gmail_client::GmailClient;
use crate::database::Database;
use crate::slack_notifier::SlackNotifier;
use crate::attachment_parser::AttachmentParser;
use crate::email::{EmailProcessingStrategy, BaseEmailProcessor};
use super::extractor::TemperatureExtractor;

/// X-Sense specific processing strategy
pub struct XSenseStrategy;

impl EmailProcessingStrategy for XSenseStrategy {
    fn search_emails<'a, 'b: 'a>(&'a self, gmail: &'b GmailClient) -> 
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(gmail.search_xsense_emails())
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
            debug!("Processing X-Sense email ID: {}", message_id);
            
            // 1. Retrieve complete email information
            let email_info = match gmail.fetch_email_complete(message_id).await {
                Ok(info) => info,
                Err(e) => {
                    let (subject, from) = gmail.fetch_email_metadata(message_id)
                        .await
                        .unwrap_or((String::from("Unknown subject"), String::from("Unknown sender")));
                    
                    return Err(anyhow::anyhow!(
                        "Unable to retrieve complete email\n  Subject: {}\n  From: {}\n  Error: {}", 
                        subject, from, e
                    ));
                }
            };
            
            // 2. In dry-run mode, display headers and date
            if is_dry_run {
                println!("📋 Headers:");
                println!("{}", email_info.headers);
                println!();
                
                println!("📅 Email date: {}", email_info.date.format("%Y-%m-%d %H:%M:%S UTC"));
                println!();
                println!("📄 Email content:");
                println!("   Size: {} bytes", email_info.content.len());
                println!();
            }
            
            // 3. Parse attachments
            let attachments = AttachmentParser::parse_email(&email_info.content)?;
            
            if attachments.is_empty() {
                if is_dry_run {
                    println!("⚠️  No attachments found in this email");
                }
                return Ok(0);
            }
            
            if is_dry_run {
                println!("📎 Found {} attachment(s):", attachments.len());
                for (i, att) in attachments.iter().enumerate() {
                    println!("   {}. {} ({} bytes, type: {})", 
                             i + 1, att.filename, att.content.len(), att.content_type);
                }
                println!();
            }
            
            // 4. Process each attachment
            let mut total_readings = 0;
            
            for (index, attachment) in attachments.iter().enumerate() {
                if is_dry_run {
                    println!("🔍 Processing attachment {}/{}: {}", 
                             index + 1, attachments.len(), attachment.filename);
                }
                
                match TemperatureExtractor::extract_from_attachment(attachment) {
                    Ok(readings) => {
                        if is_dry_run {
                            Self::display_readings_dry_run(&readings);
                        } else if let Some(db) = database {
                            // Save to database
                            match db.save_temperature_readings(&readings).await {
                                Ok(count) => {
                                    total_readings += count;
                                    debug!("Saved {} readings from {}", count, attachment.filename);
                                }
                                Err(e) => {
                                    debug!("Error saving readings from {}: {}", attachment.filename, e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        if is_dry_run {
                            println!("   ⚠️  Unable to extract data: {}", e);
                        }
                    }
                }
            }
            
            // 5. Send Slack notification (if not dry-run and has data)
            if !is_dry_run && total_readings > 0 {
                if let Some(slack) = slack {
                    let message = format!(
                        "📊 New X-Sense data: {} temperature readings\nFrom: {}",
                        total_readings,
                        email_info.headers.lines().find(|l| l.starts_with("Subject:"))
                            .unwrap_or("Subject: Unknown")
                            .trim_start_matches("Subject:")
                            .trim()
                    );
                    info!("Sending Slack notification for X-Sense readings");
                    if let Err(e) = slack.send_message(&message).await {
                        debug!("Failed to send Slack notification: {}", e);
                    } else {
                        debug!("Slack notification sent successfully");
                    }
                }
            }
            
            Ok(total_readings)
        })
    }
    
    fn mark_email_processed<'a, 'b: 'a>(
        &'a self,
        gmail: &'b GmailClient,
        message_id: &'a str,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(gmail.mark_email_as_processed(message_id))
    }
    
    fn processor_name(&self) -> &str {
        "X-Sense"
    }
    
    fn label_name(&self) -> &str {
        "homemetrics-todo-xsense"
    }
}

impl XSenseStrategy {
    fn display_readings_dry_run(readings: &[crate::xsense::TemperatureReading]) {
        if readings.is_empty() {
            println!("   ⚠️  No valid readings extracted");
            return;
        }
        
        println!("   ✅ Extracted {} reading(s):", readings.len());
        
        // Group by sensor
        let mut by_sensor: std::collections::HashMap<String, Vec<&crate::xsense::TemperatureReading>> = 
            std::collections::HashMap::new();
        
        for reading in readings {
            by_sensor.entry(reading.sensor_id.clone())
                .or_insert_with(Vec::new)
                .push(reading);
        }
        
        for (sensor_id, sensor_readings) in by_sensor.iter() {
            println!("\n   📡 Sensor: {}", sensor_id);
            
            // Display first and last reading
            if let Some(first) = sensor_readings.first() {
                let humidity_str = first.humidity
                    .map(|h| format!("{:.1}%", h))
                    .unwrap_or_else(|| "N/A".to_string());
                println!("      First: {} | Temp: {:.1}°C | Humidity: {}",
                         first.timestamp.format("%Y-%m-%d %H:%M:%S"),
                         first.temperature,
                         humidity_str);
            }
            
            if sensor_readings.len() > 1 {
                if let Some(last) = sensor_readings.last() {
                    let humidity_str = last.humidity
                        .map(|h| format!("{:.1}%", h))
                        .unwrap_or_else(|| "N/A".to_string());
                    println!("      Last:  {} | Temp: {:.1}°C | Humidity: {}",
                             last.timestamp.format("%Y-%m-%d %H:%M:%S"),
                             last.temperature,
                             humidity_str);
                }
                
                if sensor_readings.len() > 2 {
                    println!("      ... {} more readings", sensor_readings.len() - 2);
                }
            }
        }
        println!();
    }
}

/// X-Sense email processor (wrapper around BaseEmailProcessor)
pub struct XSenseEmailProcessor {
    base: BaseEmailProcessor<XSenseStrategy>,
}

impl XSenseEmailProcessor {
    pub async fn new(config: Config) -> Result<Self> {
        Ok(XSenseEmailProcessor {
            base: BaseEmailProcessor::new(config, XSenseStrategy).await?,
        })
    }
    
    pub fn new_dry_run(config: Config) -> Result<Self> {
        Ok(XSenseEmailProcessor {
            base: BaseEmailProcessor::new_dry_run(config, XSenseStrategy)?,
        })
    }
    
    pub async fn process_emails(&self, limit: Option<usize>) -> Result<usize> {
        self.base.process_emails(limit).await
    }
    
    pub async fn process_emails_dry_run(&self, limit: Option<usize>) -> Result<usize> {
        self.base.process_emails_dry_run(limit).await
    }
}
