use google_youtube3::{ YouTube, hyper::client::HttpConnector, hyper_rustls::HttpsConnector, api::Video, hyper::Client, hyper_rustls::HttpsConnectorBuilder, oauth2 };
use std::str::FromStr;
use crate::{metadata::VideoMetadata, metadata::AudioSource};
use thiserror::Error as ThisError;

pub struct YouTubeClient {
    client: YouTube<HttpsConnector<HttpConnector>>
}

#[derive(Debug, ThisError)]
pub enum YouTubeError {
    #[error("")]
    MissingValue { value: String },
    #[error("")]
    DurationString { duration_string: String },
    #[error("")]
    EmptyPlaylist,
    #[error("")]
    Api(google_youtube3::Error),
    #[error("")]
    EnvVarsMissing {vars: Vec<String>},
    #[error("")]
    OAuth(std::io::Error),
}

impl From<google_youtube3::Error> for YouTubeError {
    fn from(value: google_youtube3::Error) -> Self {
        Self::Api(value)
    }
}

impl From<std::io::Error> for YouTubeError {
    fn from(value: std::io::Error) -> Self {
        Self::OAuth(value)
    }
}

impl YouTubeClient {
    pub async fn new() -> Result<Self, YouTubeError> {
        let youtube_secret = oauth2::ServiceAccountKey {
            key_type: Some(std::env::var("YOUTUBE_KEY_TYPE").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_KEY_TYPE".to_owned()] })?),
            project_id: Some(std::env::var("YOUTUBE_PROJECT_ID").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_PROJECT_ID".to_owned()] })?),
            private_key_id: Some(std::env::var("YOUTUBE_PRIVATE_KEY_ID").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_PRIVATE_KEY_ID".to_owned()] })?),
            private_key: std::env::var("YOUTUBE_PRIVATE_KEY").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_PRIVATE_KEY".to_owned()] })?,
            client_email: std::env::var("YOUTUBE_CLIENT_EMAIL").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_CLIENT_EMAIL".to_owned()] })?,
            client_id: Some(std::env::var("YOUTUBE_CLIENT_ID").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_CLIENT_ID".to_owned()] })?),
            auth_uri: Some(std::env::var("YOUTUBE_AUTH_URI").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_AUTH_URI".to_owned()] })?),
            token_uri: std::env::var("YOUTUBE_TOKEN_URI").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_TOKEN_URI".to_owned()] })?,
            auth_provider_x509_cert_url: Some(std::env::var("YOUTUBE_AUTH_PROVIDER_X509_CERT_URL").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_AUTH_PROVIDER_X509_CERT_URL".to_owned()] })?),
            client_x509_cert_url: Some(std::env::var("YOUTUBE_CLIENT_X509_CERT_URL").map_err(|_| YouTubeError::EnvVarsMissing { vars: vec!["YOUTUBE_CLIENT_X509_CERT_URL".to_owned()] })?)
        };

        let youtube_auth = oauth2::ServiceAccountAuthenticator::builder(youtube_secret).build().await?;
        Ok(Self { client: YouTube::new(Client::builder().build(HttpsConnectorBuilder::new().with_native_roots()?.https_or_http().enable_http1().enable_http2().build()), youtube_auth) })
    }

    pub async fn video(&self, id: &str) -> Result<VideoMetadata, YouTubeError> {
        let items = self.client.videos()
            .list(&vec!["contentDetails".to_owned(), "snippet".to_owned()])
            .add_id(id)
            .doit().await?.1.items
            .ok_or(YouTubeError::MissingValue { value: "video.items".to_owned() })?;

        let video = items.first().ok_or(YouTubeError::MissingValue { value: "items[0]".to_owned() })?;
        video_to_metadata(video)
    }

    pub async fn playlist(&self, id: &str) -> Result<Vec<VideoMetadata>, YouTubeError> {
        let items = self.client.playlist_items()
            .list(&vec!["contentDetails".to_owned(), "snippet".to_owned()])
            .playlist_id(id)
            .max_results(50)
            .doit().await?.1.items
            .ok_or(YouTubeError::MissingValue { value: "playlist.items".to_owned() })?;

        if items.is_empty() { return Err(YouTubeError::EmptyPlaylist.into()); }

        let mut video_id_vec = vec![];
        for item in items {
            let video_id = item
                .content_details.ok_or(YouTubeError::MissingValue { value: "item.content_details".to_owned() })?
                .video_id.ok_or(YouTubeError::MissingValue { value: "content_details.video_id".to_owned() })?;
            video_id_vec.push(video_id);
        }

        let joined_video_ids = video_id_vec.join(",");
        let items = self.client.videos()
            .list(&vec!["contentDetails".to_owned(), "snippet".to_owned()])
            .add_id(&joined_video_ids)
            .doit().await?.1.items
            .ok_or(YouTubeError::MissingValue { value: "video.items".to_owned() })?;

        let metadata_vec = items.iter().map(|video| video_to_metadata(video)).collect::<Result<Vec<VideoMetadata>, YouTubeError>>()?;
        Ok(metadata_vec)
    }
}

fn video_to_metadata(video: &Video) -> Result<VideoMetadata, YouTubeError> {
    let content_details = video.content_details.as_ref().ok_or(YouTubeError::MissingValue { value: "content_details".to_owned() })?;
    let snippet = video.snippet.as_ref().ok_or(YouTubeError::MissingValue { value: "snippet".to_owned() })?;
    
    let title = snippet.title.as_ref().ok_or(YouTubeError::MissingValue { value: "title".to_owned() })?.clone();
    let video_id = video.id.as_ref().ok_or(YouTubeError::MissingValue { value: "id".to_owned() })?.clone();
    let duration_string = content_details.duration.as_ref().ok_or(YouTubeError::MissingValue { value: "duration".to_owned() })?.clone();
    
    let duration: std::time::Duration = iso8601::Duration::from_str(&duration_string).map_err(|_| YouTubeError::DurationString { duration_string })?.into();
    let audio_source = AudioSource::YouTube { video_id };

    Ok(VideoMetadata { title, duration, audio_source })
}