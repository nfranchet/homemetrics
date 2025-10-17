use anyhow::{Result, Context};
use imap::Session;
use native_tls::{TlsConnector, TlsStream};
use std::net::TcpStream;
use log::{info, debug, warn};

use crate::config::ImapConfig;

pub struct EmailInfo {
    pub content: Vec<u8>,
    pub date: chrono::DateTime<chrono::Utc>,
    pub headers: String,
}

pub struct ImapClient {
    session: Session<TlsStream<TcpStream>>,
}

impl ImapClient {
    pub async fn new(config: &ImapConfig) -> Result<Self> {
        info!("Connexion au serveur IMAP {}:{}", config.server, config.port);
        
        // Créer une connexion TLS
        let tls = TlsConnector::builder()
            .build()
            .context("Impossible de créer le connecteur TLS")?;
        
        // Se connecter au serveur IMAP
        let client = imap::connect((config.server.as_str(), config.port), config.server.as_str(), &tls)
            .context("Impossible de se connecter au serveur IMAP")?;
        
        // Authentification
        let session = client
            .login(&config.username, &config.password)
            .map_err(|e| anyhow::anyhow!("Erreur d'authentification IMAP: {:?}", e.0))?;
        
        info!("Connexion IMAP établie avec succès");
        
        Ok(ImapClient { session })
    }
    
    pub fn search_xsense_emails(&mut self) -> Result<Vec<u32>> {
        info!("Recherche des emails de support@x-sense.com avec titre 'Votre exportation de'");
        
        // Sélectionner la boîte aux lettres
        self.session.select("INBOX")
            .context("Impossible de sélectionner INBOX")?;
        
        // Rechercher les emails de l'expéditeur spécifique avec le titre requis
        let search_criteria = format!(
            "FROM \"support@x-sense.com\" SUBJECT \"Votre exportation de\""
        );
        
        debug!("Critères de recherche: {}", search_criteria);
        
        let message_ids = self.session
            .search(&search_criteria)
            .context("Erreur lors de la recherche d'emails")?;
        
        let ids_vec: Vec<u32> = message_ids.into_iter().collect();
        info!("Trouvé {} email(s) correspondant aux critères", ids_vec.len());
        
        Ok(ids_vec)
    }
    
    pub fn fetch_email_complete(&mut self, message_id: u32) -> Result<EmailInfo> {
        debug!("Récupération complète de l'email ID: {}", message_id);
        
        // Un seul fetch pour récupérer tout le contenu de l'email
        let messages = self.session
            .fetch(message_id.to_string(), "RFC822")
            .context("Impossible de récupérer l'email")?;
        
        if let Some(message) = messages.iter().next() {
            if let Some(body) = message.body() {
                debug!("Email récupéré, taille: {} bytes", body.len());
                
                // Parse le contenu avec mail-parser pour extraire les infos
                if let Ok(email_str) = std::str::from_utf8(body) {
                    if let Some(parsed_email) = mail_parser::MessageParser::default().parse(email_str) {
                        // Extraire la date
                        let email_date = if let Some(date_header) = parsed_email.date() {
                            chrono::DateTime::from_timestamp(date_header.to_timestamp(), 0)
                                .map(|dt| dt.with_timezone(&chrono::Utc))
                                .unwrap_or_else(|| chrono::Utc::now())
                        } else {
                            // Fallback : essayer de parser depuis les headers raw
                            self.parse_date_from_raw_headers(email_str).unwrap_or_else(|| chrono::Utc::now())
                        };
                        
                        // Extraire les headers principaux
                        let from = parsed_email.from()
                            .and_then(|addrs| addrs.first())
                            .map(|addr| {
                                match (&addr.name, &addr.address) {
                                    (Some(name), Some(email)) => format!("{} <{}>", name, email),
                                    (None, Some(email)) => email.to_string(),
                                    _ => "Expéditeur inconnu".to_string(),
                                }
                            })
                            .unwrap_or_else(|| "Expéditeur inconnu".to_string());
                        
                        let subject = parsed_email.subject()
                            .unwrap_or("Sans objet");
                        
                        let headers = format!("De: {}\nObjet: {}", from, subject);
                        
                        return Ok(EmailInfo {
                            content: body.to_vec(),
                            date: email_date,
                            headers,
                        });
                    }
                }
                
                // Fallback : utiliser l'ancienne méthode si le parsing échoue
                warn!("Impossible de parser l'email avec mail-parser, utilisation du fallback");
                return Ok(EmailInfo {
                    content: body.to_vec(),
                    date: chrono::Utc::now(),
                    headers: "Headers non disponibles".to_string(),
                });
            }
        }
        
        anyhow::bail!("Email introuvable ou vide pour l'ID: {}", message_id);
    }
    
    fn parse_date_from_raw_headers(&self, email_content: &str) -> Option<chrono::DateTime<chrono::Utc>> {
        // Chercher la ligne Date: dans les headers
        for line in email_content.lines().take(50) { // Limiter aux premiers headers
            if line.is_empty() {
                break; // Fin des headers
            }
            
            if let Some(date_part) = line.strip_prefix("Date: ") {
                // Essayer de parser la date RFC 2822
                if let Ok(parsed_date) = chrono::DateTime::parse_from_rfc2822(date_part.trim()) {
                    return Some(parsed_date.with_timezone(&chrono::Utc));
                }
            }
        }
        None
    }
    

    
    pub fn logout(mut self) -> Result<()> {
        info!("Déconnexion du serveur IMAP");
        self.session.logout()
            .context("Erreur lors de la déconnexion IMAP")?;
        Ok(())
    }
}