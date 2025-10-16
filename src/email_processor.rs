use anyhow::{Result, Context};
use log::{info, debug, warn, error};

use crate::config::Config;
use crate::imap_client::ImapClient;
use crate::attachment_parser::{AttachmentParser, Attachment};
use crate::temperature_extractor::TemperatureExtractor;
use crate::database::Database;

pub struct EmailProcessor {
    config: Config,
    database: Option<Database>,
}

impl EmailProcessor {
    pub async fn new(config: Config) -> Result<Self> {
        info!("Initialisation du processeur d'emails");
        
        // Initialiser la connexion à la base de données
        let database = Database::new(&config.database).await
            .context("Impossible d'initialiser la base de données")?;
        
        Ok(EmailProcessor {
            config,
            database: Some(database),
        })
    }
    
    pub fn new_dry_run(config: Config) -> Result<Self> {
        info!("🧪 Initialisation du processeur d'emails en mode dry-run (sans base de données)");
        
        Ok(EmailProcessor {
            config,
            database: None,
        })
    }
    
    pub async fn process_emails(&mut self, limit: Option<usize>) -> Result<usize> {
        info!("Démarrage du traitement des emails X-Sense");
        
        // 1. Se connecter au serveur IMAP
        let mut imap_client = ImapClient::new(&self.config.imap).await
            .context("Impossible de se connecter au serveur IMAP")?;
        
        // 2. Rechercher les emails de support@x-sense.com
        let message_ids = imap_client.search_xsense_emails()
            .context("Erreur lors de la recherche d'emails")?;
        
        if message_ids.is_empty() {
            info!("Aucun email trouvé correspondant aux critères");
            return Ok(0);
        }
        
        let mut total_processed = 0;
        let mut total_readings_saved = 0;
        
        // 3. Traiter chaque email trouvé (avec limite optionnelle)
        let emails_to_process = if let Some(limit) = limit {
            message_ids.into_iter().take(limit).collect()
        } else {
            message_ids
        };
        
        for message_id in emails_to_process {
            match self.process_single_email(&mut imap_client, message_id).await {
                Ok(readings_count) => {
                    total_processed += 1;
                    total_readings_saved += readings_count;
                    info!("Email {} traité avec succès: {} lectures sauvegardées", 
                          message_id, readings_count);
                }
                Err(e) => {
                    error!("Erreur lors du traitement de l'email {}: {}", message_id, e);
                    // Continuer avec les autres emails
                }
            }
        }
        
        // 4. Se déconnecter proprement
        imap_client.logout()
            .context("Erreur lors de la déconnexion IMAP")?;
        
        info!("Traitement terminé: {} emails traités, {} lectures de température sauvegardées", 
              total_processed, total_readings_saved);
        
        Ok(total_processed)
    }
    
    pub async fn process_emails_dry_run(&self, limit: Option<usize>) -> Result<usize> {
        println!("\n{}", "=".repeat(80));
        println!("🧪 MODE DRY-RUN - ANALYSE DES EMAILS X-SENSE");
        println!("{}", "=".repeat(80));
        
        // 1. Se connecter au serveur IMAP
        let mut imap_client = ImapClient::new(&self.config.imap).await
            .context("Impossible de se connecter au serveur IMAP")?;
        
        // 2. Rechercher les emails de support@x-sense.com
        let message_ids = imap_client.search_xsense_emails()
            .context("Erreur lors de la recherche d'emails")?;
        
        if message_ids.is_empty() {
            println!("❌ Aucun email trouvé correspondant aux critères");
            println!("   Critères: De 'support@x-sense.com' avec objet commençant par 'Votre exportation de'");
            return Ok(0);
        }
        
        println!("✅ Trouvé {} email(s) correspondant aux critères\n", message_ids.len());
        
        let mut total_processed = 0;
        
        // 3. Analyser chaque email trouvé (avec limite optionnelle)
        let emails_to_process: Vec<_> = if let Some(limit) = limit {
            message_ids.iter().take(limit).collect()
        } else {
            message_ids.iter().collect()
        };
        
        for (index, message_id) in emails_to_process.iter().enumerate() {
            println!("📧 Email {}/{} (ID: {})", index + 1, emails_to_process.len(), message_id);
            println!("{}", "-".repeat(60));
            
            match self.dry_run_process_single_email(&mut imap_client, **message_id).await {
                Ok(_) => {
                    total_processed += 1;
                    println!("✅ Email {} analysé avec succès\n", message_id);
                }
                Err(e) => {
                    println!("❌ Erreur lors de l'analyse de l'email {}: {}\n", message_id, e);
                }
            }
        }
        
        // 4. Se déconnecter proprement
        imap_client.logout()
            .context("Erreur lors de la déconnexion IMAP")?;
        
        println!("{}", "=".repeat(80));
        println!("🏁 Analyse terminée: {} emails analysés sur {}", total_processed, message_ids.len());
        println!("📁 Pièces jointes sauvegardées dans: {}", self.config.data_dir);
        println!("{}", "=".repeat(80));
        
        Ok(total_processed)
    }
    
    async fn dry_run_process_single_email(
        &self, 
        imap_client: &mut ImapClient, 
        message_id: u32
    ) -> Result<()> {
        // 1. Récupérer les headers de l'email
        let headers = imap_client.fetch_email_headers(message_id)
            .context("Impossible de récupérer les headers de l'email")?;
        
        println!("📋 Headers:");
        println!("{}", headers);
        println!();
        
        // 2. Récupérer le contenu de l'email
        let email_content = imap_client.fetch_email(message_id)
            .context("Impossible de récupérer l'email")?;
        
        // 3. Afficher des informations sur l'email
        println!("📄 Contenu de l'email:");
        println!("   Taille: {} bytes", email_content.len());
        
        // Essayer d'afficher un aperçu du contenu textuel
        if let Ok(content_str) = std::str::from_utf8(&email_content) {
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
        
        // 4. Extraire et analyser les pièces jointes
        let attachments = AttachmentParser::parse_email(&email_content)
            .context("Erreur lors de l'extraction des pièces jointes")?;
        
        if attachments.is_empty() {
            println!("📎 Aucune pièce jointe trouvée");
        } else {
            println!("📎 Pièces jointes trouvées: {}", attachments.len());
            println!();
            
            for attachment in attachments {
                // Afficher les informations de la pièce jointe
                AttachmentParser::display_attachment_info(&attachment);
                
                // Sauvegarder dans le répertoire data
                match AttachmentParser::save_attachment_to_data_dir(&attachment, &self.config.data_dir) {
                    Ok(path) => {
                        println!("💾 Sauvegardé dans: {:?}", path);
                    }
                    Err(e) => {
                        println!("❌ Erreur de sauvegarde: {}", e);
                    }
                }
                println!();
            }
        }
        
        Ok(())
    }
    
    async fn process_single_email(
        &self, 
        imap_client: &mut ImapClient, 
        message_id: u32
    ) -> Result<usize> {
        debug!("Traitement de l'email ID: {}", message_id);
        
        // 1. Récupérer le contenu de l'email
        let email_content = imap_client.fetch_email(message_id)
            .context("Impossible de récupérer l'email")?;
        
        // 2. Extraire les pièces jointes
        let attachments = AttachmentParser::parse_email(&email_content)
            .context("Erreur lors de l'extraction des pièces jointes")?;
        
        if attachments.is_empty() {
            warn!("Aucune pièce jointe trouvée dans l'email {}", message_id);
            return Ok(0);
        }
        
        let mut total_readings = 0;
        
        // 3. Traiter chaque pièce jointe
        for attachment in attachments {
            match self.process_attachment(&attachment).await {
                Ok(readings_count) => {
                    total_readings += readings_count;
                    info!("Pièce jointe '{}' traitée: {} lectures", 
                          attachment.filename, readings_count);
                }
                Err(e) => {
                    error!("Erreur lors du traitement de la pièce jointe '{}': {}", 
                           attachment.filename, e);
                    // Continuer avec les autres pièces jointes
                }
            }
        }
        
        Ok(total_readings)
    }
    
    async fn process_attachment(&self, attachment: &Attachment) -> Result<usize> {
        debug!("Traitement de la pièce jointe: {}", attachment.filename);
        
        // 1. Extraire les données de température de la pièce jointe
        let temperature_readings = TemperatureExtractor::extract_from_attachment(attachment)
            .context("Erreur lors de l'extraction des données de température")?;
        
        if temperature_readings.is_empty() {
            warn!("Aucune donnée de température trouvée dans '{}'", attachment.filename);
            return Ok(0);
        }
        
        // 2. Sauvegarder les lectures dans la base de données (si disponible)
        let saved_count = if let Some(ref database) = self.database {
            database.save_temperature_readings(&temperature_readings).await
                .context("Erreur lors de la sauvegarde en base de données")?
        } else {
            // Mode dry-run : pas de sauvegarde
            debug!("Mode dry-run : {} lectures ignorées (pas de sauvegarde)", temperature_readings.len());
            0
        };
        
        debug!("Pièce jointe '{}' terminée: {} lectures extraites, {} sauvegardées", 
               attachment.filename, temperature_readings.len(), saved_count);
        
        Ok(saved_count)
    }
    
    pub async fn get_recent_readings(&self, sensor_id: Option<&str>, limit: i64) -> Result<Vec<crate::temperature_extractor::TemperatureReading>> {
        if let Some(ref database) = self.database {
            database.get_latest_readings(sensor_id, limit).await
        } else {
            Ok(Vec::new()) // Mode dry-run : pas de données
        }
    }
    
    pub async fn close(self) -> Result<()> {
        info!("Fermeture du processeur d'emails");
        if let Some(database) = self.database {
            database.close().await
                .context("Erreur lors de la fermeture de la base de données")?;
        }
        Ok(())
    }
}