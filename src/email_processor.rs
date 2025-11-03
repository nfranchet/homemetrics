use anyhow::{Result, Context};
use log::{info, debug, warn, error};

use crate::config::Config;
use crate::gmail_client::GmailClient;
use crate::attachment_parser::{AttachmentParser, Attachment};
use crate::temperature_extractor::TemperatureExtractor;
use crate::database::Database;
use crate::slack_notifier::SlackNotifier;

pub struct EmailProcessor {
    config: Config,
    database: Option<Database>,
    slack: Option<SlackNotifier>,
}

impl EmailProcessor {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initializing email processor");
        
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
        
        Ok(EmailProcessor {
            config,
            database: Some(database),
            slack,
        })
    }
    
    pub fn new_dry_run(config: Config) -> Result<Self> {
        info!("🧪 Initializing email processor en mode dry-run (without database)");
        
        Ok(EmailProcessor {
            config,
            database: None,
            slack: None,  // Pas de notifications Slack en mode dry-run
        })
    }
    
    pub async fn process_emails(&mut self, limit: Option<usize>) -> Result<usize> {
        info!("Démarrage du traitement des emails X-Sense");
        self.process_emails_common(limit, false).await
    }
    
    pub async fn process_emails_dry_run(&self, limit: Option<usize>) -> Result<usize> {
        println!("\n{}", "=".repeat(80));
        println!("🧪 MODE DRY-RUN - ANALYSE DES EMAILS X-SENSE");
        println!("{}", "=".repeat(80));
        
        self.process_emails_common(limit, true).await
    }
    
    // Fonction commune pour traiter les emails en mode dry-run ou normal
    async fn process_emails_common(&self, limit: Option<usize>, is_dry_run: bool) -> Result<usize> {
        // 1. Connect to Gmail API
        let gmail_client = GmailClient::new(&self.config.gmail).await
            .context("Unable to connect to Gmail API")?;
        
        // 2. Rechercher les emails avec le label 'homemetrics-todo-xsense'
        let message_ids = gmail_client.search_xsense_emails()
            .await
            .context("Error searching for emails")?;
        
        if message_ids.is_empty() {
            if is_dry_run {
                println!("❌ No emails found with label 'homemetrics-todo-xsense'");
                println!("   Astuce: Ajoutez le label 'homemetrics-todo-xsense' aux emails X-Sense à traiter");
            } else {
                info!("No emails found with label 'homemetrics-todo-xsense'");
            }
            return Ok(0);
        }
        
        if is_dry_run {
            println!("✅ Found {} email(s) matching criteria\n", message_ids.len());
        }
        
        let mut total_processed = 0;
        let mut total_readings_saved = 0;
        
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
            
            match self.process_single_email_common(&gmail_client, message_id, is_dry_run).await {
                Ok(readings_count) => {
                    total_processed += 1;
                    if readings_count == 0 {
                        // Special case: email skipped (unexpected subject or no attachments)
                        if is_dry_run {
                            println!("✅ Email {} analyzed without success\n", message_id);
                        } else {
                            info!("Email {} traité sans succès", message_id);
                        }
                        continue; // Skip moving email if no readings were processed
                    }
                    total_readings_saved += readings_count;
                    

                    if is_dry_run {
                        println!("✅ Email {} analysé avec succès\n", message_id);
                    } else {
                        info!("Email {} traité avec succès: {} readings sauvegardées", 
                              message_id, readings_count);
                    }
                }
                Err(e) => {
                    if is_dry_run {
                        println!("❌ Error analyzing email {}: {}\n", message_id, e);
                    } else {
                        error!("Error processing email {}: {}", message_id, e);
                    }
                }
            }
        }
        
        // Pas besoin de logout avec l'API REST Gmail
        
        if is_dry_run {
            println!("{}", "=".repeat(80));
            println!("🏁 Analysis completed: {} emails analyzed sur {}", total_processed, emails_to_process.len());
            println!("📁 Attachments saved in: {}", self.config.data_dir);
            println!("{}", "=".repeat(80));
        } else {
            info!("Processing completed: {} emails processed, {} readings de température sauvegardées", 
                  total_processed, total_readings_saved);
        }
        
        Ok(total_processed)
    }
    
    // Fonction commune pour traiter un seul email selon le mode
    async fn process_single_email_common(
        &self,
        gmail_client: &GmailClient,
        message_id: &str,
        is_dry_run: bool,
    ) -> Result<usize> {
        if is_dry_run {
            debug!("Analyse de l'email ID: {}", message_id);
        } else {
            debug!("Traitement de l'email ID: {}", message_id);
        }
        
        // 1. Retrieve all email information in one call
        let email_info = match gmail_client.fetch_email_complete(message_id).await {
            Ok(info) => info,
            Err(e) => {
                // In case of error, try to retrieve at least metadata for display
                let (subject, from) = gmail_client.fetch_email_metadata(message_id)
                    .await
                    .unwrap_or((String::from("Sujet inconnu"), String::from("Expéditeur inconnu")));
                
                return Err(anyhow::anyhow!(
                    "Unable to retrieve complete email\n  Sujet: {}\n  De: {}\n  Erreur: {}", 
                    subject, from, e
                ));
            }
        };
        
        // 2. En mode dry-run, afficher les headers et la date
        if is_dry_run {
            println!("📋 Headers:");
            println!("{}", email_info.headers);
            println!();
            
            println!("📅 Date de l'email: {}", email_info.date.format("%Y-%m-%d %H:%M:%S UTC"));
            println!();
            println!("📄 Contenu de l'email:");
            println!("   Size: {} bytes", email_info.content.len());
            
            // Try to display text content preview
            if let Ok(content_str) = std::str::from_utf8(&email_info.content) {
                let lines: Vec<&str> = content_str.lines().collect();
                let preview_lines = std::cmp::min(10, lines.len());
                
                println!("   Aperçu (premières {} lignes):", preview_lines);
                for (i, line) in lines.iter().take(preview_lines).enumerate() {
                    let preview_line = if line.len() > 80 {
                        format!("{}...", &line[..77])
                    } else {
                        line.to_string()
                    };
                    println!("   {:2}: {}", i + 1, preview_line);
                }
                
                if lines.len() > preview_lines {
                    println!("   ... ({} lignes supplémentaires)", lines.len() - preview_lines);
                }
            }
            println!();
        }

        // Check email subject for expected pattern
        if !email_info.subject.starts_with("Votre exportation de") {
            if is_dry_run {
                println!("❌ Sujet inattendu: '{}'", email_info.subject);
            } else {
                warn!("Sujet inattendu pour l'email {}: '{}'", message_id, email_info.subject);
            }
            return Ok(0);
        }

        // 5. Extract attachments
        let attachments = AttachmentParser::parse_email(&email_info.content)
            .context("Error extracting attachments")?;
        
        if attachments.is_empty() {
            if is_dry_run {
                println!("📎 No attachment found");
            } else {
                warn!("No attachment found dans l'email {}", message_id);
            }
            return Ok(0);
        }
        
        if is_dry_run {
            println!("📎 Attachments found: {}", attachments.len());
            println!();
        }
        
        let mut total_readings = 0;
        let mut sensor_details: Vec<(String, usize)> = Vec::new();
        
        // 6. Process each attachment
        for attachment in attachments {                            
            match AttachmentParser::save_attachment_to_data_dir_with_date(&attachment, &self.config.data_dir, Some(email_info.date)) {
                Ok(path) => {
                    println!("💾 Sauvegardé dans: {:?}", path);
                }
                Err(e) => {
                    println!("❌ Save error: {}", e);
                }
            }

            if is_dry_run {
                // Dry-run mode : afficher info et sauvegarder seulement
                AttachmentParser::display_attachment_info(&attachment);
                println!();
            } else {
                // Mode normal : traitement complet with database
                match self.process_attachment(&attachment).await {
                    Ok((sensor_name, readings_count)) => {
                        total_readings += readings_count;
                        sensor_details.push((sensor_name.clone(), readings_count));
                        info!("Attachment '{}' processed: {} readings for sensor '{}'", 
                              attachment.filename, readings_count, sensor_name);
                    }
                    Err(e) => {
                        error!("Error processing attachment '{}': {}", 
                               attachment.filename, e);
                        // Continue with other attachments
                    }
                }
            }
        }
        
        // 7. Mark email as processed and send Slack notification (normal mode only)
        if !is_dry_run && total_readings > 0 {
            // 7a. Marquer avec le label
            match gmail_client.mark_email_as_processed(message_id).await {
                Ok(_) => {
                    info!("Email {} marqué comme traité", message_id);
                }
                Err(e) => {
                    error!("Unable to mark email {} as processed: {}", 
                           message_id, e);
                    // Continue anyway, error is not fatal
                }
            }
            
            // 7b. Send Slack notification
            if let Some(ref slack) = self.slack {
                match slack.notify_email_processed(
                    &email_info.id,
                    &email_info.subject,
                    email_info.date,
                    total_readings,
                    sensor_details,
                ).await {
                    Ok(_) => {
                        info!("✅ Notification Slack envoyée pour l'email {}", message_id);
                    }
                    Err(e) => {
                        error!("❌ Error sending Slack notification: {}", e);
                        // Do not fail processing if Slack fails
                    }
                }
            }
        }
        
        Ok(total_readings)
    }
    
    async fn process_attachment(&self, attachment: &Attachment) -> Result<(String, usize)> {
        debug!("Traitement de la pièce jointe: {}", attachment.filename);
        
        // 1. Extract temperature data from attachment
        let temperature_readings = TemperatureExtractor::extract_from_attachment(attachment)
            .context("Error extracting temperature data")?;
        
        if temperature_readings.is_empty() {
            warn!("No temperature data found in '{}'", attachment.filename);
            return Ok(("unknown".to_string(), 0));
        }
        
        // Extract sensor name from readings (all have same sensor_id)
        let sensor_name = temperature_readings.first()
            .map(|r| r.sensor_id.clone())
            .unwrap_or_else(|| "unknown".to_string());
        
        // 2. Save readings to database (if available)
        let saved_count = if let Some(ref database) = self.database {
            database.save_temperature_readings(&temperature_readings).await
                .context("Error saving to database")?
        } else {
            // Dry-run mode : pas de sauvegarde
            debug!("Dry-run mode : {} readings ignorées (pas de sauvegarde)", temperature_readings.len());
            0
        };
        
        debug!("Attachment '{}' completed: {} readings extracted, {} saved for sensor '{}'", 
               attachment.filename, temperature_readings.len(), saved_count, sensor_name);
        
        Ok((sensor_name, saved_count))
    }
    
    #[allow(dead_code)]
    pub async fn get_recent_readings(&self, sensor_id: Option<&str>, limit: i64) -> Result<Vec<crate::temperature_extractor::TemperatureReading>> {
        if let Some(ref database) = self.database {
            database.get_latest_readings(sensor_id, limit).await
        } else {
            Ok(Vec::new()) // Dry-run mode: no data
        }
    }
    
    #[allow(dead_code)]
    pub async fn close(self) -> Result<()> {
        info!("Fermeture du processeur d'emails");
        if let Some(database) = self.database {
            database.close().await
                .context("Error closing database")?;
        }
        Ok(())
    }
}