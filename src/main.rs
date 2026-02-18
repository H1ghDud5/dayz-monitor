use std::{sync::Arc, time::Duration};

use a2s::A2SClient;
use dayz_monitor::{retrieve_server_info, DayzMonitorConfig, ServerInfo};
use serenity::{
    all::{
        ChannelId, Client, CreateEmbed, CreateMessage, EditMessage,
        GatewayIntents, MessageId,
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
    status_message_id: Arc<RwLock<Option<MessageId>>>,
}

struct Handler {
    state: Arc<BotState>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _: Ready) {
        let state = self.state.clone();
        let http = ctx.http.clone();
        let channel_id = ChannelId::new(state.config.text_channel_id);

        if let Some(mid) = state.config.status_message_id {
            *state.status_message_id.write().await = Some(MessageId::new(mid));
        }

        tokio::spawn(async move {
            loop {
                let msg_id = ensure_status_message(&state, &http, channel_id).await;

                let edit = match retrieve_server_info(&state.a2s, state.config.server_address).await {
                    Ok(info) => build_online(&state, &info),
                    Err(err) => build_offline(&state, &err.to_string()),
                };

                let _ = channel_id.edit_message(&http, msg_id, edit).await;

                tokio::time::sleep(Duration::from_secs(state.config.update_interval_secs)).await;
            }
        });
    }
}

async fn ensure_status_message(
    state: &BotState,
    http: &serenity::http::Http,
    channel_id: ChannelId,
) -> MessageId {
    if let Some(id) = *state.status_message_id.read().await {
        return id;
    }

    let embed = CreateEmbed::new()
        .title("Starting…")
        .description("Fetching server status…")
        .colour(0x5865F2);

    let msg = CreateMessage::new().add_embed(embed);

    match channel_id.send_message(http, msg).await {
        Ok(sent) => {
            let id = sent.id;
            *state.status_message_id.write().await = Some(id);
            id
        }
        Err(_) => {
            tokio::time::sleep(Duration::from_secs(state.config.update_interval_secs)).await;
            MessageId::new(0)
        }
    }
}

fn rel_ts(secs: u64) -> String {
    format!("<t:{}:R>", secs)
}

fn build_online(state: &BotState, info: &ServerInfo) -> EditMessage {
    let players_line = match info.players_in_queue {
        Some(q) if q > 0 => format!("**{} / {}** • ⏳ Queue: **{}**", info.players, info.max_players, q),
        _ => format!("**{} / {}**", info.players, info.max_players),
    };

    let time_line = info.server_time.clone().unwrap_or_else(|| "Unknown".to_string());

    let embed = CreateEmbed::new()
        .title(format!("🟢 {} — Online", state.config.server_name))
        .description(format!(
            "👥 Players: {}\n🕒 Server Time: **{}**",
            players_line, time_line
        ))
        .colour(0x57F287)
        .field("📍 Address", format!("`{}`", state.config.server_address), true)
        .field("🔄 Update", format!("`{}s`", state.config.update_interval_secs), true)
        .field("🕐 Last Updated", rel_ts(info.last_updated_unix), false);

    EditMessage::new().embed(embed)
}

fn build_offline(state: &BotState, err: &str) -> EditMessage {
    let embed = CreateEmbed::new()
        .title(format!("🔴 {} — Offline", state.config.server_name))
        .description("⚠️ Could not query the server.")
        .colour(0xED4245)
        .field("📍 Address", format!("`{}`", state.config.server_address), true)
        .field("🧾 Error", format!("`{}`", err), false);

    EditMessage::new().embed(embed)
}

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let _ = dotenv::dotenv();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config: DayzMonitorConfig = serde_env::from_env()?;
    let a2s = Arc::new(A2SClient::new().await?);

    let state = Arc::new(BotState {
        config: config.clone(),
        a2s,
        status_message_id: Arc::new(RwLock::new(None)),
    });

    let intents = GatewayIntents::GUILDS;

    let mut client = Client::builder(config.discord_token, intents)
        .event_handler(Handler { state })
        .await?;

    client.start().await?;
    Ok(())
}
