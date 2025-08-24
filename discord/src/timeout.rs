use serenity::all::{CreateEmbed, CreateMessage, GuildChannel, Timestamp};
use serenity::http::Http;
use std::sync::Arc;
use std::time::Duration;
use tokio::select;
use tokio::sync::mpsc::error::SendError;
use tokio::sync::{mpsc, Mutex};
use tokio::time::sleep;

pub struct Timeout {
    warning_duration: Duration,
    timeout_duration: Duration,
    reset_tx: mpsc::UnboundedSender<()>,
    reset_rx: Mutex<mpsc::UnboundedReceiver<()>>,
    http: Arc<Http>,
    channel: GuildChannel
}

impl Timeout {
    pub fn new(
        warning_duration: Duration,
        timeout_duration: Duration,
        http: Arc<Http>,
        channel: GuildChannel
    ) -> Self {
        let (reset_tx, reset_rx) = mpsc::unbounded_channel();
        Self {
            warning_duration,
            timeout_duration,
            reset_tx,
            reset_rx: reset_rx.into(),
            http,
            channel,
        }
    }

    pub async fn reset(&self) -> Result<(), SendError<()>> {
        self.reset_tx.send(())
    }

    pub async fn send_timeout_warning(&self) {
        let timeout = Timestamp::now().unix_timestamp() + self.timeout_duration.as_secs() as i64;

        let embed = CreateEmbed::new()
            .title("⚠️ Timeout warning")
            .description(format!("This thread will closed automatically for inactivity <t:{timeout}:R>"));
        let message = CreateMessage::new()
            .embed(embed);

        if let Err(e) = self.channel.send_message(&self.http, message).await {
            println!("Error sending timeout warning: {e:?}");
        }
    }

    pub async fn send_timeout_message(&self) {
        let embed = CreateEmbed::new()
            .title("⚠️ Timeout")
            .description("This thread has been closed due to inactivity");
        let message = CreateMessage::new()
            .embed(embed);

        if let Err(e) = self.channel.send_message(&self.http, message).await {
            println!("Error sending timeout message: {e:?}");
        }
    }

    pub async fn run(&self) {
        loop {
            {
                let warning_sleep = sleep(self.warning_duration);
                let mut reset_rx = self.reset_rx.lock().await;
                let reset = reset_rx.recv();

                select! {
                    _ = warning_sleep => {
                        self.send_timeout_warning().await;
                    },
                    _ = reset => {
                        continue;
                    }
                }
            }

            let timeout_sleep = sleep(self.timeout_duration);
            let mut reset_rx = self.reset_rx.lock().await;
            let reset = reset_rx.recv();
            select! {
                _ = timeout_sleep => {
                    self.send_timeout_message().await;
                    break;
                },
                _ = reset => {
                    continue;
                }
            }
        }
    }
}