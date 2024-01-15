pub mod error;
pub mod api_integration;
pub mod scrapers;
pub mod convert_query;
pub mod metadata;
pub mod utils;
mod stream_media_source;

use poise::ReplyHandle;
use serenity::{async_trait, model::channel::Message};
use songbird::input::{ Input, restartable::Restart, Restartable };
use error::Error;
use convert_query::MediaType;
use songbird::input::{Metadata as SongbirdMetadata, Codec, Container};
use tokio::{sync::Mutex, task::AbortHandle};
use metadata::{ TrackMetadata, UserMetadata, VideoMetadata };
use std::collections::HashMap;

pub type Context<'a> = poise::Context<'a, Data, error::Error>;
pub struct Data {
    pub cleanups: Mutex<Vec<Cleanup>>,
    pub spotify_client: api_integration::spotify::SpotifyClient,
    pub youtube_client: api_integration::youtube::YouTubeClient,
    pub afk_timeout_abort_handle_map: Mutex<HashMap<u64, AbortHandle>>
}

#[derive(Clone)]
pub struct Cleanup {
    pub message: Message,
    pub delay: std::time::Duration
}

#[derive(Debug, Clone, Hash)]
pub enum AudioSource {
    YouTube { video_id: String },
    File { path: std::path::PathBuf },
    Jeja { guild_id: u64}
}

pub struct MetaInput {
    pub input: Input,
    pub track_metadata: TrackMetadata
}

pub struct PendingMetaInput {
    pub input: Input,
    pub added_by: UserMetadata
}

pub enum ConvertedQuery {
    LiveVideo(MetaInput),
    LivePlaylist(Vec<MetaInput>),
    PendingPlaylist(Vec<PendingMetaInput>)
}

impl Data {
    pub fn new(spotify_client: api_integration::spotify::SpotifyClient, youtube_client: api_integration::youtube::YouTubeClient) -> Self {
        Self { cleanups: Mutex::new(vec![]), spotify_client, youtube_client, afk_timeout_abort_handle_map: Mutex::new(HashMap::new()) }
    }

    pub async fn convert_query(&self, query: &str, added_by: UserMetadata) -> Result<ConvertedQuery, Error> {
        return Ok(match convert_query::extract_media_type(query)? {
            MediaType::YouTubeVideo { video_id } => {
                let video_metadata = self.youtube_client.video(&video_id).await?;
                let restartable = Restartable::new(LazyQueued::Metadata { metadata: video_metadata.clone() }, true).await?;
                let track_metadata = TrackMetadata { video_metadata, added_by };
                ConvertedQuery::LiveVideo(MetaInput { input: restartable.into(), track_metadata })
            },
            MediaType::YouTubePlaylist { playlist_id } => {
                let playlist_video_metadata = self.youtube_client.playlist(&playlist_id).await?;
                let mut metainputs = vec![];
                for video_metadata in playlist_video_metadata {
                    let restartable = Restartable::new(LazyQueued::Metadata { metadata: video_metadata.clone() }, true).await?;
                    let track_metadata = TrackMetadata { video_metadata, added_by: added_by.clone() };
                    let metainput = MetaInput { input: restartable.into(), track_metadata };
                    metainputs.push(metainput);
                }
                ConvertedQuery::LivePlaylist(metainputs)
            },
            MediaType::SpotifyTrack { track_id } => {
                let track_data = self.spotify_client.track(&track_id)?;
                let video_metadata = crate::scrapers::youtube::search(&format!("{} by {}", track_data.title, track_data.artists.join(", "))).await?;
                let restartable = Restartable::new(LazyQueued::Metadata { metadata: video_metadata.clone() }, true).await?;
                let track_metadata = TrackMetadata { video_metadata, added_by };
                ConvertedQuery::LiveVideo(MetaInput { input: restartable.into(), track_metadata })
            },
            MediaType::SpotifyPlaylist { playlist_id } => {
                let playlist_data = self.spotify_client.playlist(&playlist_id)?;
                let mut metainputs = vec![];
                for track_data in playlist_data {
                    let query = format!("{} by {}", track_data.title, track_data.artists.join(", "));
                    let restartable = Restartable::new(LazyQueued::Query { query }, true).await?;
                    let metainput = PendingMetaInput { input: restartable.into(), added_by: added_by.clone() };
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
                    let metainput = PendingMetaInput { input: restartable.into(), added_by: added_by.clone() };
                    metainputs.push(metainput);
                }
                ConvertedQuery::PendingPlaylist(metainputs)
            },
            MediaType::Search { query } => {
                let video_metadata = crate::scrapers::youtube::search(&query).await?;
                let restartable = Restartable::new(LazyQueued::Metadata { metadata: video_metadata.clone() }, true).await?;
                let track_metadata = TrackMetadata { video_metadata, added_by };
                ConvertedQuery::LiveVideo(MetaInput { input: restartable.into(), track_metadata })
            }
        });
    }

    pub async fn add_to_cleanup<'a>(&self, reply_handle: ReplyHandle<'a>, delay: std::time::Duration) {
        if let Ok(message) = reply_handle.into_message().await {
            self.cleanups.lock().await.push(Cleanup { message, delay});
        }
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
                    },
                    AudioSource::Jeja { guild_id } => {
                        scrapers::jeja::tts_download(guild_id).await;

                        return songbird::ffmpeg(format!("{}.mp4", guild_id)).await;
                    }
                }
            }
        }
    }

    async fn lazy_init(&mut self) -> songbird::input::error::Result<(Option<SongbirdMetadata>, Codec, Container)> {
        Ok(match self {
            LazyQueued::Metadata { metadata } => {
                match metadata.audio_source {
                    AudioSource::Jeja { .. } => {
                        (Some(SongbirdMetadata {
                            channels: Some(1),
                            sample_rate: Some(24000),
                            title: Some(metadata.title.clone()),
                            ..Default::default()
                        }), Codec::FloatPcm, Container::Raw)
                    },
                    _ => (Some(metadata.clone().into()), Codec::FloatPcm, Container::Raw)
                }
                
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