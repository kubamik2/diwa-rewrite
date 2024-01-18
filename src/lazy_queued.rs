use serenity::async_trait;
use songbird::input::{Codec, Container, restartable::Restart, Input, Metadata as SongbirdMetadata};

use crate::{metadata::VideoMetadata, metadata::AudioSource, scrapers, stream_media_source};

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
                let AudioSource::YouTube { video_id } = metadata.audio_source else {panic!("youtube search returned non youtube audio source???")};

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
                        let _ = scrapers::jeja::tts_download(guild_id).await;

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