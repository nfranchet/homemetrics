use anyhow::{Result, Context};
use google_gmail1::{Gmail, hyper, hyper_rustls, oauth2};
use log::{info, debug, warn};

use crate::config::GmailConfig;

pub struct EmailInfo {
    pub subject: String,
    pub content: Vec<u8>,
    pub date: chrono::DateTime<chrono::Utc>,
    pub headers: String,
    pub id: String,
}

pub struct GmailClient {
    hub: Gmail<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
}

impl GmailClient {
    pub async fn new(config: &GmailConfig) -> Result<Self> {
        info!("Connexion à l'API Gmail via OAuth2");
        
        // Lire les credentials OAuth2 client depuis le fichier
        let secret = oauth2::read_application_secret(&config.credentials_path)
            .await
            .context("Impossible de lire le fichier de credentials client OAuth2")?;
        
        // Créer l'authenticator avec persistance du token et les scopes nécessaires
        let auth = oauth2::InstalledFlowAuthenticator::builder(
            secret,
            oauth2::InstalledFlowReturnMethod::HTTPRedirect,
        )
        .persist_tokens_to_disk(&config.token_cache_path)
        .build()
        .await
        .context("Impossible de créer l'authenticator OAuth2")?;
        
        // Créer le client HTTP
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http1()
            .build();
        
        let client = hyper::Client::builder().build(connector);
        
        // Créer le hub Gmail avec les scopes appropriés
        let hub = Gmail::new(client, auth);
        
        info!("✅ Connexion à l'API Gmail établie avec succès");
        
        Ok(GmailClient { hub })
    }
    
    pub async fn search_xsense_emails(&self) -> Result<Vec<String>> {
        info!("Recherche des emails avec le label 'homemetrics-todo-xsense'");
        
        let user_id = "me";
        let query = "label:homemetrics-todo-xsense";
        
        debug!("Critères de recherche: {}", query);
        
        let result = self.hub
            .users()
            .messages_list(user_id)
            .q(query)
            .add_scope(google_gmail1::api::Scope::Readonly)
            .doit()
            .await
            .context("Erreur lors de la recherche d'emails")?;
        
        let message_ids: Vec<String> = result.1
            .messages
            .unwrap_or_default()
            .into_iter()
            .filter_map(|msg| msg.id)
            .collect();
        
        info!("Trouvé {} email(s) avec le label 'homemetrics-todo-xsense'", message_ids.len());
        
        Ok(message_ids)
    }
    
    /// Récupère uniquement les métadonnées de l'email (sujet et expéditeur)
    pub async fn fetch_email_metadata(&self, message_id: &str) -> Result<(String, String)> {
        debug!("Récupération des métadonnées de l'email ID: {}", message_id);
        
        let user_id = "me";
        
        // Récupérer uniquement les headers avec le format METADATA
        let result = self.hub
            .users()
            .messages_get(user_id, message_id)
            .format("metadata")
            .add_metadata_headers("From")
            .add_metadata_headers("Subject")
            .add_scope(google_gmail1::api::Scope::Readonly)
            .doit()
            .await
            .context("Impossible de récupérer les métadonnées de l'email")?;
        
        let message = result.1;
        
        // Extraire le sujet et l'expéditeur des headers
        let mut subject = String::from("Sans objet");
        let mut from = String::from("Expéditeur inconnu");
        
        if let Some(payload) = message.payload {
            if let Some(headers) = payload.headers {
                for header in headers {
                    if let (Some(name), Some(value)) = (header.name, header.value) {
                        match name.as_str() {
                            "Subject" => subject = value,
                            "From" => from = value,
                            _ => {}
                        }
                    }
                }
            }
        }
        
        Ok((subject, from))
    }
    
    pub async fn fetch_email_complete(&self, message_id: &str) -> Result<EmailInfo> {
        debug!("Récupération complète de l'email ID: {}", message_id);
        
        let user_id = "me";
        
        // Récupérer le message complet avec le format RAW
        let result = self.hub
            .users()
            .messages_get(user_id, message_id)
            .format("raw")
            .add_scope(google_gmail1::api::Scope::Readonly)
            .doit()
            .await;
        
        let message = match result {
            Ok((_, msg)) => msg,
            Err(e) => {
                warn!("Erreur lors de la récupération en format RAW: {}", e);
                return Err(anyhow::anyhow!("Impossible de récupérer l'email: {}", e));
            }
        };
        
        // Le contenu raw est déjà décodé par Gmail API (format RFC822)
        let raw_content = message.raw
            .context("Pas de contenu raw dans l'email")?;
        
        debug!("Email récupéré, taille: {} bytes", raw_content.len());
        
        // raw_content est déjà le contenu RFC822 brut (pas en base64)
        let raw_bytes = raw_content;
        
        debug!("Email récupéré, taille: {} bytes", raw_bytes.len());
        
        // Parser le contenu avec mail-parser
        let email_str = String::from_utf8_lossy(&raw_bytes);
        let parsed_email = mail_parser::MessageParser::default()
            .parse(email_str.as_bytes())
            .context("Impossible de parser l'email")?;
        
        // Extraire la date
        let email_date = if let Some(date_header) = parsed_email.date() {
            chrono::DateTime::from_timestamp(date_header.to_timestamp(), 0)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(|| chrono::Utc::now())
        } else {
            warn!("Pas de date dans l'email, utilisation de la date actuelle");
            chrono::Utc::now()
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
            .unwrap_or("Sans objet")
            .to_string();
        
        let headers = format!("De: {}\nObjet: {}", from, subject);
        
        Ok(EmailInfo {
            subject,
            content: raw_bytes,
            date: email_date,
            headers,
            id: message_id.to_string(),
        })
    }
    
    pub async fn mark_email_as_processed(&self, message_id: &str) -> Result<()> {
        info!("Marquage de l'email {} comme traité", message_id);
        
        let user_id = "me";
        
        // D'abord, récupérer les labels existants pour obtenir les IDs
        let labels_result = self.hub
            .users()
            .labels_list(user_id)
            .add_scope(google_gmail1::api::Scope::Readonly)
            .doit()
            .await
            .context("Impossible de lister les labels")?;
        
        let labels = labels_result.1.labels.unwrap_or_default();
        
        // Trouver les IDs des labels
        let todo_label_id = labels.iter()
            .find(|l| l.name.as_deref() == Some("homemetrics-todo-xsense"))
            .and_then(|l| l.id.clone());
        
        let done_label_id = labels.iter()
            .find(|l| l.name.as_deref() == Some("homemetrics-done-xsense"))
            .and_then(|l| l.id.clone());
        
        // Créer la requête de modification
        let mut modify_request = google_gmail1::api::ModifyMessageRequest::default();
        
        // Supprimer le label "todo"
        if let Some(todo_id) = todo_label_id {
            modify_request.remove_label_ids = Some(vec![todo_id]);
            debug!("Suppression du label 'homemetrics-todo-xsense'");
        } else {
            warn!("Label 'homemetrics-todo-xsense' non trouvé");
        }
        
        // Ajouter le label "done"
        if let Some(done_id) = done_label_id {
            modify_request.add_label_ids = Some(vec![done_id]);
            debug!("Ajout du label 'homemetrics-done-xsense'");
        } else {
            warn!("Label 'homemetrics-done-xsense' non trouvé, il faudra le créer dans Gmail");
        }
        
        // Appliquer les modifications
        self.hub
            .users()
            .messages_modify(modify_request, user_id, message_id)
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await
            .context("Impossible de modifier les labels de l'email")?;
        
        info!("✅ Email {} marqué comme traité avec le label 'homemetrics-done-xsense'", message_id);
        Ok(())
    }
}
