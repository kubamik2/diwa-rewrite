use crate::{metadata::{TrackMetadata, UserMetadata}, lazy_queued::LazyQueued, api_integration::{spotify::{SpotifyClient, SpotifyError}, youtube::{YouTubeClient, YoutubeError}}};
use reqwest::Url;
use songbird::input::{Restartable, Input};
use thiserror::Error as ThisError;

#[derive(Debug)]
pub enum MediaType {
    YouTubeVideo { video_id: String },
    YouTubePlaylist { playlist_id: String },
    SpotifyTrack { track_id: String },
    SpotifyPlaylist { playlist_id: String },
    SpotifyAlbum { album_id: String },
    Search { query: String }
}

#[derive(Debug, ThisError)]
pub enum MediaTypeError {
    #[error("")]
    UnsupportedDomain { domain: String },
    #[error("")]
    UrlYouTubeLongInvalid { url: String },
    #[error("")]
    UrlYouTubeShortInvalid { url: String },
    #[error("")]
    UrlSpotifyArgumentsInvalid { url: String },
    #[error("")]
    UrlSpotifyContentTypeMissing { url: String },
    #[error("")]
    UrlSpotifyIdMissing { url: String },
    #[error("")]
    UrlSpotifyContentTypeInvalid { url: String },
    #[error("")]
    EpisodesUnsupported,
    #[error("")]
    DomainMissing
}

pub fn extract_media_type(query: &str) -> Result<MediaType, MediaTypeError> {
    match Url::parse(query) {
        Ok(url) => {
            let domain = url.domain().ok_or(MediaTypeError::DomainMissing)?;
            match domain {
                "www.youtube.com" | "youtube.com" => {
                    if let Some(video_id) = url.query_pairs().into_owned().find(|p| p.0 == "v").map(|f| f.1){
                        return Ok(MediaType::YouTubeVideo { video_id });
                    } else if let Some(playlist_id) = url.query_pairs().into_owned().find(|p| p.0 == "list").map(|f| f.1) {
                        return Ok(MediaType::YouTubePlaylist { playlist_id });
                    }
                    return Err(MediaTypeError::UrlYouTubeLongInvalid { url: url.to_string() }.into());
                },
                "www.youtu.be" | "youtu.be" => {
                    if let Some(video_id) = url.path().strip_prefix("/").map(|f| f.to_owned()) {
                        return Ok(MediaType::YouTubeVideo { video_id });
                    }
                    return Err(MediaTypeError::UrlYouTubeShortInvalid { url: url.to_string() }.into());
                },
                "open.spotify.com" | "www.open.spotify.com" => {
                    let argumets = url.path_segments().map(|f| f.collect::<Vec<&str>>()).ok_or(MediaTypeError::UrlSpotifyArgumentsInvalid { url: url.to_string() })?;
                    let content_type = argumets.get(0).ok_or(MediaTypeError::UrlSpotifyContentTypeMissing { url: url.to_string() })?;
                    let id = argumets.get(1).ok_or(MediaTypeError::UrlSpotifyIdMissing { url: url.to_string() })?.to_string();

                    return Ok(match *content_type {
                        "track" => MediaType::SpotifyTrack { track_id: id },
                        "playlist" => MediaType::SpotifyPlaylist { playlist_id: id },
                        "album" => MediaType::SpotifyAlbum { album_id: id },
                        "episode" => return Err(MediaTypeError::EpisodesUnsupported.into()),
                        _ => return Err(MediaTypeError::UrlSpotifyContentTypeInvalid { url: url.to_string() }.into())
                    });
                }
                _ => return Err(MediaTypeError::UnsupportedDomain { domain: domain.to_owned() }.into())
            }
        },
        Err(_) => {
            return Ok(MediaType::Search { query: query.to_owned() });
        }
    }
}

pub enum ConvertedQuery {
    LiveVideo(MetaInput),
    LivePlaylist(Vec<MetaInput>),
    PendingPlaylist(Vec<PendingMetaInput>)
}

pub struct MetaInput {
    pub input: Input,
    pub track_metadata: TrackMetadata
}

pub struct PendingMetaInput {
    pub input: Input,
    pub added_by: UserMetadata
}

#[derive(Debug, ThisError)]
pub enum ConversionError {
    #[error("{0}")]
    Youtube(#[from] YoutubeError),
    #[error("{0}")]
    Spotify(#[from] SpotifyError),
    #[error("{0}")]
    MediaType(#[from] MediaTypeError),
    #[error("")]
    Input(#[from] songbird::input::error::Error),
    #[error("{0}")]
    YoutubeScrape(#[from] crate::scrapers::youtube::YoutubeScrapeError)
    
}

pub async fn convert_query(youtube_client: &YouTubeClient, spotify_client: &SpotifyClient, query: &str, added_by: UserMetadata) -> Result<ConvertedQuery, ConversionError> {
    return Ok(match extract_media_type(query)? {
        MediaType::YouTubeVideo { video_id } => {
            let video_metadata = youtube_client.video(&video_id).await?;
            let restartable = Restartable::new(LazyQueued::Metadata { metadata: video_metadata.clone() }, true).await?;
            let track_metadata = TrackMetadata { video_metadata, added_by };
            ConvertedQuery::LiveVideo(MetaInput { input: restartable.into(), track_metadata })
        },
        MediaType::YouTubePlaylist { playlist_id } => {
            let playlist_video_metadata = youtube_client.playlist(&playlist_id).await?;
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
            let track_data = spotify_client.track(&track_id)?;
            let video_metadata = crate::scrapers::youtube::search(&format!("{} by {}", track_data.title, track_data.artists.join(", "))).await?;
            let restartable = Restartable::new(LazyQueued::Metadata { metadata: video_metadata.clone() }, true).await?;
            let track_metadata = TrackMetadata { video_metadata, added_by };
            ConvertedQuery::LiveVideo(MetaInput { input: restartable.into(), track_metadata })
        },
        MediaType::SpotifyPlaylist { playlist_id } => {
            let playlist_data = spotify_client.playlist(&playlist_id)?;
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
            let album_data = spotify_client.album(&album_id)?;
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