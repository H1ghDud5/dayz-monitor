use std::{sync::Arc, time::Duration};

use a2s::A2SClient;
use dayz_monitor::{retrieve_server_info, DayzMonitorConfig, ServerInfo};
use serenity::{
    all::{
        ChannelId, Client, CreateEmbed, CreateEmbedFooter, CreateMessage, EditMessage,
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
    message_id: Arc<RwLock<Option<MessageId>>>,
}

struct Handler {
    state: Arc<BotState>,
}

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, ctx: Context, _: Ready) {
        let state = self.state.clo
