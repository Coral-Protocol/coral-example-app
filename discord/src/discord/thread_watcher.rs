use serenity::all::{Context, GuildChannel, PartialGuildChannel, Ready, ShardManager};
use serenity::client::EventHandler;
use serenity::{async_trait};
use std::sync::Arc;
use serenity::futures::StreamExt;
use serenity::prelude::TypeMapKey;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};
use crate::discord::thread_message::ThreadMessage;

pub struct ThreadWatcher {
    channel: GuildChannel,
    pub sender: Arc<Mutex<UnboundedSender<ThreadMessage>>>,
    pub receiver: Arc<Mutex<UnboundedReceiver<ThreadMessage>>>
}

pub struct ThreadEventHandler;
#[async_trait]
impl EventHandler for ThreadEventHandler {
    async fn ready(&self, ctx: Context, ready: Ready) {
        let data = ctx.data.read().await;
        let watcher = data
            .get::<ThreadWatcher>()
            .unwrap();

        let shard_manager = data
            .get::<ShardManagerContainer>()
            .unwrap();

        info!("Listening for messages in channel {} ({})",
            watcher.channel.name, watcher.channel.id);
        let mut stream = watcher.channel
            .await_replies(&ctx.shard)
            .stream();

        while let Some(message) = stream.next().await {
            if message.author.id == ready.user.id {
                continue;
            }

            let sender = watcher.sender.lock().await;
            if let Err(e) = sender.send(message.into()) {
                error!("Could not send message from collector to MPSC channel: {e}");
                shard_manager.shutdown_all().await;
            }
        }
    }

    async fn thread_update(
        &self,
        ctx: Context,
        _old: Option<GuildChannel>,
        new: GuildChannel
    ) {
        let data = ctx.data.read().await;
        let watcher = data
            .get::<ThreadWatcher>()
            .unwrap();

        let shard_manager = data
            .get::<ShardManagerContainer>()
            .unwrap();

        if let Some(data) = new.thread_metadata {
            if watcher.channel.id == new.id && (data.archived || data.locked) {
                warn!("Thread has been archived or locked, shutting down {} ({})",
                    watcher.channel.name, watcher.channel.id);

                shard_manager.shutdown_all().await;
            }
        }
    }

    async fn thread_delete(
        &self,
        ctx: Context,
        thread: PartialGuildChannel,
        _full_thread_data: Option<GuildChannel>
    ) {
        let data = ctx.data.read().await;
        let watcher = data
            .get::<ThreadWatcher>()
            .unwrap();

        let shard_manager = data
            .get::<ShardManagerContainer>()
            .unwrap();

        if watcher.channel.id == thread.id {
            warn!("Thread {} ({}) has been deleted, shutting down",
                    watcher.channel.name, watcher.channel.id);

            shard_manager.shutdown_all().await;
        }
    }
}

pub struct ShardManagerContainer;

impl TypeMapKey for ShardManagerContainer {
    type Value = Arc<ShardManager>;
}

impl TypeMapKey for ThreadWatcher {
    type Value = Arc<ThreadWatcher>;
}

impl ThreadWatcher {
    pub fn new(channel: GuildChannel) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        Self {
            channel,
            sender: Arc::new(tx.into()),
            receiver: Arc::new(rx.into()),
        }
    }
}