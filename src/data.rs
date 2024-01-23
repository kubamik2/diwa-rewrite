use poise::ReplyHandle;
use serenity::model::channel::Message;
use crate::convert_query::ConvertedQuery;
use tokio::{sync::Mutex, task::AbortHandle};
use crate::metadata::UserMetadata;
use std::collections::HashMap;

pub type Context<'a> = poise::Context<'a, Data, crate::commands::error::CommandError>;
pub struct Data {
    pub cleanups: Mutex<Vec<Cleanup>>,
    pub spotify_client: crate::api_integration::spotify::SpotifyClient,
    pub youtube_client: crate::api_integration::youtube::YouTubeClient,
    pub afk_timeout_abort_handle_map: Mutex<HashMap<u64, AbortHandle>>,
    pub reqwest_client: reqwest::Client
}

#[derive(Clone)]
pub struct Cleanup {
    pub message: Message,
    pub delay: std::time::Duration
}

impl Data {
    pub fn new(spotify_client: crate::api_integration::spotify::SpotifyClient, youtube_client: crate::api_integration::youtube::YouTubeClient) -> Self {
        Self { cleanups: Mutex::new(vec![]), spotify_client, youtube_client, afk_timeout_abort_handle_map: Mutex::new(HashMap::new()), reqwest_client: reqwest::Client::new() }
    }

    pub async fn convert_query(&self, query: &str, added_by: UserMetadata) -> Result<ConvertedQuery, crate::convert_query::ConversionError> {
        crate::convert_query::convert_query(&self.youtube_client, &self.spotify_client, query, added_by, self.reqwest_client.clone()).await
    }

    pub async fn add_to_cleanup<'a>(&self, reply_handle: ReplyHandle<'a>, delay: std::time::Duration) {
        if let Ok(message) = reply_handle.into_message().await {
            self.cleanups.lock().await.push(Cleanup { message, delay});
        }
    }
}