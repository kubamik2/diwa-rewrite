use std::{time::Duration, sync::Arc};

use crate::{
    error::{ Error, AppError },
    utils::{format_duration, create_now_playing_embed},
    AudioSource
};
use serenity::http::Http;
use songbird::{
    input::Metadata as SongbirdMetadata,
    typemap::TypeMapKey, tracks::TrackHandle, Call, EventContext
};
use poise::{ async_trait, serenity_prelude::{User, ChannelId} };
use tokio::sync::Mutex;
#[derive(Debug, Clone, Hash)]
pub struct TrackMetadata {
    pub video_metadata: VideoMetadata,
    pub added_by: UserMetadata
}

impl Default for TrackMetadata {
    fn default() -> Self {
        Self { 
            video_metadata: VideoMetadata { title: "!Error!".to_owned(), duration: std::time::Duration::ZERO, audio_source: AudioSource::YouTube { video_id: "".to_owned() } },
            added_by: UserMetadata { id: 0, name: "".to_owned(), avatar_url: None } 
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub struct VideoMetadata {
    pub title: String,
    pub duration: std::time::Duration,
    pub audio_source: AudioSource
}

impl Into<SongbirdMetadata> for VideoMetadata {
    fn into(self) -> SongbirdMetadata {
        let source_url = match self.audio_source {
            AudioSource::YouTube { video_id } => Some(format!("https://youtu.be/{video_id}")),
            AudioSource::File { path: _ } => None
        };
        SongbirdMetadata { 
            channels: Some(2),
            sample_rate: Some(48000),
            title: Some(self.title),
            duration: Some(self.duration),
            source_url,
            ..Default::default()
        }
    }
}

impl VideoMetadata {
    pub fn to_queue_string(&self, playtime: Option<Duration>) -> String {
        let mut formatted_duration = format_duration(self.duration, None);
        if let Some(playtime) = playtime {
            let formatted_playtime = format_duration(playtime, Some(formatted_duration.len()));
            formatted_duration = format!("{} / {}", formatted_playtime, formatted_duration);
        }
        match &self.audio_source {
            AudioSource::YouTube { video_id } => {
                format!("[{}](https://youtu.be/{}) | {}", self.title, video_id, formatted_duration)
            },
            AudioSource::File { path: _ } => {
                format!("{} | {}", self.title, formatted_duration)
            }
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub struct UserMetadata {
    pub id: u64,
    pub name: String,
    pub avatar_url: Option<String>
}

impl From<User> for UserMetadata {
    fn from(value: User) -> Self {
        let id = value.id.0;
        let name = value.name.clone();
        let avatar_url = value.avatar_url();
        Self { id, name, avatar_url }
    }
}

impl From<&User> for UserMetadata {
    fn from(value: &User) -> Self {
        let id = value.id.0;
        let name = value.name.clone();
        let avatar_url = value.avatar_url();
        Self { id, name, avatar_url }
    }
}

impl TypeMapKey for TrackMetadata {
    type Value = TrackMetadata;
}

impl TypeMapKey for VideoMetadata {
    type Value = VideoMetadata;
}

impl TypeMapKey for UserMetadata {
    type Value = UserMetadata;
}

#[async_trait]
pub trait LazyMetadata {
    async fn read_lazy_metadata(&self) -> Option<TrackMetadata>;
    async fn write_lazy_metadata(&mut self, metadata: TrackMetadata);
    async fn generate_lazy_metadata(&mut self) -> Result<TrackMetadata, Error>;
    async fn read_awake_lazy_metadata(&mut self) -> Result<TrackMetadata, Error>;
    async fn awake_lazy_metadata(&mut self) -> Result<(), Error>;
    async fn read_added_by(&self) -> Option<UserMetadata>;
    async fn write_added_by(&mut self, user_metadata: UserMetadata);
    fn is_lazy(&self) -> bool;
}

#[async_trait]
impl LazyMetadata for TrackHandle {
    async fn read_lazy_metadata(&self) -> Option<TrackMetadata> {
        self.typemap().read().await.get::<TrackMetadata>().cloned()
    }

    async fn write_lazy_metadata(&mut self, metadata: TrackMetadata) {
        self.typemap().write().await.insert::<TrackMetadata>(metadata)
    }

    async fn generate_lazy_metadata(&mut self) -> Result<TrackMetadata, Error> {
        let title = self.metadata().title.as_ref().ok_or(AppError::MissingValue { value: "metadata.title".to_owned() })?;
        let added_by = self.read_added_by().await.ok_or(AppError::MissingValue { value: "added_by".to_owned() })?;
        let video_metadata = crate::scrapers::youtube::search(title).await?;
        Ok(TrackMetadata { video_metadata, added_by })
    }

    async fn read_awake_lazy_metadata(&mut self) -> Result<TrackMetadata, Error> {
        match self.read_lazy_metadata().await {
            Some(metadata) => Ok(metadata),
            None => {
                let track_metadata = self.generate_lazy_metadata().await?;
                self.write_lazy_metadata(track_metadata.clone()).await;
                Ok(track_metadata)
            }
        }
    }

    async fn awake_lazy_metadata(&mut self) -> Result<(), Error> {
        if self.is_lazy() {
            if self.read_lazy_metadata().await.is_none() {
                let metadata = self.generate_lazy_metadata().await?;
                self.write_lazy_metadata(metadata).await;
            }
        }
        Ok(())
    }

    async fn read_added_by(&self) -> Option<UserMetadata> {
        self.typemap().read().await.get::<UserMetadata>().cloned()
    }

    async fn write_added_by(&mut self, user_metadata: UserMetadata) {
        self.typemap().write().await.insert::<UserMetadata>(user_metadata)
    }

    fn is_lazy(&self) -> bool {
        self.metadata().date == Some("$lazy$".to_owned())
    }
}

pub struct LazyMetadataEventHandler {
    pub handler: Arc<Mutex<Call>>,
    pub channel_id: ChannelId,
    pub http: Arc<Http>
}

#[async_trait]
impl songbird::events::EventHandler for LazyMetadataEventHandler {
    async fn act(&self, ctx: &songbird::EventContext<'_>) -> Option<songbird::Event> {
        if let EventContext::Track(slice) = ctx {
            if let Some((track_state, _)) = slice.get(0) {
                if let Some(mut current_track) = self.handler.lock().await.queue().current() {
                    if track_state.play_time.as_secs() == 0 {
                        if let Ok(track_metadata) = current_track.read_awake_lazy_metadata().await {
                            if let Ok(message) = self.channel_id.send_message(&self.http, |msg| msg.set_embed(create_now_playing_embed(track_metadata))).await {
                                tokio::time::sleep(Duration::from_secs(10)).await;
                                message.delete(&self.http).await;
                            }
                        }
                    }
                }
            }
        }
        None
    }
}