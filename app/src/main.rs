mod session;

use crate::session::Session;
use clap::{Parser};
use coral_rs::api::Error;
use serenity::all::{Context, EventHandler, GatewayIntents, GuildChannel, Message, Ready};
use serenity::prelude::TypeMapKey;
use serenity::{async_trait, Client};
use std::sync::{Arc};
use tracing::{error, info, warn};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Arguments {
    /// The address of the Coral server
    #[arg(short, long, default_value = "http://localhost:5555", env = "CORAL_SERVER")]
    coral_server: String,

    /// The discord API token
    #[arg(short, long, env = "DISCORD_API_TOKEN")]
    discord_api_token: String,
}

struct Handler;

#[async_trait]
impl EventHandler for Handler {

    async fn ready(&self, _: Context, ready: Ready) {
        info!("bot \"{}\" connected to Discord", ready.user.name);
    }

    async fn thread_create(&self, ctx: Context, thread: GuildChannel) {
        let data = ctx.data.read().await;
        let arguments = data
            .get::<Arguments>()
            .unwrap();

        let channel_id = match thread.parent_id {
            None => {
                warn!("received thread_create event for a thread without an ID!");
                return;
            }
            Some(channel_id) => channel_id.to_string()
        };

        let session = Session::new(
            &arguments,
            thread.id.to_string(),
            channel_id,
        );

        match session.execute().await {
            Ok(session) => {
                info!("Created session {} for thread \"{}\" ({})",
                    session.session_id, thread.name, thread.id);
            },
            Err(e) => {
                match e {
                    Error::UnexpectedResponse(e) => {
                        match e.text().await {
                            Ok(text) => {
                                error!("received unexpected response:");
                                error!("{text}");
                            },
                            Err(e) =>
                                error!("received unexpected which could not be parsed: {e}"),
                        }
                    }
                    _ => error!("error: {:?}", e),
                }
            }
        }
    }
}

impl TypeMapKey for Arguments {
    type Value = Arc<Self>;
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let args = Arguments::parse();

    let intents = GatewayIntents::GUILD_MESSAGES
        | GatewayIntents::GUILDS
        | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&args.discord_api_token, intents)
        .event_handler(Handler)
        .await.expect("Error creating client");

    {
        let mut data = client.data.write().await;
        data.insert::<Arguments>(Arc::new(args));
    }

    client
        .start()
        .await.expect("Error while running the client");
}
