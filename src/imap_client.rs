use anyhow::{Result, Context};
use imap::Session;
use native_tls::{TlsConnector, TlsStream};
use std::net::TcpStream;
use log::{info, debug};

use crate::config::ImapConfig;

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
    
    pub fn fetch_email(&mut self, message_id: u32) -> Result<Vec<u8>> {
        debug!("Récupération de l'email ID: {}", message_id);
        
        let messages = self.session
            .fetch(message_id.to_string(), "RFC822")
            .context("Impossible de récupérer l'email")?;
        
        if let Some(message) = messages.iter().next() {
            if let Some(body) = message.body() {
                debug!("Email récupéré, taille: {} bytes", body.len());
                return Ok(body.to_vec());
            }
        }
        
        anyhow::bail!("Email introuvable ou vide pour l'ID: {}", message_id);
    }
    
    pub fn fetch_email_headers(&mut self, message_id: u32) -> Result<String> {
        debug!("Récupération des headers de l'email ID: {}", message_id);
        
        let messages = self.session
            .fetch(message_id.to_string(), "ENVELOPE")
            .context("Impossible de récupérer les headers de l'email")?;
        
        if let Some(message) = messages.iter().next() {
            if let Some(envelope) = message.envelope() {
                let subject = envelope.subject.as_ref()
                    .map(|s| String::from_utf8_lossy(s).to_string())
                    .unwrap_or_else(|| "Sans objet".to_string());
                
                let from = envelope.from.as_ref()
                    .and_then(|addresses| addresses.first())
                    .map(|addr| {
                        let mailbox = addr.mailbox.as_ref()
                            .map(|m| String::from_utf8_lossy(m)).unwrap_or_default();
                        let host = addr.host.as_ref()
                            .map(|h| String::from_utf8_lossy(h)).unwrap_or_default();
                        format!("{}@{}", mailbox, host)
                    })
                    .unwrap_or_else(|| "Expéditeur inconnu".to_string());
                
                return Ok(format!("De: {}\nObjet: {}", from, subject));
            }
        }
        
        Ok("Headers non disponibles".to_string())
    }
    
    pub fn logout(mut self) -> Result<()> {
        info!("Déconnexion du serveur IMAP");
        self.session.logout()
            .context("Erreur lors de la déconnexion IMAP")?;
        Ok(())
    }
}