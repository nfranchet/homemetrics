use anyhow::{Result, Context};
use log::{info, debug};
use std::path::{PathBuf, Path};
use std::fs;
use chrono::Utc;

#[derive(Debug)]
pub struct Attachment {
    pub filename: String,
    pub content: Vec<u8>,
    pub content_type: String,
}

pub struct AttachmentParser;

impl AttachmentParser {
    pub fn parse_email(raw_email: &[u8]) -> Result<Vec<Attachment>> {
        debug!("Parsing email pour extraire les pièces jointes");
        
        // Pour l'instant, simulation basique - en production vous utiliseriez
        // un parser MIME plus sophistiqué ou mail-parser avec la bonne API
        let email_str = String::from_utf8_lossy(raw_email);
        
        let mut attachments = Vec::new();
        
        // Recherche basique de patterns d'attachments dans l'email
        // Cette implémentation simplifiée sera remplacée par un vrai parser MIME
        if email_str.contains("Content-Disposition: attachment") {
            // Simuler des pièces jointes pour les tests
            let test_attachment = Attachment {
                filename: "temperature_data.csv".to_string(),
                content: Self::create_sample_csv_data(),
                content_type: "text/csv".to_string(),
            };
            attachments.push(test_attachment);
            info!("Pièce jointe de test ajoutée pour démonstration");
        }
        
        info!("Trouvé {} pièce(s) jointe(s)", attachments.len());
        Ok(attachments)
    }
    
    // Génère des données CSV de test pour la démonstration
    fn create_sample_csv_data() -> Vec<u8> {
        let csv_content = r#"timestamp,sensor_id,temperature,humidity,location
2024-10-16 10:00:00,SENSOR001,22.5,45.2,Living Room
2024-10-16 10:15:00,SENSOR001,22.7,44.8,Living Room
2024-10-16 10:30:00,SENSOR002,21.2,50.1,Bedroom
2024-10-16 10:45:00,SENSOR002,21.0,51.3,Bedroom
2024-10-16 11:00:00,SENSOR003,23.1,42.5,Kitchen
"#;
        csv_content.as_bytes().to_vec()
    }
    
    fn is_data_file(filename: &str) -> bool {
        let lowercase_name = filename.to_lowercase();
        lowercase_name.ends_with(".csv") ||
        lowercase_name.ends_with(".json") ||
        lowercase_name.ends_with(".xml") ||
        lowercase_name.ends_with(".txt") ||
        lowercase_name.ends_with(".xlsx") ||
        lowercase_name.ends_with(".xls")
    }
    
    pub fn save_attachment_to_data_dir(attachment: &Attachment, data_dir: &str) -> Result<PathBuf> {
        // Créer le répertoire data s'il n'existe pas
        fs::create_dir_all(data_dir)
            .context("Impossible de créer le répertoire data")?;
        
        // Générer un nom de fichier avec préfixe de date
        let now = Utc::now();
        let date_prefix = now.format("%Y%m%d_%H%M%S");
        let filename = format!("{}_{}", date_prefix, attachment.filename);
        let file_path = PathBuf::from(data_dir).join(&filename);
        
        // Sauvegarder le contenu
        fs::write(&file_path, &attachment.content)
            .context("Impossible d'écrire la pièce jointe")?;
        
        info!("Pièce jointe sauvegardée: {:?}", file_path);
        
        Ok(file_path)
    }
    
    pub fn display_attachment_info(attachment: &Attachment) {
        println!("📎 Pièce jointe: {}", attachment.filename);
        println!("   Type: {}", attachment.content_type);
        println!("   Taille: {} bytes", attachment.content.len());
        
        // Afficher un aperçu du contenu si c'est du texte
        if attachment.content_type.starts_with("text/") || 
           attachment.filename.to_lowercase().ends_with(".csv") ||
           attachment.filename.to_lowercase().ends_with(".json") ||
           attachment.filename.to_lowercase().ends_with(".txt") {
            
            if let Ok(content_str) = std::str::from_utf8(&attachment.content) {
                let preview = if content_str.len() > 500 {
                    format!("{}...", &content_str[..500])
                } else {
                    content_str.to_string()
                };
                println!("   Aperçu:\n{}", preview);
            }
        }
        println!("   {}", "─".repeat(80));
    }
}