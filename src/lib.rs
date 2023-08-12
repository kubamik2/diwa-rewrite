pub mod error;
pub mod api_integration;
pub mod scrapers;
pub mod convert_query;
mod stream_media_source;

use poise::serenity_prelude::UserId;
use serenity::{async_trait, prelude::TypeMapKey};
use songbird::{input::{ Input, restartable::Restart, Restartable }, tracks::TrackHandle};
use error::{ Error, AppError };
use convert_query::MediaType;
use songbird::input::{Metadata as SongbirdMetadata, Codec, Container};

pub type Context<'a> = poise::Context<'a, Data, error::Error>;
pub struct Data {
    pub spotify_client: api_integration::spotify::SpotifyClient,
    pub youtube_client: api_integration::youtube::YouTubeClient
}

#[derive(Debug, Clone, Hash)]
pub enum AudioSource {
    YouTube { video_id: String },
    File { path: std::path::PathBuf }
}

#[derive(Debug, Clone, Hash)]
pub struct VideoMetadata {
    pub title: String,
    pub duration: std::time::Duration,
    pub audio_source: AudioSource,
    pub added_by: Option<UserId>
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

pub struct MetaInput {
    pub input: Input,
    pub metadata: VideoMetadata
}

pub struct PendingMetaInput {
    pub input: Input,
    pub user_id: Option<UserId>
}

pub enum ConvertedQuery {
    LiveVideo(MetaInput),
    LivePlaylist(Vec<MetaInput>),
    PendingPlaylist(Vec<PendingMetaInput>)
}

impl Data {
    pub async fn convert_query(&self, query: &str, user_id: Option<UserId>) -> Result<ConvertedQuery, Error> {
        return Ok(match convert_query::extract_media_type(query)? {
            MediaType::YouTubeVideo { video_id } => {
                let mut metadata = self.youtube_client.video(&video_id).await?;
                metadata.added_by = user_id;
                let restartable = Restartable::new(LazyQueued::Metadata { metadata: metadata.clone() }, true).await?;
                ConvertedQuery::LiveVideo(MetaInput { input: restartable.into(), metadata })
            },
            MediaType::YouTubePlaylist { playlist_id } => {
                let playlist_metadata = self.youtube_client.playlist(&playlist_id).await?;
                let mut metainputs = vec![];
                for mut metadata in playlist_metadata {
                    metadata.added_by = user_id;
                    let restartable = Restartable::new(LazyQueued::Metadata { metadata: metadata.clone() }, true).await?;
                    let metainput = MetaInput { input: restartable.into(), metadata };
                    metainputs.push(metainput);
                }
                ConvertedQuery::LivePlaylist(metainputs)
            },
            MediaType::SpotifyTrack { track_id } => {
                let track_data = self.spotify_client.track(&track_id)?;
                let mut metadata = crate::scrapers::youtube::search(&format!("{} by {}", track_data.title, track_data.artists.join(", "))).await?;
                metadata.added_by = user_id;
                let restartable = Restartable::new(LazyQueued::Metadata { metadata: metadata.clone() }, true).await?;
                ConvertedQuery::LiveVideo(MetaInput { input: restartable.into(), metadata })
            },
            MediaType::SpotifyPlaylist { playlist_id } => {
                let playlist_data = self.spotify_client.playlist(&playlist_id)?;
                let mut metainputs = vec![];
                for track_data in playlist_data {
                    let query = format!("{} by {}", track_data.title, track_data.artists.join(", "));
                    let restartable = Restartable::new(LazyQueued::Query { query }, true).await?;
                    let metainput = PendingMetaInput { input: restartable.into(), user_id };
                    metainputs.push(metainput);
                }
                ConvertedQuery::PendingPlaylist(metainputs)
            },
            MediaType::SpotifyAlbum { album_id } => {
                let album_data = self.spotify_client.album(&album_id)?;
                let mut metainputs = vec![];
                for track_data in album_data {
                    let query = format!("{} by {}", track_data.title, track_data.artists.join(", "));
                    let restartable = Restartable::new(LazyQueued::Query { query }, true).await?;
                    let metainput = PendingMetaInput { input: restartable.into(), user_id };
                    metainputs.push(metainput);
                }
                ConvertedQuery::PendingPlaylist(metainputs)
            },
            MediaType::Search { query } => {
                let mut metadata = crate::scrapers::youtube::search(&query).await?;
                metadata.added_by = user_id;
                let restartable = Restartable::new(LazyQueued::Metadata { metadata: metadata.clone() }, true).await?;
                ConvertedQuery::LiveVideo(MetaInput { input: restartable.into(), metadata })
            }
        });
    }
}

pub enum LazyQueued {
    Metadata { metadata: VideoMetadata },
    Query { query: String }
}

#[async_trait]
impl Restart for LazyQueued {
    async fn call_restart(&mut self, _: Option<std::time::Duration>) -> songbird::input::error::Result<Input> {
        match self {
            LazyQueued::Query { query } => {
                let metadata = scrapers::youtube::search(query).await.map_err(|_| songbird::input::error::Error::Streams)?;
                let video_id = if let AudioSource::YouTube { video_id } = metadata.audio_source { video_id } 
                    else { panic!("youtube search returned non youtube audio source???") };

                let media = stream_media_source::StreamMediaSource::new(&video_id).await.map_err(|_| songbird::input::error::Error::Streams)?;
                return Ok(Input::new(true, songbird::input::Reader::Extension(Box::new(media)), songbird::input::Codec::FloatPcm, songbird::input::Container::Raw, None));
            },
            LazyQueued::Metadata { metadata } => {
                match metadata.audio_source {
                    AudioSource::YouTube { ref video_id } => {
                        let media = stream_media_source::StreamMediaSource::new(video_id).await.map_err(|_| songbird::input::error::Error::Streams)?;
                        return Ok(Input::new(true, songbird::input::Reader::Extension(Box::new(media)), songbird::input::Codec::FloatPcm, songbird::input::Container::Raw, None));
                    },
                    AudioSource::File { ref path } => {
                        return songbird::ffmpeg(path).await;
                    }
                }
            }
        }
    }

    async fn lazy_init(&mut self) -> songbird::input::error::Result<(Option<SongbirdMetadata>, Codec, Container)> {
        Ok(match self {
            LazyQueued::Metadata { metadata } => {
                (Some(metadata.clone().into()), Codec::FloatPcm, Container::Raw)
            },
            LazyQueued::Query { query } => {
                (SongbirdMetadata {
                    channels: Some(2),
                    sample_rate: Some(48000),
                    title: Some(query.clone()),
                    date: Some("$lazy$".to_owned()),
                    ..Default::default()
                }.clone().into(),
                Codec::FloatPcm, Container::Raw)
            }
        })
    }
}

#[async_trait]
pub trait LazyMetadata {
    async fn read_lazy_metadata(&self) -> Option<VideoMetadata>;
    async fn write_lazy_metadata(&mut self, metadata: VideoMetadata);
    async fn generate_lazy_metadata(&mut self) -> Result<VideoMetadata, Error>;
    fn is_lazy(&self) -> bool;
}

impl TypeMapKey for VideoMetadata {
    type Value = VideoMetadata;
}

#[async_trait]
impl LazyMetadata for TrackHandle {
    async fn read_lazy_metadata(&self) -> Option<VideoMetadata> {
        self.typemap().read().await.get::<VideoMetadata>().cloned()
    }

    async fn write_lazy_metadata(&mut self, metadata: VideoMetadata) {
        self.typemap().write().await.insert::<VideoMetadata>(metadata)
    }

    async fn generate_lazy_metadata(&mut self) -> Result<VideoMetadata, Error> {
        let title = self.metadata().title.as_ref().ok_or(AppError::MissingValue { value: "metadata.title".to_owned() })?;
        scrapers::youtube::search(title).await
    }

    fn is_lazy(&self) -> bool {
        self.metadata().date == Some("$lazy$".to_owned())
    }
}