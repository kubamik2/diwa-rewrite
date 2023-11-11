use google_youtube3::{ YouTube, hyper::client::HttpConnector, hyper_rustls::HttpsConnector, api::Video, hyper::Client, hyper_rustls::HttpsConnectorBuilder, oauth2 };
use std::str::FromStr;
use crate::{ VideoMetadata, AudioSource };
use crate::error::{ Error, AppError };
use thiserror::Error as ThisError;

pub struct YouTubeClient {
    client: YouTube<HttpsConnector<HttpConnector>>
}

#[derive(Debug, ThisError)]
pub enum YoutubeApiError {
    #[error("\"{value}\" object is missing")]
    MissingValue { value: String },
    #[error("could not parse the video duration string")]
    DurationString { duration_string: String },
    #[error("playlist is empty")]
    EmptyPlaylist
}

impl YouTubeClient {
    pub async fn new() -> Result<Self, Error> {
        let youtube_secret = oauth2::ServiceAccountKey {
            key_type: Some(std::env::var("YOUTUBE_KEY_TYPE").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_KEY_TYPE".to_owned() })?),
            project_id: Some(std::env::var("YOUTUBE_PROJECT_ID").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_PROJECT_ID".to_owned() })?),
            private_key_id: Some(std::env::var("YOUTUBE_PRIVATE_KEY_ID").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_PRIVATE_KEY_ID".to_owned() })?),
            private_key: std::env::var("YOUTUBE_PRIVATE_KEY").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_PRIVATE_KEY".to_owned() })?,
            client_email: std::env::var("YOUTUBE_CLIENT_EMAIL").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_CLIENT_EMAIL".to_owned() })?,
            client_id: Some(std::env::var("YOUTUBE_CLIENT_ID").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_CLIENT_ID".to_owned() })?),
            auth_uri: Some(std::env::var("YOUTUBE_AUTH_URI").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_AUTH_URI".to_owned() })?),
            token_uri: std::env::var("YOUTUBE_TOKEN_URI").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_TOKEN_URI".to_owned() })?,
            auth_provider_x509_cert_url: Some(std::env::var("YOUTUBE_AUTH_PROVIDER_X509_CERT_URL").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_AUTH_PROVIDER_X509_CERT_URL".to_owned() })?),
            client_x509_cert_url: Some(std::env::var("YOUTUBE_CLIENT_X509_CERT_URL").map_err(|_| AppError::MissingEnvEntry { entry: "YOUTUBE_CLIENT_X509_CERT_URL".to_owned() })?)
        };

        let youtube_auth = oauth2::ServiceAccountAuthenticator::builder(youtube_secret).build().await?;
        Ok(Self { client: YouTube::new(Client::builder().build(HttpsConnectorBuilder::new().with_native_roots().https_or_http().enable_http1().enable_http2().build()), youtube_auth) })
    }

    pub async fn video(&self, id: &str) -> Result<VideoMetadata, Error> {
        let items = self.client.videos()
            .list(&vec!["contentDetails".to_owned(), "snippet".to_owned()])
            .add_id(id)
            .doit().await?.1.items
            .ok_or(YoutubeApiError::MissingValue { value: "video.items".to_owned() })?;

        let video = items.first().ok_or(YoutubeApiError::MissingValue { value: "items[0]".to_owned() })?;
        video_to_metadata(video)
    }

    pub async fn playlist(&self, id: &str) -> Result<Vec<VideoMetadata>, Error> {
        let items = self.client.playlist_items()
            .list(&vec!["contentDetails".to_owned(), "snippet".to_owned()])
            .playlist_id(id)
            .max_results(50)
            .doit().await?.1.items
            .ok_or(YoutubeApiError::MissingValue { value: "playlist.items".to_owned() })?;

        if items.is_empty() { return Err(YoutubeApiError::EmptyPlaylist.into()); }

        let mut video_id_vec = vec![];
        for item in items {
            let video_id = item
                .content_details.ok_or(YoutubeApiError::MissingValue { value: "item.content_details".to_owned() })?
                .video_id.ok_or(YoutubeApiError::MissingValue { value: "content_details.video_id".to_owned() })?;
            video_id_vec.push(video_id);
        }

        let joined_video_ids = video_id_vec.join(",");
        let items = self.client.videos()
            .list(&vec!["contentDetails".to_owned(), "snippet".to_owned()])
            .add_id(&joined_video_ids)
            .doit().await?.1.items
            .ok_or(YoutubeApiError::MissingValue { value: "video.items".to_owned() })?;

        let metadata_vec = items.iter().map(|video| video_to_metadata(video)).collect::<Result<Vec<VideoMetadata>, Error>>()?;
        Ok(metadata_vec)
    }
}

fn video_to_metadata(video: &Video) -> Result<VideoMetadata, Error> {
    let content_details = video.content_details.as_ref().ok_or(YoutubeApiError::MissingValue { value: "content_details".to_owned() })?;
    let snippet = video.snippet.as_ref().ok_or(YoutubeApiError::MissingValue { value: "snippet".to_owned() })?;
    
    let title = snippet.title.as_ref().ok_or(YoutubeApiError::MissingValue { value: "title".to_owned() })?.clone();
    let video_id = video.id.as_ref().ok_or(YoutubeApiError::MissingValue { value: "id".to_owned() })?.clone();
    let duration_string = content_details.duration.as_ref().ok_or(YoutubeApiError::MissingValue { value: "duration".to_owned() })?.clone();
    
    let duration: std::time::Duration = iso8601::Duration::from_str(&duration_string).map_err(|_| YoutubeApiError::DurationString { duration_string })?.into();
    let audio_source = AudioSource::YouTube { video_id };

    Ok(VideoMetadata { title, duration, audio_source })
}