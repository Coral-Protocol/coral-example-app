mod discord;
mod timeout;

use std::sync::Arc;
use crate::discord::thread_watcher::{ShardManagerContainer, ThreadEventHandler, ThreadWatcher};
use clap::{arg, Parser};
use coral_rs::agent::Agent;
use coral_rs::agent_loop::AgentLoop;
use coral_rs::completion_evaluated_prompt::CompletionEvaluatedPrompt;
use coral_rs::init_tracing;
use coral_rs::mcp_server::McpConnectionBuilder;
use coral_rs::rig::client::{CompletionClient, ProviderClient};
use coral_rs::rig::providers::openrouter;
use coral_rs::rig::providers::openai::GPT_4_1_MINI;
use coral_rs::telemetry::TelemetryMode;
use futures::stream;
use serenity::all::{ChannelId, GatewayIntents, GetMessages};
use serenity::Client;
use tokio::select;
use tracing::log::info;
use crate::discord::thread_message::ThreadMessage;
use crate::discord::tools::THREAD_RESPOND_TOOL_NAME;

use crate::discord::tools::ThreadRespondTool;
use crate::timeout::Timeout;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Arguments {
    /// The discord API token
    #[arg(long, env = "DISCORD_API_TOKEN")]
    api_token: String,

    /// The target thread to provide support in
    #[arg(long, env = "DISCORD_THREAD_ID")]
    thread_id: ChannelId,

    /// The amount of time before sending a timeout warning
    #[arg(long, env = "DISCORD_TIMEOUT_WARNING")]
    timeout_duration_warning: humantime::Duration,

    /// The amount of time to wait before timing out the support thread.  This is in addition to
    /// the warning time
    #[arg(long, env = "DISCORD_TIMEOUT")]
    timeout_duration: humantime::Duration,
}

#[tokio::main]
async fn main() {
    init_tracing().expect("Failed to set up tracing");

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

    let timeout = Arc::new(Timeout::new(
        args.timeout_duration_warning.into(),
        args.timeout_duration.into(),
        client.http.clone(),
        channel.clone(),
    ));

    let watcher = Arc::new(ThreadWatcher::new(channel.clone(), timeout.clone()));
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
        .connect()
        .await.expect("Failed to connect to the Coral server");

    let mut preamble = CompletionEvaluatedPrompt::new()
        .string(format!(r#"
You a Coral agent tasked with assisting {owner_id} in a support thread.

# Workflow for every new message:
1. Create a Coral thread for this support thread if one doesn't already exist
2. Determine which agents can help with the {owner_id}'s request
3. Alert {owner_id} using {THREAD_RESPOND_TOOL_NAME} that you are going ask other agents and that it make take some time
4. Communicate with the agents determined in step 2.
5. Summarise information from other agents and provide it to {owner_id} using {THREAD_RESPOND_TOOL_NAME}

# Support tips
1. If a message looks incomplete, wait for the user to follow-up
2. Prioritise responding to {owner_id}, other users may send messages in the same support thread

# Discord tips
1. Some or all of the the user's query may exist as the title of the thread
2. Markdown and emojis are supported, notifying users can be done with the <@userid> syntax, e.g <@{owner_id}>
3. The platform and communication on it is generally informal

# Discord thread information
Title: {}
Owner: {owner_id}
"#, channel.name));

    // A discord thread is spawned with one message, so take the last message and send it to the
    // message queue so that it is processed as a loop prompt
    let last_message = existing_messages.pop()
        .expect("No existing messages found on the created thread");

    info!("Responding to thread: {}", channel.name);
    info!("With message body: {}", last_message.content);

    let _ = watcher.sender.lock().await.send(last_message);

    // If there are more messages (happens if the support agent joins late or if they are re-added
    // to the thread), attach the messages to the additional_prompting string
    if !existing_messages.is_empty() {
        preamble = preamble.string("\n\n# Previous messages\n");
        preamble = preamble.string(existing_messages
            .iter()
            .rev()
            .flat_map(|x| serde_json::to_string(x))
            .collect::<Vec<_>>()
            .join("\n")
            .as_str());
    }

    // Add coral resources
    preamble = preamble.all_resources(coral.clone());

    let model = GPT_4_1_MINI;
    let completion_agent = openrouter::Client::from_env()
        .agent(model)
        .tool(ThreadRespondTool::new(http.clone(), channel))
        .temperature(0.30)
        .max_tokens(512)
        .build();

    let agent = Agent::new(completion_agent)
        .preamble(preamble)
        .telemetry(TelemetryMode::OpenAI, model)
        .mcp_server(coral);

    let prompt_stream = stream::unfold(watcher.receiver.clone(), |receiver| async move {
        let mut messages = Vec::new();
        if receiver.lock().await.recv_many(&mut messages, 16).await == 0 {
            None
        }
        else {
            info!("Received {} messages", messages.len());

            let prompt = CompletionEvaluatedPrompt::new()
                .string("[START OF AUTOMATED MESSAGE]")
                .string(format!("New message data received, respond using {THREAD_RESPOND_TOOL_NAME}:"))
                .string(messages
                    .iter()
                    .flat_map(serde_json::to_string)
                    .collect::<Vec<_>>()
                    .join("\n"))
                .string("[END OF AUTOMATED MESSAGE]");

            Some((prompt, receiver))
        }
    });

    let agent_handle = AgentLoop::new(agent, prompt_stream)
        .execute();

    let timeout_handle = timeout.run();

    select! {
        _ = agent_handle => {
            info!("Agent thread exited")
        },
        _ = discord_handle => {
            info!("Discord thread exited")
        },
        _ = timeout_handle => {
            info!("Timeout reached")
        }
    }
}
