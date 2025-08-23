use std::sync::Arc;
use coral_rs::rig::completion::ToolDefinition;
use coral_rs::rig::tool::Tool;
use coral_rs::rmcp as rmcp;
use coral_rs::rmcp::schemars::schema_for;
use rmcp::schemars as schemars;
use serde::{Deserialize, Serialize};
use serenity::all::{CreateMessage, GuildChannel, Http, MessageId, Timestamp};

pub struct ThreadRespondTool {
    http: Arc<Http>,
    channel: GuildChannel
}

#[derive(Debug, thiserror::Error)]
#[error("Response error")]
pub enum ResponseError {
    #[error("Discord error: {0}")]
    SerenityError(serenity::Error)
}

#[derive(Deserialize, Serialize, schemars::JsonSchema)]
pub struct Args {
    #[schemars(description = "The message content")]
    content: String,
}

impl ThreadRespondTool {
    pub fn new(
        http: Arc<Http>,
        channel: GuildChannel,
    ) -> Self {
        Self {
            http,
            channel
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ThreadRespondToolOutput {
    id: MessageId,
    timestamp: Timestamp,
}

impl Tool for ThreadRespondTool {
    const NAME: &'static str = "send_discord_message";
    type Error = ResponseError;
    type Args = Args;
    type Output = ThreadRespondToolOutput;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        let parameters = serde_json::to_value(schema_for!(Args)).unwrap();

        ToolDefinition {
            name: Self::NAME.to_string(),
            description: "Sends a message to the Discord thread.".to_string(),
            parameters,
        }
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let message = CreateMessage::new().content(args.content);
        let msg = self.channel
            .send_message(&self.http, message).await
            .map_err(ResponseError::SerenityError)?;

        Ok(ThreadRespondToolOutput {
            id: msg.id,
            timestamp: msg.timestamp
        })
    }
}