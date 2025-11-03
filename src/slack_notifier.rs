use anyhow::{Result, Context};
use log::{info, error};
use slack_morphism::prelude::*;

use crate::config::SlackConfig;

pub struct SlackNotifier {
    client: SlackClient<SlackClientHyperHttpsConnector>,
    token: SlackApiToken,
    channel_id: SlackChannelId,
}

impl SlackNotifier {
    pub fn new(config: &SlackConfig) -> Result<Self> {
        info!("Initialisation du notifieur Slack");
        
        let client = SlackClient::new(SlackClientHyperHttpsConnector::new()?);
        let token = SlackApiToken::new(config.bot_token.clone().into());
        let channel_id = SlackChannelId::new(config.channel_id.clone());
        
        Ok(SlackNotifier {
            client,
            token,
            channel_id,
        })
    }
    
    /// Envoie une notification de succès pour un email traité
    pub async fn notify_email_processed(
        &self,
        email_id: &str,
        email_subject: &str,
        email_date: chrono::DateTime<chrono::Utc>,
        readings_count: usize,
        sensor_details: Vec<(String, usize)>,
    ) -> Result<()> {
        info!("Envoi de notification Slack pour l'email {}", email_id);
        
        // Construire le message avec formatage Slack
        let mut message_text = format!(
            "✅ *Email X-Sense traité avec succès*\n\n\
             • Email ID: `{}`\n\
             • Sujet: {}\n\
             • Date: {}\n\
             • Lectures sauvegardées: *{}*\n",
            email_id,
            email_subject,
            email_date.format("%Y-%m-%d %H:%M:%S UTC"),
            readings_count
        );
        
        if !sensor_details.is_empty() {
            message_text.push_str("• Sensors:\n");
            for (sensor, count) in sensor_details {
                message_text.push_str(&format!("  - {} ({} lectures)\n", sensor, count));
            }
        }
        
        // Créer la requête de message
        let post_chat_req = SlackApiChatPostMessageRequest::new(
            self.channel_id.clone(),
            SlackMessageContent::new().with_text(message_text),
        );
        
        // Créer une session avec le token
        let session = self.client.open_session(&self.token);
        
        // Envoyer le message
        match session.chat_post_message(&post_chat_req).await {
            Ok(response) => {
                info!("✅ Message Slack envoyé avec succès: {:?}", response.ts);
                Ok(())
            }
            Err(e) => {
                error!("❌ Erreur lors de l'envoi du message Slack: {}", e);
                Err(anyhow::anyhow!("Impossible d'envoyer le message Slack: {}", e))
            }
        }
    }
    
    /// Envoie une notification d'erreur
    pub async fn notify_error(&self, email_id: u32, error_message: &str) -> Result<()> {
        info!("Envoi de notification d'erreur Slack pour l'email {}", email_id);
        
        let message_text = format!(
            "❌ *Erreur lors du traitement de l'email X-Sense*\n\n\
             • Email ID: `{}`\n\
             • Erreur: ```{}```",
            email_id,
            error_message
        );
        
        let post_chat_req = SlackApiChatPostMessageRequest::new(
            self.channel_id.clone(),
            SlackMessageContent::new().with_text(message_text),
        );
        
        let session = self.client.open_session(&self.token);
        
        session.chat_post_message(&post_chat_req)
            .await
            .context("Impossible d'envoyer le message d'erreur Slack")?;
        
        Ok(())
    }
}
