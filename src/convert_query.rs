use crate::error::{Error, AppError};
use reqwest::Url;
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
pub enum ConversionError {
    #[error("\"{domain}\" domain is not supported")]
    UnsupportedDomain { domain: String },
    #[error("invalid youtube longform url format")]
    UrlYouTubeLongInvalid { url: String },
    #[error("invalid youtube shortform url format")]
    UrlYouTubeShortInvalid { url: String },
    #[error("invalid spotify url")]
    UrlSpotifyArgumentsInvalid { url: String },
    #[error("content type missing in spotify url")]
    UrlSpotifyContentTypeMissing { url: String },
    #[error("id missing in spotify url")]
    UrlSpotifyIdMissing { url: String },
    #[error("invalid content type in spotify url")]
    UrlSpotifyContentTypeInvalid { url: String }
}

pub fn extract_media_type(query: &str) -> Result<MediaType, Error> {
    match Url::parse(query) {
        Ok(url) => {
            let domain = url.domain().ok_or(AppError::MissingValue { value: "domain".to_owned() })?;
            match domain {
                "www.youtube.com" | "youtube.com" => {
                    if let Some(video_id) = url.query_pairs().into_owned().find(|p| p.0 == "v").map(|f| f.1){
                        return Ok(MediaType::YouTubeVideo { video_id });
                    } else if let Some(playlist_id) = url.query_pairs().into_owned().find(|p| p.0 == "list").map(|f| f.1) {
                        return Ok(MediaType::YouTubePlaylist { playlist_id });
                    }
                    return Err(ConversionError::UrlYouTubeLongInvalid { url: url.to_string() }.into());
                },
                "www.youtu.be" | "youtu.be" => {
                    if let Some(video_id) = url.path().strip_prefix("/").map(|f| f.to_owned()) {
                        return Ok(MediaType::YouTubeVideo { video_id });
                    }
                    return Err(ConversionError::UrlYouTubeShortInvalid { url: url.to_string() }.into());
                },
                "open.spotify.com" | "www.open.spotify.com" => {
                    let argumets = url.path_segments().map(|f| f.collect::<Vec<&str>>()).ok_or(ConversionError::UrlSpotifyArgumentsInvalid { url: url.to_string() })?;
                    let content_type = argumets.get(0).ok_or(ConversionError::UrlSpotifyContentTypeMissing { url: url.to_string() })?;
                    let id = argumets.get(1).ok_or(ConversionError::UrlSpotifyIdMissing { url: url.to_string() })?.to_string();

                    return Ok(match *content_type {
                        "track" => MediaType::SpotifyTrack { track_id: id },
                        "playlist" => MediaType::SpotifyPlaylist { playlist_id: id },
                        "album" => MediaType::SpotifyAlbum { album_id: id },
                        _ => return Err(ConversionError::UrlSpotifyContentTypeInvalid { url: url.to_string() }.into())
                    });
                }
                _ => return Err(ConversionError::UnsupportedDomain { domain: domain.to_owned() }.into())
            }
        },
        Err(_) => {
            return Ok(MediaType::Search { query: query.to_owned() });
        }
    }
}