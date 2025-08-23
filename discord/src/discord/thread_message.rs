use serde::Serialize;
use serenity::all::{Message, UserId};

/// This thread message is given to AI models.  Currently, it only includes the message content but
/// should be extended to include other fields from Discord that the user may populate.
///
/// For example, the user may send GIFs, images or attachments that the AI models will ignore if not
/// here
#[derive(Serialize)]
pub struct ThreadMessage {
    sender: UserId,
    content: String
}

impl From<&Message> for ThreadMessage {
    fn from(message: &Message) -> ThreadMessage {
        ThreadMessage {
            sender: message.author.id,
            content: message.content.clone()
        }
    }
}

impl From<Message> for ThreadMessage {
    fn from(message: Message) -> ThreadMessage {
        ThreadMessage {
            sender: message.author.id,
            content: message.content
        }
    }
}