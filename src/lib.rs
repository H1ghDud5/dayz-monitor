use a2s::{info::ExtendedServerInfo, A2SClient};
use serde::Deserialize;
use std::net::SocketAddr;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DayzMonitorError {
    #[error("Tokio IO error: {0}")]
    TokioIOError(#[from] tokio::io::Error),

    #[error("A2S error: {0}")]
    A2SError(#[from] a2s::errors::Error),

    #[error("Failed to extract server keywords from A2S response (keywords missing).")]
    ExtractServerInfoKeywordsMissing,
}

fn default_server_name() -> String {
    "DayZ Server".to_string()
}

fn default_update_interval_secs() -> u64 {
    60
}

#[derive(Debug, Deserialize, Clone)]
pub struct DayzMonitorConfig {
    pub discord_token: String,
    pub server_address: SocketAddr,

    #[serde(default = "default_server_name")]
    pub server_name: String,

    /// Text channel to post/edit the status embed in
    pub text_channel_id: u64,

    /// Optional: message id to ALWAYS edit (recommended)
    #[serde(default)]
    pub status_message_id: Option<u64>,

    #[serde(default = "default_update_interval_secs")]
    pub update_interval_secs: u64,
}

#[derive(Debug, Clone)]
pub struct ServerInfo {
    pub server_time: Option<String>,
    pub players_in_queue: Option<u32>,
    pub players: u32,
    pub max_players: u32,
}

pub async fn retrieve_server_info(
    client: &A2SClient,
    addr: SocketAddr,
) -> Result<ServerInfo, DayzMonitorError> {
    tracing::debug!("Querying server info for '{addr}'");
    let info = client.info(addr).await?;

    let mut server_info = extract_time_and_queue(info.extended_server_info)
        .ok_or(DayzMonitorError::ExtractServerInfoKeywordsMissing)?;

    server_info.players = info.players as u32;
    server_info.max_players = info.max_players as u32;
    Ok(server_info)
}

fn extract_time_and_queue(info: ExtendedServerInfo) -> Option<ServerInfo> {
    let values = info.keywords?;
    let split: Vec<&str> = values.split(',').collect();

    let mut server_info = ServerInfo {
        server_time: None,
        players_in_queue: None,
        players: 0,
        max_players: 0,
    };

    for value in split {
        if value.starts_with("lqs") {
            server_info.players_in_queue = value.replace("lqs", "").parse::<u32>().ok();
            continue;
        }

        if value.contains(':') && server_info.server_time.is_none() && value.len() <= 8 {
            server_info.server_time = Some(value.to_owned());
        }
    }

    Some(server_info)
}
