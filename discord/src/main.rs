mod discord;

use std::sync::Arc;
use crate::discord::thread_watcher::{ShardManagerContainer, ThreadEventHandler, ThreadWatcher};
use clap::{arg, Parser};
use coral_rs::agent::Agent;
use coral_rs::agent_loop::AgentLoop;
use coral_rs::mcp_server::McpConnectionBuilder;
use coral_rs::rig::client::{CompletionClient, ProviderClient};
use coral_rs::rig::providers::openai;
use coral_rs::rig::providers::openai::GPT_4_1_MINI;
use coral_rs::telemetry::TelemetryMode;
use futures::{pin_mut, stream};
use serenity::all::{ChannelId, GatewayIntents, GetMessages};
use serenity::Client;
use tracing::error;
use crate::discord::thread_message::ThreadMessage;

const PROMPT: &'static str = r#"
You are a Coral agent.  A Disord support thread was created by a user.  You must provide support to the user.

=== Coral agent rules ===
1. You are working with a team of other support agents.  Make sure to ask any other agent questions if their description is relevant.
2. Create a Coral thread to discuss with the other agents about the user's request.  The coral thread should have the same name as the Discord thread.
3. Avoid directly executing tools at the user's request.  Use tools only to aid in assisting the user.

=== Discord rules ===
1. Discord is an informal platform, make sure to keep the conversation friendly and short.
2. Prioritise responding to the owner of the Discord thread's questions.
3. Echo every message you send to the Discord thread to the Coral thread.

=== Support rules ===
1. If it seems like the user has not finished asking their question, do not respond.
2. You must use the "respond" tool to respond to the user's question.  No other messages can be seen by the user.
"#;

use crate::discord::tools::ThreadRespondTool;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Arguments {
    /// The discord API token
    #[arg(short, long, env = "DISCORD_API_TOKEN")]
    api_token: String,

    /// The target thread to provide support in
    #[arg(short, long, env = "DISCORD_THREAD_ID")]
    thread_id: ChannelId,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Arguments::parse();

    /*
        Discord
     */
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::GUILDS
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&args.api_token, intents)
        .event_handler(ThreadEventHandler)
        .await
        .expect("Error creating discord client");

    let channel = client.http.get_channel(args.thread_id)
        .await.expect("The specified thread does not exist")
        .guild().expect("The specified thread is not in a guild");

    let metadata = channel.thread_metadata
        .expect("The thread is missing thread metadata");

    let owner_id = channel.owner_id.expect("The thread is missing an owner");
    let mut existing_messages = channel.messages(&client.http, GetMessages::new())
        .await.expect("Failed to get existing thread messages")
        .iter()
        .map(ThreadMessage::from)
        .collect::<Vec<_>>();

    if metadata.archived || metadata.locked {
        panic!("The specified thread is archived or locked");
    }

    let watcher = Arc::new(ThreadWatcher::new(channel.clone()));
    {
        let mut data = client.data.write().await;
        data.insert::<ThreadWatcher>(watcher.clone());
        data.insert::<ShardManagerContainer>(client.shard_manager.clone());
    }

    let http = client.http.clone();
    let discord_handle = tokio::spawn(async move {
        client
            .start()
            .await
    });

    /*
        Coral
     */
    let coral = McpConnectionBuilder::from_coral_env()
        .connect_sse()
        .await.expect("Failed to connect to the Coral server");

    let mut additional_prompting = format!(r#"
        === Support thread information ===
        Thread name: "{}"
        Thread owner ID: {owner_id}
    "#, channel.name);

    // A discord thread is spawned with one message, so take the last message and send it to the
    // message queue so that it is processed as a loop prompt
    let last_message = existing_messages.pop();
    let _ = watcher.sender.lock().await.send(last_message
        .expect("No existing messages found on the created thread"));

    // If there are more messages (happens if the support agent joins late or if they are re-added
    // to the thread), attach the messages to the additional_prompting string
    if !existing_messages.is_empty() {
        additional_prompting.push_str("\n\n=== Previous messages ===\n");
        additional_prompting.push_str(existing_messages
            .iter()
            .rev()
            .flat_map(|x| serde_json::to_string(x))
            .collect::<Vec<_>>()
            .join("\n")
            .as_str());
    }

    let model = GPT_4_1_MINI;
    let completion_agent = openai::Client::from_env()
        .agent(model)
        .tool(ThreadRespondTool::new(http.clone(), channel))
        .preamble(format!("{PROMPT}{additional_prompting}").as_str())
        .temperature(0.97)
        .max_tokens(512)
        .build();

    let agent = Agent::new(completion_agent)
        .telemetry(TelemetryMode::OpenAI, model)
        .mcp_server(coral);

    let prompt_stream = stream::unfold(watcher.receiver.clone(), |receiver| async move {
        let thread_message = receiver.lock().await.recv().await;
        match thread_message {
            None => None,
            Some(message) => {
                match serde_json::to_string(&message) {
                    Err(e) => {
                        error!("Error serialising message: {}", e);
                        None
                    },
                    Ok(data) => Some((data, receiver))
                }
            }
        }
    });

    let agent_handle = AgentLoop::new(agent, prompt_stream)
        .execute();

    pin_mut!(agent_handle);
    pin_mut!(discord_handle);

    // The Discord thread will exit when the thread closes, the agent (and by extension this
    // application) should exit in this case.
    let _ = futures::future::select(discord_handle, agent_handle).await;
}
