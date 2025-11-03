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
        info!("Initialisation du processeur d'emails");
        
        // Initialiser la connexion à la base de données
        let database = Database::new(&config.database).await
            .context("Impossible d'initialiser la base de données")?;
        
        // Initialiser le notifieur Slack si configuré
        let slack = if let Some(slack_config) = &config.slack {
            match SlackNotifier::new(slack_config) {
                Ok(notifier) => {
                    info!("✅ Notifications Slack activées");
                    Some(notifier)
                },
                Err(e) => {
                    warn!("⚠️  Impossible d'initialiser le notifieur Slack: {} - notifications désactivées", e);
                    None
                }
            }
        } else {
            info!("ℹ️  Notifications Slack non configurées");
            None
        };
        
        Ok(EmailProcessor {
            config,
            database: Some(database),
            slack,
        })
    }
    
    pub fn new_dry_run(config: Config) -> Result<Self> {
        info!("🧪 Initialisation du processeur d'emails en mode dry-run (sans base de données)");
        
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
        // 1. Se connecter à l'API Gmail
        let gmail_client = GmailClient::new(&self.config.gmail).await
            .context("Impossible de se connecter à l'API Gmail")?;
        
        // 2. Rechercher les emails avec le label 'homemetrics-todo-xsense'
        let message_ids = gmail_client.search_xsense_emails()
            .await
            .context("Erreur lors de la recherche d'emails")?;
        
        if message_ids.is_empty() {
            if is_dry_run {
                println!("❌ Aucun email trouvé avec le label 'homemetrics-todo-xsense'");
                println!("   Astuce: Ajoutez le label 'homemetrics-todo-xsense' aux emails X-Sense à traiter");
            } else {
                info!("Aucun email trouvé avec le label 'homemetrics-todo-xsense'");
            }
            return Ok(0);
        }
        
        if is_dry_run {
            println!("✅ Trouvé {} email(s) correspondant aux critères\n", message_ids.len());
        }
        
        let mut total_processed = 0;
        let mut total_readings_saved = 0;
        
        // 3. Traiter chaque email trouvé (avec limite optionnelle)
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
                        // Cas spécial : email ignoré (sujet inattendu ou pas de pièces jointes)
                        if is_dry_run {
                            println!("✅ Email {} analysé sans succès\n", message_id);
                        } else {
                            info!("Email {} traité sans succès", message_id);
                        }
                        continue; // Skip moving email if no readings were processed
                    }
                    total_readings_saved += readings_count;
                    

                    if is_dry_run {
                        println!("✅ Email {} analysé avec succès\n", message_id);
                    } else {
                        info!("Email {} traité avec succès: {} lectures sauvegardées", 
                              message_id, readings_count);
                    }
                }
                Err(e) => {
                    if is_dry_run {
                        println!("❌ Erreur lors de l'analyse de l'email {}: {}\n", message_id, e);
                    } else {
                        error!("Erreur lors du traitement de l'email {}: {}", message_id, e);
                    }
                }
            }
        }
        
        // Pas besoin de logout avec l'API REST Gmail
        
        if is_dry_run {
            println!("{}", "=".repeat(80));
            println!("🏁 Analyse terminée: {} emails analysés sur {}", total_processed, emails_to_process.len());
            println!("📁 Pièces jointes sauvegardées dans: {}", self.config.data_dir);
            println!("{}", "=".repeat(80));
        } else {
            info!("Traitement terminé: {} emails traités, {} lectures de température sauvegardées", 
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
        
        // 1. Récupérer toutes les informations de l'email en un seul appel
        let email_info = match gmail_client.fetch_email_complete(message_id).await {
            Ok(info) => info,
            Err(e) => {
                // En cas d'erreur, essayer de récupérer au moins les métadonnées pour l'affichage
                let (subject, from) = gmail_client.fetch_email_metadata(message_id)
                    .await
                    .unwrap_or((String::from("Sujet inconnu"), String::from("Expéditeur inconnu")));
                
                return Err(anyhow::anyhow!(
                    "Impossible de récupérer l'email complet\n  Sujet: {}\n  De: {}\n  Erreur: {}", 
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
            println!("   Taille: {} bytes", email_info.content.len());
            
            // Essayer d'afficher un aperçu du contenu textuel
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

        // 5. Extraire les pièces jointes
        let attachments = AttachmentParser::parse_email(&email_info.content)
            .context("Erreur lors de l'extraction des pièces jointes")?;
        
        if attachments.is_empty() {
            if is_dry_run {
                println!("📎 Aucune pièce jointe trouvée");
            } else {
                warn!("Aucune pièce jointe trouvée dans l'email {}", message_id);
            }
            return Ok(0);
        }
        
        if is_dry_run {
            println!("📎 Pièces jointes trouvées: {}", attachments.len());
            println!();
        }
        
        let mut total_readings = 0;
        let mut sensor_details: Vec<(String, usize)> = Vec::new();
        
        // 6. Traiter chaque pièce jointe
        for attachment in attachments {                            
            match AttachmentParser::save_attachment_to_data_dir_with_date(&attachment, &self.config.data_dir, Some(email_info.date)) {
                Ok(path) => {
                    println!("💾 Sauvegardé dans: {:?}", path);
                }
                Err(e) => {
                    println!("❌ Erreur de sauvegarde: {}", e);
                }
            }

            if is_dry_run {
                // Mode dry-run : afficher info et sauvegarder seulement
                AttachmentParser::display_attachment_info(&attachment);
                println!();
            } else {
                // Mode normal : traitement complet avec base de données
                match self.process_attachment(&attachment).await {
                    Ok((sensor_name, readings_count)) => {
                        total_readings += readings_count;
                        sensor_details.push((sensor_name.clone(), readings_count));
                        info!("Pièce jointe '{}' traitée: {} lectures pour le sensor '{}'", 
                              attachment.filename, readings_count, sensor_name);
                    }
                    Err(e) => {
                        error!("Erreur lors du traitement de la pièce jointe '{}': {}", 
                               attachment.filename, e);
                        // Continuer avec les autres pièces jointes
                    }
                }
            }
        }
        
        // 7. Marquer l'email comme traité et envoyer notification Slack (mode normal uniquement)
        if !is_dry_run && total_readings > 0 {
            // 7a. Marquer avec le label
            match gmail_client.mark_email_as_processed(message_id).await {
                Ok(_) => {
                    info!("Email {} marqué comme traité", message_id);
                }
                Err(e) => {
                    error!("Impossible de marquer l'email {} comme traité: {}", 
                           message_id, e);
                    // Continuer quand même, l'erreur n'est pas fatale
                }
            }
            
            // 7b. Envoyer notification Slack
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
                        error!("❌ Erreur lors de l'envoi de la notification Slack: {}", e);
                        // Ne pas faire échouer le traitement si Slack échoue
                    }
                }
            }
        }
        
        Ok(total_readings)
    }
    
    async fn process_attachment(&self, attachment: &Attachment) -> Result<(String, usize)> {
        debug!("Traitement de la pièce jointe: {}", attachment.filename);
        
        // 1. Extraire les données de température de la pièce jointe
        let temperature_readings = TemperatureExtractor::extract_from_attachment(attachment)
            .context("Erreur lors de l'extraction des données de température")?;
        
        if temperature_readings.is_empty() {
            warn!("Aucune donnée de température trouvée dans '{}'", attachment.filename);
            return Ok(("unknown".to_string(), 0));
        }
        
        // Extraire le nom du sensor depuis les lectures (toutes ont le même sensor_id)
        let sensor_name = temperature_readings.first()
            .map(|r| r.sensor_id.clone())
            .unwrap_or_else(|| "unknown".to_string());
        
        // 2. Sauvegarder les lectures dans la base de données (si disponible)
        let saved_count = if let Some(ref database) = self.database {
            database.save_temperature_readings(&temperature_readings).await
                .context("Erreur lors de la sauvegarde en base de données")?
        } else {
            // Mode dry-run : pas de sauvegarde
            debug!("Mode dry-run : {} lectures ignorées (pas de sauvegarde)", temperature_readings.len());
            0
        };
        
        debug!("Pièce jointe '{}' terminée: {} lectures extraites, {} sauvegardées pour le sensor '{}'", 
               attachment.filename, temperature_readings.len(), saved_count, sensor_name);
        
        Ok((sensor_name, saved_count))
    }
    
    #[allow(dead_code)]
    pub async fn get_recent_readings(&self, sensor_id: Option<&str>, limit: i64) -> Result<Vec<crate::temperature_extractor::TemperatureReading>> {
        if let Some(ref database) = self.database {
            database.get_latest_readings(sensor_id, limit).await
        } else {
            Ok(Vec::new()) // Mode dry-run : pas de données
        }
    }
    
    #[allow(dead_code)]
    pub async fn close(self) -> Result<()> {
        info!("Fermeture du processeur d'emails");
        if let Some(database) = self.database {
            database.close().await
                .context("Erreur lors de la fermeture de la base de données")?;
        }
        Ok(())
    }
}