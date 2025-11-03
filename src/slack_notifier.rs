use anyhow::Result;
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
        info!("Initializing Slack notifier");
        
        let client = SlackClient::new(SlackClientHyperHttpsConnector::new()?);
        let token = SlackApiToken::new(config.bot_token.clone().into());
        let channel_id = SlackChannelId::new(config.channel_id.clone());
        
        Ok(SlackNotifier {
            client,
            token,
            channel_id,
        })
    }
    
    /// Send a success notification for a processed email
    pub async fn notify_email_processed(
        &self,
        email_id: &str,
        email_subject: &str,
        email_date: chrono::DateTime<chrono::Utc>,
        readings_count: usize,
        sensor_details: Vec<(String, usize)>,
    ) -> Result<()> {
        info!("Sending Slack notification for email {}", email_id);
        
        // Build message with Slack formatting
        let mut message_text = format!(
            "✅ *X-Sense email processed successfully*\n\n\
             • Email ID: `{}`\n\
             • Subject: {}\n\
             • Date: {}\n\
             • Saved readings: *{}*\n",
            email_id,
            email_subject,
            email_date.format("%Y-%m-%d %H:%M:%S UTC"),
            readings_count
        );
        
        if !sensor_details.is_empty() {
            message_text.push_str("• Sensors:\n");
            for (sensor, count) in sensor_details {
                message_text.push_str(&format!("  - {} ({} readings)\n", sensor, count));
            }
        }
        
        // Create message request
        let post_chat_req = SlackApiChatPostMessageRequest::new(
            self.channel_id.clone(),
            SlackMessageContent::new().with_text(message_text),
        );
        
        // Create session with token
        let session = self.client.open_session(&self.token);
        
        // Send message
        match session.chat_post_message(&post_chat_req).await {
            Ok(response) => {
                info!("✅ Slack message sent successfully: {:?}", response.ts);
                Ok(())
            }
            Err(e) => {
                error!("❌ Error sending Slack message: {}", e);
                Err(anyhow::anyhow!("Unable to send Slack message: {}", e))
            }
        }
    }
}
