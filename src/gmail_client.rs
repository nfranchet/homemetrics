use anyhow::{Result, Context};
use google_gmail1::{Gmail, hyper, hyper_rustls, oauth2};
use log::{info, debug, warn};

use crate::config::GmailConfig;

pub struct EmailInfo {
    pub content: Vec<u8>,
    pub date: chrono::DateTime<chrono::Utc>,
    pub headers: String,
}

pub struct GmailClient {
    hub: Gmail<hyper_rustls::HttpsConnector<hyper::client::HttpConnector>>,
}

impl GmailClient {
    pub async fn new(config: &GmailConfig) -> Result<Self> {
        info!("Connecting to Gmail API via OAuth2");
        
        // Read OAuth2 client credentials from file
        let secret = oauth2::read_application_secret(&config.credentials_path)
            .await
            .context("Unable to read OAuth2 client credentials file")?;
        
        // Create authenticator with token persistence
        // Note: We use Scope::Modify on all API calls, which is the broadest scope available
        // in google-gmail1 (covers reading, modifying labels, and managing emails)
        let auth = oauth2::InstalledFlowAuthenticator::builder(
            secret,
            oauth2::InstalledFlowReturnMethod::HTTPRedirect,
        )
        .persist_tokens_to_disk(&config.token_cache_path)
        .build()
        .await
        .context("Unable to create OAuth2 authenticator")?;
        
        // Create HTTP client
        let connector = hyper_rustls::HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_or_http()
            .enable_http1()
            .build();
        
        let client = hyper::Client::builder().build(connector);
        
        // Create Gmail hub with appropriate scopes
        let hub = Gmail::new(client, auth);
        
        info!("✅ Gmail API connection established successfully");
        
        Ok(GmailClient { hub })
    }
    
    pub async fn search_xsense_emails(&self) -> Result<Vec<String>> {
        info!("Searching for emails with label 'homemetrics/todo/xsense'");
        
        let user_id = "me";
        let query = "label:homemetrics/todo/xsense";
        
        debug!("Search criteria: {}", query);
        
        let result = self.hub
            .users()
            .messages_list(user_id)
            .q(query)
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await
            .context("Error searching for emails")?;
        
        let message_ids: Vec<String> = result.1
            .messages
            .unwrap_or_default()
            .into_iter()
            .filter_map(|msg| msg.id)
            .collect();
        
        info!("Found {} email(s) with label 'homemetrics/todo/xsense'", message_ids.len());
        
        Ok(message_ids)
    }
    
    /// List all Gmail labels with their IDs and names
    pub async fn list_labels(&self) -> Result<()> {
        info!("Retrieving Gmail labels list");
        
        let user_id = "me";
        
        let result = self.hub
            .users()
            .labels_list(user_id)
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await
            .context("Unable to list labels")?;
        
        let labels = result.1.labels.unwrap_or_default();
        
        if labels.is_empty() {
            println!("No labels found.");
            return Ok(());
        }
        
        println!("Found {} label(s):\n", labels.len());
        println!("{:<40} {:<30} {:<15}", "Label Name", "Label ID", "Type");
        println!("{}", "=".repeat(85));
        
        // Sort labels: homemetrics labels first, then system, then user
        let mut sorted_labels = labels;
        sorted_labels.sort_by(|a, b| {
            let a_name = a.name.as_deref().unwrap_or("");
            let b_name = b.name.as_deref().unwrap_or("");
            
            let a_is_homemetrics = a_name.starts_with("homemetrics");
            let b_is_homemetrics = b_name.starts_with("homemetrics");
            
            match (a_is_homemetrics, b_is_homemetrics) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a_name.cmp(b_name),
            }
        });
        
        for label in sorted_labels {
            let name = label.name.unwrap_or_else(|| "Unknown".to_string());
            let id = label.id.unwrap_or_else(|| "Unknown".to_string());
            let label_type = label.type_.unwrap_or_else(|| "Unknown".to_string());
            
            // Highlight homemetrics labels
            if name.starts_with("homemetrics") {
                println!("✨ {:<38} {:<30} {:<15}", name, id, label_type);
            } else {
                println!("{:<40} {:<30} {:<15}", name, id, label_type);
            }
        }
        
        Ok(())
    }
    
    /// Retrieve only email metadata (subject and sender)
    pub async fn fetch_email_metadata(&self, message_id: &str) -> Result<(String, String)> {
        debug!("Retrieving email metadata for ID: {}", message_id);
        
        let user_id = "me";
        
        // Retrieve only headers with METADATA format
        let result = self.hub
            .users()
            .messages_get(user_id, message_id)
            .format("metadata")
            .add_metadata_headers("From")
            .add_metadata_headers("Subject")
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await
            .context("Unable to retrieve email metadata")?;
        
        let message = result.1;
        
        // Extract subject and sender from headers
        let mut subject = String::from("No subject");
        let mut from = String::from("Unknown sender");
        
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
        debug!("Complete email retrieval for ID: {}", message_id);
        
        let user_id = "me";
        
        // Retrieve complete message with RAW format
        let result = self.hub
            .users()
            .messages_get(user_id, message_id)
            .format("raw")
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await;
        
        let message = match result {
            Ok((_, msg)) => msg,
            Err(e) => {
                warn!("Error retrieving in RAW format: {}", e);
                return Err(anyhow::anyhow!("Unable to retrieve email: {}", e));
            }
        };
        
        // Raw content is already decoded by Gmail API (RFC822 format)
        let raw_content = message.raw
            .context("No raw content in email")?;
        
        debug!("Email retrieved, size: {} bytes", raw_content.len());
        
        // raw_content is already the raw RFC822 content (not base64)
        let raw_bytes = raw_content;
        
        debug!("Email retrieved, size: {} bytes", raw_bytes.len());
        
        // Parser le contenu avec mail-parser
        let email_str = String::from_utf8_lossy(&raw_bytes);
        let parsed_email = mail_parser::MessageParser::default()
            .parse(email_str.as_bytes())
            .context("Unable to parse email")?;
        
        // Extraire la date
        let email_date = if let Some(date_header) = parsed_email.date() {
            chrono::DateTime::from_timestamp(date_header.to_timestamp(), 0)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or_else(chrono::Utc::now)
        } else {
            warn!("No date in email, using current date");
            chrono::Utc::now()
        };
        
        // Extraire les headers principaux
        let from = parsed_email.from()
            .and_then(|addrs| addrs.first())
            .map(|addr| {
                match (&addr.name, &addr.address) {
                    (Some(name), Some(email)) => format!("{} <{}>", name, email),
                    (None, Some(email)) => email.to_string(),
                    _ => "Unknown sender".to_string(),
                }
            })
            .unwrap_or_else(|| "Unknown sender".to_string());
        
        let subject = parsed_email.subject()
            .unwrap_or("No subject")
            .to_string();
        
        let headers = format!("De: {}\nObjet: {}", from, subject);
        
        Ok(EmailInfo {
            content: raw_bytes,
            date: email_date,
            headers,
        })
    }
    
    pub async fn mark_email_as_processed(&self, message_id: &str) -> Result<()> {
        info!("Marking email {} as processed", message_id);
        
        let user_id = "me";
        
        // First, retrieve existing labels to get IDs
        let labels_result = self.hub
            .users()
            .labels_list(user_id)
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await
            .context("Unable to list labels")?;
        
        let labels = labels_result.1.labels.unwrap_or_default();
        
        // Trouver les IDs des labels
        let todo_label_id = labels.iter()
            .find(|l| l.name.as_deref() == Some("homemetrics/todo/xsense"))
            .and_then(|l| l.id.clone());
        
        let done_label_id = labels.iter()
            .find(|l| l.name.as_deref() == Some("homemetrics/done/xsense"))
            .and_then(|l| l.id.clone());
        
        // Create modification request
        let mut modify_request = google_gmail1::api::ModifyMessageRequest::default();
        
        // Supprimer le label "todo"
        if let Some(todo_id) = todo_label_id {
            modify_request.remove_label_ids = Some(vec![todo_id]);
            debug!("Removing label 'homemetrics/todo/xsense'");
        } else {
            warn!("Label 'homemetrics/todo/xsense' not found");
        }
        
        // Ajouter le label "done"
        if let Some(done_id) = done_label_id {
            modify_request.add_label_ids = Some(vec![done_id]);
            debug!("Adding label 'homemetrics/done/xsense'");
        } else {
            warn!("Label 'homemetrics/done/xsense' not found, it will need to be created in Gmail");
        }
        
        // Apply modifications
        self.hub
            .users()
            .messages_modify(modify_request, user_id, message_id)
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await
            .context("Unable to modify email labels")?;
        
        info!("✅ Email {} marked as processed with label 'homemetrics/done/xsense'", message_id);
        Ok(())
    }
    
    // ============================================================================
    // Blue Riot Pool Monitoring Methods
    // ============================================================================
    
    pub async fn search_pool_emails(&self) -> Result<Vec<String>> {
        info!("Searching for emails with label 'homemetrics/todo/blueriot'");
        
        let user_id = "me";
        let query = "label:homemetrics/todo/blueriot";
        
        debug!("Search criteria: {}", query);
        
        let result = self.hub
            .users()
            .messages_list(user_id)
            .q(query)
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await
            .context("Error searching for pool emails")?;
        
        let message_ids: Vec<String> = result.1
            .messages
            .unwrap_or_default()
            .into_iter()
            .filter_map(|msg| msg.id)
            .collect();
        
        info!("Found {} email(s) with label 'homemetrics/todo/blueriot'", message_ids.len());
        
        Ok(message_ids)
    }
    
    pub async fn mark_pool_email_as_processed(&self, message_id: &str) -> Result<()> {
        info!("Marking pool email {} as processed", message_id);
        
        let user_id = "me";
        
        // First, retrieve existing labels to get IDs
        let labels_result = self.hub
            .users()
            .labels_list(user_id)
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await
            .context("Unable to list labels")?;
        
        let labels = labels_result.1.labels.unwrap_or_default();
        
        // Find label IDs for Blue Riot
        let todo_label_id = labels.iter()
            .find(|l| l.name.as_deref() == Some("homemetrics/todo/blueriot"))
            .and_then(|l| l.id.clone());
        
        let done_label_id = labels.iter()
            .find(|l| l.name.as_deref() == Some("homemetrics/done/blueriot"))
            .and_then(|l| l.id.clone());
        
        let inbox_label_id = labels.iter()
            .find(|l| l.name.as_deref() == Some("INBOX"))
            .and_then(|l| l.id.clone());
        
        let unread_label_id = labels.iter()
            .find(|l| l.name.as_deref() == Some("UNREAD"))
            .and_then(|l| l.id.clone());
        
        // Create modification request
        let mut modify_request = google_gmail1::api::ModifyMessageRequest::default();
        let mut remove_labels = Vec::new();
        let mut add_labels = Vec::new();
        
        // Remove "todo" label
        if let Some(todo_id) = todo_label_id {
            remove_labels.push(todo_id);
            debug!("Removing label 'homemetrics/todo/blueriot'");
        } else {
            warn!("Label 'homemetrics/todo/blueriot' not found");
        }
        
        // Remove INBOX
        if let Some(inbox_id) = inbox_label_id {
            remove_labels.push(inbox_id);
            debug!("Removing from INBOX");
        }
        
        // Remove UNREAD (mark as read)
        if let Some(unread_id) = unread_label_id {
            remove_labels.push(unread_id);
            debug!("Marking as read");
        }
        
        // Add "done" label
        if let Some(done_id) = done_label_id {
            add_labels.push(done_id);
            debug!("Adding label 'homemetrics/done/blueriot'");
        } else {
            warn!("Label 'homemetrics/done/blueriot' not found, it will need to be created in Gmail");
        }
        
        modify_request.remove_label_ids = Some(remove_labels);
        modify_request.add_label_ids = Some(add_labels);
        
        // Apply modifications
        self.hub
            .users()
            .messages_modify(modify_request, user_id, message_id)
            .add_scope(google_gmail1::api::Scope::Modify)
            .doit()
            .await
            .context("Unable to modify pool email labels")?;
        
        info!("✅ Pool email {} marked as processed (read, archived, labeled 'done')", message_id);
        Ok(())
    }
}
