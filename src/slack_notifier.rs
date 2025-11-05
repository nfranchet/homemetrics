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
    
    /// Send a simple text message to Slack
    pub async fn send_message(&self, text: &str) -> Result<()> {
        info!("Sending Slack message");
        
        let post_chat_req = SlackApiChatPostMessageRequest::new(
            self.channel_id.clone(),
            SlackMessageContent::new().with_text(text.to_string()),
        );
        
        let session = self.client.open_session(&self.token);
        
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
