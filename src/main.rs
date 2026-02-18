use std::{sync::Arc, time::Duration};

use a2s::A2SClient;
use dayz_monitor::{retrieve_server_info, DayzMonitorConfig, ServerInfo};
use serenity::{
    all::{
        ChannelId, CreateEmbed, CreateMessage, EditMessage, GatewayIntents, MessageId,
    },
    async_trait,
    model::gateway::Ready,
    prelude::*,
};
use tokio::sync::RwLock;
use tracing_subscriber::EnvFilter;

struct BotState {
    config: DayzMonitorConfig,
    a2s: Arc<A2SClient>,
    message_id: Arc<RwLock<Option<MessageId>>>,
}

impl BotState {
    fn title_online(&self) -> String {
        format!("🟢 {} — Online", self.config.server_name)
    }

    fn title_offline(&self) -> String {
        format!("🔴 {} — Offline", self.config.server_name)
    }

    fn line_players(&self, info: &ServerInfo) -> String {
        match info.players_in_queue {
            Some(q) if q > 0 => format!(
                "Players: **{} / {}**  •  Queue: **{}**",
                info.players, info.max_players, q
            ),
            _ => format!("Players: **{} / {}**", info.players, info.max_players),
        }
    }

    fn line_time(&self, info: &ServerInfo) -> String {
        match &info.server_time {
            Some(t) => format!("Server time: **{}**", t),
            None => "Server time: *(unavailable)*".to_string(),
        }
    }
}

struct Handler {
    state: Arc<BotState>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _ready: Ready) {
        let state = self.state.clone();
        let http = ctx.http.clone();

        // If you set STATUS_MESSAGE_ID, we always edit that one.
        if let Some(mid) = state.config.status_message_id {
            *state.message_id.write().await = Some(MessageId::new(mid));
        }

        tokio::spawn(async move {
            let channel_id = ChannelId::new(state.config.text_channel_id);

            loop {
                // Ensure there is a message to edit (send once if missing)
                let mut lock = state.message_id.write().await;
                if lock.is_none() {
                    let mut embed = CreateEmbed::new()
                        .title("Starting…")
                        .description("Fetching server status…");

                    let msg = CreateMessage::new().add_embed(embed);

                    match channel_id.send_message(&http, msg).await {
                        Ok(sent) => {
                            tracing::info!("Posted status message: {}", sent.id);
                            *lock = Some(sent.id);
                        }
                        Err(err) => {
                            tracing::error!("Failed to send initial status message: {err:#?}");
                            drop(lock);
                            tokio::time::sleep(Duration::from_secs(state.config.update_interval_secs))
                                .await;
                            continue;
                        }
                    }
                }

                let msg_id = lock.unwrap();
                drop(lock);

                let result = retrieve_server_info(&state.a2s, state.config.server_address).await;

                let edit = match result {
                    Ok(info) => build_online_edit(&state, &info),
                    Err(err) => build_offline_edit(&state, &err.to_string()),
                };

                if let Err(err) = channel_id.edit_message(&http, msg_id, edit).await {
                    tracing::error!("Failed to edit status message: {err:#?}");
                }

                tokio::time::sleep(Duration::from_secs(state.config.update_interval_secs)).await;
            }
        });
    }
}

fn now_relative_timestamp() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("<t:{secs}:R>")
}

fn build_online_edit(state: &BotState, info: &ServerInfo) -> EditMessage {
    let title = state.title_online();
    let players_line = state.line_players(info);
    let time_line = state.line_time(info);
    let updated = now_relative_timestamp();

    let embed = CreateEmbed::new()
        .title(title)
        .description(players_line)
        .field("Details", time_line, false)
        .field("Last updated", updated, false);

    EditMessage::new().embed(embed)
}

fn build_offline_edit(state: &BotState, err: &str) -> EditMessage {
    let title = state.title_offline();
    let updated = now_relative_timestamp();

    let embed = CreateEmbed::new()
        .title(title)
        .description("Could not query the server right now.")
        .field("Last updated", updated, false)
        .field("Error", err, false);

    EditMessage::new().embed(embed)
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let _ = dotenv::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    tracing::info!("Loading dayz-monitor configuration from environment variables");
    let config: DayzMonitorConfig = serde_env::from_env()?;

    let a2s = Arc::new(A2SClient::new().await?);

    // Status-only: no privileged intents needed.
    let intents = GatewayIntents::GUILDS;

    let state = Arc::new(BotState {
        config: config.clone(),
        a2s,
        message_id: Arc::new(RwLock::new(None)),
    });

    let mut client = Client::builder(config.discord_token, intents)
        .event_handler(Handler { state })
        .await?;

    client.start().await?;
    Ok(())
}
