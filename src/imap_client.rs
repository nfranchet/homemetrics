use anyhow::{Result, Context};
use async_imap::Session;
use async_native_tls::{TlsConnector, TlsStream};
use tokio::net::TcpStream;
use tokio_util::compat::TokioAsyncReadCompatExt;
use futures::stream::StreamExt;
use log::{info, debug, warn};

use crate::config::ImapConfig;

pub struct EmailInfo {
    pub subject: String,
    pub content: Vec<u8>,
    pub date: chrono::DateTime<chrono::Utc>,
    pub headers: String,
}

pub struct ImapClient {
    session: Session<TlsStream<tokio_util::compat::Compat<TcpStream>>>,
}

impl ImapClient {
    pub async fn new(config: &ImapConfig) -> Result<Self> {
        info!("Connexion au serveur IMAP {}:{}", config.server, config.port);
        
        // Créer une connexion TCP
        let tcp_stream = TcpStream::connect((config.server.as_str(), config.port))
            .await
            .context("Impossible de se connecter au serveur IMAP")?;
        
        // Wrapper pour compatibilité futures
        let tcp_stream_compat = tcp_stream.compat();
        
        // Créer une connexion TLS
        let tls = TlsConnector::new();
        let tls_stream = tls.connect(&config.server, tcp_stream_compat)
            .await
            .context("Impossible d'établir la connexion TLS")?;
        
        // Créer le client IMAP avec async-imap
        let client = async_imap::Client::new(tls_stream);
        
        // Authentification
        let session = client
            .login(&config.username, &config.password)
            .await
            .map_err(|e| anyhow::anyhow!("Erreur d'authentification IMAP: {:?}", e.0))?;
        
        info!("Connexion IMAP établie avec succès");
        
        Ok(ImapClient { session })
    }
    
    pub async fn search_xsense_emails(&mut self) -> Result<Vec<u32>> {
        info!("Recherche des emails avec le label 'homemetrics-todo-xsense'");
        
        // Sélectionner la boîte aux lettres
        self.session.select("INBOX")
            .await
            .context("Impossible de sélectionner INBOX")?;
        
        // Utiliser X-GM-RAW pour rechercher par label Gmail
        // Format: X-GM-RAW "label:nom-du-label"
        let search_criteria = "X-GM-RAW \"label:homemetrics-todo-xsense\"";

        debug!("Critères de recherche: {}", search_criteria);
        
        let message_ids = self.session
            .search(&search_criteria)
            .await
            .context("Erreur lors de la recherche d'emails par label")?;
        
        let ids_vec: Vec<u32> = message_ids.into_iter().collect();
        info!("Trouvé {} email(s) avec le label 'homemetrics-todo-xsense'", ids_vec.len());
        
        Ok(ids_vec)
    }
    
    /// Vérifie rapidement si un email provient bien de l'expéditeur spécifié
    async fn verify_email_sender(&mut self, message_id: u32, expected_sender: &str) -> Result<bool> {
        // Récupérer uniquement l'envelope (plus rapide que RFC822 complet)
        let messages_stream = self.session
            .fetch(message_id.to_string(), "ENVELOPE")
            .await
            .context("Impossible de récupérer l'envelope")?;
        
        let messages: Vec<_> = messages_stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();
        
        if let Some(message) = messages.first() {
            if let Some(envelope) = message.envelope() {
                if let Some(from_addresses) = envelope.from.as_ref() {
                    for addr in from_addresses {
                        let mailbox = addr.mailbox.as_ref()
                            .map(|m| String::from_utf8_lossy(m).to_string())
                            .unwrap_or_default();
                        let host = addr.host.as_ref()
                            .map(|h| String::from_utf8_lossy(h).to_string())
                            .unwrap_or_default();
                        
                        let email = format!("{}@{}", mailbox, host);
                        
                        if email.to_lowercase() == expected_sender.to_lowercase() {
                            return Ok(true);
                        }
                    }
                }
            }
        }
        
        Ok(false)
    }
    
    pub async fn fetch_email_complete(&mut self, message_id: u32) -> Result<EmailInfo> {
        debug!("Récupération complète de l'email ID: {}", message_id);
        
        // Un seul fetch pour récupérer tout le contenu de l'email
        let messages_stream = self.session
            .fetch(message_id.to_string(), "RFC822")
            .await
            .context("Impossible de récupérer l'email")?;
        
        // Collecter le stream en vec
        let messages: Vec<_> = messages_stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();
        
        if let Some(message) = messages.first() {
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
                            subject: subject.to_string(),
                            content: body.to_vec(),
                            date: email_date,
                            headers,
                        });
                    }
                }
                
                // Fallback : utiliser l'ancienne méthode si le parsing échoue
                warn!("Impossible de parser l'email avec mail-parser, utilisation du fallback");
                return Ok(EmailInfo {
                    subject: "Objet inconnu".to_string(),
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
    
    /// Crée le répertoire cible s'il n'existe pas
    pub async fn ensure_folder_exists(&mut self, folder_name: &str) -> Result<()> {
        debug!("Vérification de l'existence du répertoire: {}", folder_name);
        
        // Lister les mailboxes pour vérifier si le dossier existe
        let mailboxes_stream = self.session.list(None, Some(folder_name))
            .await
            .context("Impossible de lister les mailboxes")?;
        
        // Collecter le stream en vec
        let mailboxes: Vec<_> = mailboxes_stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();
        
        if mailboxes.is_empty() {
            info!("Création du répertoire IMAP: {}", folder_name);
            self.session.create(folder_name)
                .await
                .context(format!("Impossible de créer le répertoire {}", folder_name))?;
            info!("✅ Répertoire créé: {}", folder_name);
        } else {
            debug!("Répertoire existant: {}", folder_name);
        }
        
        Ok(())
    }
    
    /// Déplace un email vers un répertoire spécifique
    pub async fn move_email_to_folder(&mut self, message_id: u32, target_folder: &str) -> Result<()> {
        info!("Déplacement de l'email {} vers {}", message_id, target_folder);
        
        // S'assurer que nous sommes dans INBOX
        self.session.select("INBOX")
            .await
            .context("Impossible de sélectionner INBOX")?;
        
        // Copier l'email vers le dossier cible
        self.session.copy(&message_id.to_string(), target_folder)
            .await
            .context(format!("Impossible de copier l'email vers {}", target_folder))?;
        
        // Marquer l'email comme supprimé dans INBOX
        let store_stream = self.session.store(format!("{}", message_id), "+FLAGS (\\Deleted)")
            .await
            .context("Impossible de marquer l'email comme supprimé")?;
        
        // Consommer le stream (nécessaire pour que l'opération soit effectuée)
        let _store_results: Vec<_> = store_stream.collect::<Vec<_>>().await;
        
        // Expunge pour supprimer définitivement les emails marqués
        //self.session.expunge()
        //    .await
        //    .context("Impossible d'expunge les emails supprimés")?;
        
        info!("✅ Email {} déplacé vers {}", message_id, target_folder);
        Ok(())
    }
    
    /// Ajoute le label "done" et supprime tous les autres labels Gmail
    /// Utilise X-GM-LABELS pour gérer les labels Gmail
    pub async fn mark_email_as_processed(&mut self, message_id: u32) -> Result<()> {
        info!("Marquage de l'email {} comme traité", message_id);
        
        // S'assurer que nous sommes dans INBOX
        self.session.select("INBOX")
            .await
            .context("Impossible de sélectionner INBOX")?;
        
        // Étape 1: Supprimer TOUS les labels existants
        // On utilise -X-GM-LABELS pour supprimer les labels
        debug!("Suppression de tous les labels de l'email {}", message_id);
        let store_stream = self.session
            .store(format!("{}", message_id), "-X-GM-LABELS (\\All)")
            .await
            .context("Impossible de supprimer les labels")?;
        
        // Consommer le stream
        let _results: Vec<_> = store_stream.collect::<Vec<_>>().await;
        
        // Étape 2: Ajouter uniquement le label "done"
        debug!("Ajout du label 'homemetrics-done-xsense' à l'email {}", message_id);
        let store_stream = self.session
            .store(format!("{}", message_id), "+X-GM-LABELS (homemetrics-done-xsense)")
            .await
            .context("Impossible d'ajouter le label 'homemetrics-done-xsense'")?;
        
        // Consommer le stream
        let _results: Vec<_> = store_stream.collect::<Vec<_>>().await;
        
        info!("✅ Email {} marqué comme traité avec le label 'homemetrics-done-xsense'", message_id);
        Ok(())
    }
    
    pub async fn logout(mut self) -> Result<()> {
        info!("Déconnexion du serveur IMAP");
        self.session.logout()
            .await
            .context("Erreur lors de la déconnexion IMAP")?;
        Ok(())
    }
}