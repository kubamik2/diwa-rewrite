use std::{time::Duration, sync::Arc};

use crate::utils::{format_duration, create_now_playing_embed};
use serenity::{http::Http, builder::CreateMessage};
use songbird::{typemap::TypeMapKey, tracks::TrackHandle, Call, EventContext};
use poise::{ async_trait, serenity_prelude::{User, ChannelId} };
use tokio::sync::Mutex;
use thiserror::Error as ThisError;

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

impl VideoMetadata {
    pub fn to_queue_string(&self, playtime: Option<Duration>, limit: Option<usize>) -> String {
        let mut formatted_duration = format_duration(self.duration, None);
        if let Some(playtime) = playtime {
            let formatted_playtime = format_duration(playtime, Some(formatted_duration.len()));
            formatted_duration = format!("{} / {}", formatted_playtime, formatted_duration);
        }
        match &self.audio_source {
            AudioSource::YouTube { video_id } => {
                let mut queue_string = format!("[{}](https://youtu.be/{}) | {}", self.title, video_id, formatted_duration);

                if let Some(limit) = limit {
                    if queue_string.len() <= limit { return queue_string; }
                    let mut truncated_title = self.title.clone();
                    truncated_title.truncate(self.title.len()- (queue_string.len() - limit) - 3);
                    truncated_title.push_str("...");

                    queue_string = format!("[{}](https://youtu.be/{}) | {}", truncated_title, video_id, formatted_duration);
                }

                queue_string
            },
            AudioSource::File { path: _ } => {
                let mut queue_string = format!("{} | {}", self.title, formatted_duration);

                if let Some(limit) = limit {
                    if queue_string.len() <= limit { return queue_string; }
                    let mut truncated_title = self.title.clone();
                    truncated_title.truncate(self.title.len()- (queue_string.len() - limit) - 3);
                    truncated_title.push_str("...");
                    
                    queue_string = format!("{} | {}", self.title, formatted_duration);
                }

                queue_string
            },
            AudioSource::Jeja { .. } => {
                let queue_string = format!("{}", self.title);

                queue_string
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
        let id = value.id.get();
        let name = value.name.clone();
        let avatar_url = value.avatar_url();
        Self { id, name, avatar_url }
    }
}

impl From<&User> for UserMetadata {
    fn from(value: &User) -> Self {
        let id = value.id.get();
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
    async fn generate_lazy_metadata(&mut self) -> Result<TrackMetadata, MetadataError>;
    async fn read_generate_lazy_metadata(&mut self) -> Result<TrackMetadata, MetadataError>;
    async fn awake_lazy_metadata(&mut self) -> Result<(), MetadataError>;
    async fn read_added_by(&self) -> Option<UserMetadata>;
    async fn write_added_by(&mut self, user_metadata: UserMetadata);
    async fn is_awake(&self) -> bool;
    async fn read_query(&self) -> Option<String>;
    async fn write_query(&mut self, query: String);
}

#[derive(Debug, ThisError)]
pub enum MetadataError {
    #[error("")]
    MissingQuery,
    #[error("")]
    MissingAddedBy,
    #[error("")]
    YoutubeScrape(#[from] crate::scrapers::youtube::YoutubeScrapeError),
}

pub struct Query(pub String);

impl TypeMapKey for Query {
    type Value = Query;
}

#[async_trait]
impl LazyMetadata for TrackHandle {
    // reads metadata
    async fn read_lazy_metadata(&self) -> Option<TrackMetadata> {
        self.typemap().read().await.get::<TrackMetadata>().cloned()
    }

    // writes metadata
    async fn write_lazy_metadata(&mut self, metadata: TrackMetadata) {
        self.typemap().write().await.insert::<TrackMetadata>(metadata)
    }

    // generates metadata without writing it
    async fn generate_lazy_metadata(&mut self) -> Result<TrackMetadata, MetadataError> {
        let query = self.read_query().await.ok_or(MetadataError::MissingQuery)?;
        let added_by = self.read_added_by().await.ok_or(MetadataError::MissingAddedBy)?;
        let video_metadata = crate::scrapers::youtube::search(&query).await?;
        Ok(TrackMetadata { video_metadata, added_by })
    }

    // awakes and reads metadata
    async fn read_generate_lazy_metadata(&mut self) -> Result<TrackMetadata, MetadataError> {
        match self.read_lazy_metadata().await {
            Some(metadata) => Ok(metadata),
            None => {
                let track_metadata = self.generate_lazy_metadata().await?;
                self.write_lazy_metadata(track_metadata.clone()).await;
                Ok(track_metadata)
            }
        }
    }

    // generates and writes metadata
    async fn awake_lazy_metadata(&mut self) -> Result<(), MetadataError> {
        if !self.is_awake().await {
            let metadata = self.generate_lazy_metadata().await?;
            self.write_lazy_metadata(metadata).await;
            self.typemap().write().await.remove::<Query>();
        }
        Ok(())
    }

    async fn read_added_by(&self) -> Option<UserMetadata> {
        self.typemap().read().await.get::<UserMetadata>().cloned()
    }

    async fn write_added_by(&mut self, user_metadata: UserMetadata) {
        self.typemap().write().await.insert::<UserMetadata>(user_metadata)
    }

    // check whether the query is present in the typemap
    async fn is_awake(&self) -> bool {
        !self.typemap().read().await.contains_key::<Query>()
    }

    async fn read_query(&self) -> Option<String> {
        self.typemap().read().await.get::<Query>().map(|query| query.0.clone())
    }

    async fn write_query(&mut self, query: String) {
        self.typemap().write().await.insert::<Query>(Query(query))
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
        let EventContext::Track(slice) = ctx else { return None; };
        let Some((track_state, _)) = slice.get(0) else { return None; };
        let Some(mut current_track) = ({ let handler_guard = self.handler.lock().await; handler_guard.queue().current() }) else { return None; }; // have to do this monstrosity to avoid mutex dead locking
       
        if track_state.play_time.as_secs() != 0 { return None; } ;

        let Ok(track_metadata) = current_track.read_generate_lazy_metadata().await else { return None; };

        let Ok(message) = self.channel_id.send_message(&self.http, CreateMessage::new().embed(create_now_playing_embed(track_metadata))).await else { return None; };
        
        tokio::time::sleep(Duration::from_secs(10)).await;
        let _ = message.delete(&self.http).await;
        
        None
    }
}

#[derive(Debug, Clone, Hash)]
pub enum AudioSource {
    YouTube { video_id: String },
    File { path: std::path::PathBuf },
    Jeja { filename: String }
}