use crate::{metadata::{AudioSource, TrackMetadata, UserMetadata, VideoMetadata}, api_integration::{spotify::{SpotifyClient, SpotifyError}, youtube::{YouTubeClient, YouTubeError}}};
use reqwest::Url;
use songbird::input::Input;
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
    pub query: String,
    pub added_by: UserMetadata
}

#[derive(Debug, ThisError)]
pub enum ConversionError {
    #[error("{0}")]
    Youtube(#[from] YouTubeError),
    #[error("{0}")]
    Spotify(#[from] SpotifyError),
    #[error("{0}")]
    MediaType(#[from] MediaTypeError),
    #[error("{0}")]
    YoutubeScrape(#[from] crate::scrapers::youtube::YoutubeScrapeError),
    #[error("")]
    RustyYtdl(#[from] rusty_ytdl::VideoError),
    #[error("")]
    NoVideoFormat
}

pub enum YouTubeComposer {
    Query { query: String, client: reqwest::Client },
    Metadata { metadata: VideoMetadata, client: reqwest::Client  }
}

async fn find_video_format(video_id: String) -> Result<String, ConversionError> {
    let video = rusty_ytdl::Video::new(video_id)?;
    let video_basic_info = video.get_basic_info().await?;
    
    let mut stream_url = None;
    let mut desired_audio_quality_num = 0;
    for video_format in video_basic_info.formats.into_iter().filter(|p| p.has_audio && !p.has_video) {//video_basic_info.formats.iter().filter(|p| p.mime_type.codecs.contains(&"opus".to_string())) {
        if let Some(audio_quality) = &video_format.audio_quality {
            let audio_quality_num = match audio_quality.as_str() {
                "AUDIO_QUALITY_HIGH" => 2,
                "AUDIO_QUALITY_MEDIUM" => 3,
                "AUDIO_QUALITY_MIN" => 1,
                _ => 0
            };

            if audio_quality_num > desired_audio_quality_num {
                stream_url = Some(video_format.url.clone());
                desired_audio_quality_num = audio_quality_num;
                if audio_quality_num == 3 { break; }
            }
        }
    }
    if stream_url.is_none() { log::warn!("find_video_format stream_url is None")}
    Ok(stream_url.ok_or(ConversionError::NoVideoFormat)?)
}

#[serenity::async_trait]
impl songbird::input::Compose for YouTubeComposer {
    fn create(&mut self) -> Result<songbird::input::AudioStream<Box<dyn symphonia::core::io::MediaSource> > ,songbird::input::AudioStreamError> {
        Err(songbird::input::AudioStreamError::Unsupported)
    }

    async fn create_async(&mut self) -> Result<songbird::input::AudioStream<Box<dyn symphonia::core::io::MediaSource> > ,songbird::input::AudioStreamError> {
        match self {
            Self::Query { query, client } => {
                match crate::scrapers::youtube::search(&query).await {
                    Ok(video_metadata) => {
                        let crate::metadata::AudioSource::YouTube { video_id } = video_metadata.audio_source else { panic!("youtube search returned non youtube source") };
                        match find_video_format(video_id).await {
                            Ok(url) => {
                                let mut http_request = songbird::input::HttpRequest::new(client.clone(), url);
                                http_request.create_async().await
                            },
                            Err(err) => {
                                Err(songbird::input::AudioStreamError::Fail(err.into()))
                            }
                        }
                    },
                    Err(err) => Err(songbird::input::AudioStreamError::Fail(err.into()))
                }
            },
            Self::Metadata { metadata, client } => {
                match metadata.audio_source.clone() {
                    AudioSource::YouTube { video_id } => {
                        match find_video_format(video_id.clone()).await {
                            Ok(url) => {
                                let mut headers = reqwest::header::HeaderMap::new();
                                headers.insert(reqwest::header::USER_AGENT, "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:129.0) Gecko/20100101 Firefox/129.0".parse().unwrap());
                                headers.insert(reqwest::header::ACCEPT, "image/avif,image/webp,image/png,image/svg+xml,image/*;q=0.8,*/*;q=0.5".parse().unwrap());
                                headers.insert(reqwest::header::CONNECTION, "keep-alive".parse().unwrap());
                                headers.insert(reqwest::header::ACCEPT_ENCODING, "gzip, deflate, br, zstd".parse().unwrap());
                                headers.insert(reqwest::header::ACCEPT_LANGUAGE, "pl,en-US;q=0.7,en;q=0.3".parse().unwrap());
                                let mut http_request = songbird::input::HttpRequest::new_with_headers(client.clone(), url, headers);
                                http_request.create_async().await
                            },
                            Err(err) => {
                                Err(songbird::input::AudioStreamError::Fail(err.into()))
                            }
                        }
                    },
                    AudioSource::File { path } => {
                        songbird::input::File::new(path).create_async().await
                    },
                    AudioSource::Jeja { filename } => {
                        crate::scrapers::jeja::tts_download(&filename, client.clone()).await
                            .map_err(|err| songbird::input::AudioStreamError::Fail(err.into()))?;
                        songbird::input::File::new(filename).create_async().await
                    }
                }
            }
        }
    }

    async fn aux_metadata(&mut self) -> Result<songbird::input::AuxMetadata,songbird::input::AudioStreamError> {
        Err(songbird::input::AudioStreamError::Unsupported)
    }

    fn should_create_async(&self) -> bool {
        true
    }
}

impl Into<Input> for YouTubeComposer {
    fn into(self) -> Input {
        Input::Lazy(Box::new(self))
    }
}

pub async fn convert_query(youtube_client: &YouTubeClient, spotify_client: &SpotifyClient, query: &str, added_by: UserMetadata, client: reqwest::Client) -> Result<ConvertedQuery, ConversionError> {
    return Ok(match extract_media_type(query)? {
        MediaType::YouTubeVideo { video_id } => {
            let video_metadata = youtube_client.video(&video_id).await?;
            let input = YouTubeComposer::Metadata { metadata: video_metadata.clone(), client }.into();
            let track_metadata = TrackMetadata { video_metadata, added_by };
            ConvertedQuery::LiveVideo(MetaInput { input, track_metadata })
        },
        MediaType::YouTubePlaylist { playlist_id } => {
            let playlist_video_metadata = youtube_client.playlist(&playlist_id).await?;
            let mut metainputs = vec![];
            for video_metadata in playlist_video_metadata {
                let input = YouTubeComposer::Metadata { metadata: video_metadata.clone(), client: client.clone() }.into();
                let track_metadata = TrackMetadata { video_metadata, added_by: added_by.clone() };
                let metainput = MetaInput { input, track_metadata };
                metainputs.push(metainput);
            }
            ConvertedQuery::LivePlaylist(metainputs)
        },
        MediaType::SpotifyTrack { track_id } => {
            let track_data = spotify_client.track(&track_id)?;
            let video_metadata = crate::scrapers::youtube::search(&format!("{} by {}", track_data.title, track_data.artists.join(", "))).await?;
            let input = YouTubeComposer::Metadata { metadata: video_metadata.clone(), client }.into();
            let track_metadata = TrackMetadata { video_metadata, added_by };
            ConvertedQuery::LiveVideo(MetaInput { input, track_metadata })
        },
        MediaType::SpotifyPlaylist { playlist_id } => {
            let playlist_data = spotify_client.playlist(&playlist_id)?;
            let mut metainputs = vec![];
            for track_data in playlist_data {
                let query = format!("{} by {}", track_data.title, track_data.artists.join(", "));
                let input = YouTubeComposer::Query { query: query.clone(), client: client.clone() }.into();
                let metainput = PendingMetaInput { input, added_by: added_by.clone(), query };
                metainputs.push(metainput);
            }
            ConvertedQuery::PendingPlaylist(metainputs)
        },
        MediaType::SpotifyAlbum { album_id } => {
            let album_data = spotify_client.album(&album_id)?;
            let mut metainputs = vec![];
            for track_data in album_data {
                let query = format!("{} by {}", track_data.title, track_data.artists.join(", "));
                let input = YouTubeComposer::Query { query: query.clone(), client: client.clone() }.into();
                let metainput = PendingMetaInput { input, added_by: added_by.clone(), query };
                metainputs.push(metainput);
            }
            ConvertedQuery::PendingPlaylist(metainputs)
        },
        MediaType::Search { query } => {
            let video_metadata = crate::scrapers::youtube::search(&query).await?;
            let input = YouTubeComposer::Metadata { metadata: video_metadata.clone(), client }.into();
            let track_metadata = TrackMetadata { video_metadata, added_by };
            ConvertedQuery::LiveVideo(MetaInput { input, track_metadata })
        }
    });
}