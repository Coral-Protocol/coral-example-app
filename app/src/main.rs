mod session;

use std::collections::HashSet;
use crate::session::Session;
use clap::{Parser};
use coral_rs::api::generated::{Error, ResponseValue};
use serenity::all::{ChannelId, Context, EventHandler, GatewayIntents, GuildChannel, Ready};
use serenity::prelude::TypeMapKey;
use serenity::{async_trait, Client};
use std::sync::{Arc};
use coral_rs::api::generated::types::RouteException;
use tokio::sync::Mutex;
use tracing::{error, info};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Arguments {
    /// The address of the Coral server
    #[arg(long, default_value = "http://localhost:5555", env = "CORAL_SERVER")]
    coral_server: String,

    /// The discord API token
    #[arg(long, env = "DISCORD_API_TOKEN")]
    discord_api_token: String,

    /// The amount of time before sending a timeout warning
    #[arg(long, env = "DISCORD_TIMEOUT_WARNING")]
    timeout_duration_warning: Option<humantime::Duration>,

    /// The amount of time to wait before timing out the support thread.  This is in addition to
    /// the warning time
    #[arg(long, env = "DISCORD_TIMEOUT")]
    timeout_duration: Option<humantime::Duration>,

    /// The OpenRouter API key
    #[arg(long, env = "OPENROUTER_API_KEY")]
    openrouter_api_key: String,

    /// If this is set, agents.coralprotocol.org will be used to source the context7 agent remotely.
    /// The local coral server must be set up with a wallet to use this feature
    #[arg(long)]
    remote: bool,
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

        {
            let watchlist = data
                .get::<Watchlist>()
                .unwrap();

            // Watchlist already contained thread ID
            if !watchlist.lock().await.insert(thread.id) {
                return;
            }
        }

        let session = Session::new(
            &arguments,
            thread.id.to_string()
        );

        match session.execute(arguments.remote).await {
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
                    Error::ErrorResponse(e) => {
                        let status = e.status();
                        let exception: RouteException = ResponseValue::into_inner(e);
                        error!("{status}: {}", exception.message.unwrap_or_else(|| "no message".to_string()));
                        error!("Stack trace: ");
                        for (i, stack) in exception.stack_trace.iter().enumerate() {
                            error!("{i}. {stack}");
                        }
                    }
                    _ => error!("{e:#?}"),
                }
            }
        }
    }
}

impl TypeMapKey for Arguments {
    type Value = Arc<Self>;
}

struct Watchlist;

impl TypeMapKey for Watchlist {
    type Value = Mutex<HashSet<ChannelId>>;
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
        data.insert::<Watchlist>(Mutex::new(HashSet::new()));
    }

    client
        .start()
        .await.expect("Error while running the client");
}
