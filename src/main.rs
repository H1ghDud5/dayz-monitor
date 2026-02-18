use std::{sync::Arc, time::Duration};

use a2s::A2SClient;
use dayz_monitor::{retrieve_server_info, DayzMonitorConfig, ServerInfo};
use serenity::{
    all::{ChannelId, CreateEmbed, CreateEmbedFooter, CreateMessage, EditMessage, GatewayIntents, MessageId},
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

    fn occupancy_percent(&self, info: &ServerInfo) -> u32 {
        if info.max_players == 0 {
            return 0;
        }
        (((info.players as f64 / info.max_players as f64) * 100.0).round() as u32).min(100)
    }

    fn occupancy_bar(&self, info: &ServerInfo, width: usize) -> String {
        if info.max_players == 0 {
            return "`—`".to_string();
        }
        let ratio = (info.players as f64 / info.max_players as f64).clamp(0.0, 1.0);
        let filled = (ratio * width as f64).round() as usize;
        let filled = filled.min(width);
        let empty = width.saturating_sub(filled);

        format!("`{}{}`", "█".repeat(filled), "░".repeat(empty))
    }

    fn players_summary(&self, info: &ServerInfo) -> String {
        let base = format!("**{} / {}**", info.players, info.max_players);
        match info.players_in_queue {
            Some(q) if q > 0 => format!("{base}  •  ⏳ Queue: **{q}**"),
            _ => base,
        }
    }

    fn time_summary(&self, info: &ServerInfo) -> String {
        match &info.server_time {
            Some(t) => format!("🕒 **{t}**"),
            None => "🕒 *(unavailable)*".to_string(),
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

        // If STATUS_MESSAGE_ID is set, always edit that message.
        if let Some(mid) = state.config.status_message_id {
            *state.message_id.write().await = Some(MessageId::new(mid));
        }

        tokio::spawn(async move {
            let channel_id = ChannelId::new(state.config.text_channel_id);

            loop {
                // Ensure there is a message to edit (send once if missing)
                let mut lock = state.message_id.write().await;
                if lock.is_none() {
                    let embed = CreateEmbed::new()
                        .title("Starting…")
                        .description("Fetching server status…")
                        .colour(0x5865F2)
                        .footer(CreateEmbedFooter::new("dayz-monitor"));

                    let msg = CreateMessage::new().add_embed(embed);

                    match channel_id.send_message(&http, msg).await {
                        Ok(sent) => {
                            tracing::info!("Posted status message: {}", sent.id);
                            *lock = Some(sent.id);
                        }
                        Err(err) => {
                            tracing::error!("Failed to send initial status message: {err:#?}");
                            drop(lock);
                            tokio::time::sleep(Duration::from_secs(state.config.update_interval_secs)).await;
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

fn discord_ts(secs: u64, style: &str) -> String {
    format!("<t:{secs}:{style}>")
}

fn build_online_edit(state: &BotState, info: &ServerInfo) -> EditMessage {
    let pct = state.occupancy_percent(info);
    let bar = state.occupancy_bar(info, 18);
    let players = state.players_summary(info);
    let time = state.time_summary(info);

    let last_rel = discord_ts(info.last_updated_unix, "R");
    let last_full = discord_ts(info.last_updated_unix, "f");

    let embed = CreateEmbed::new()
        .title(state.title_online())
        .description(format!(
            "👥 Players: {players}\n📊 Load: {bar} **{pct}%**\n{time}"
        ))
        .colour(0x57F287)
        .field("📍 Address", format!("`{}`", state.config.server_address), true)
        .field("🔄 Update interval", format!("`{}s`", state.config.update_interval_secs), true)
        .field("🕐 Last updated", format!("{last_rel}\n{last_full}"), false)
        .footer(CreateEmbedFooter::new("dayz-monitor"));

    EditMessage::new().embed(embed)
}

fn build_offline_edit(state: &BotState, err: &str) -> EditMessage {
    let now_secs = std::time::SystemTi_
