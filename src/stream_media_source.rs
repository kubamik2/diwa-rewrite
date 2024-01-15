use std::io::{ Write, Read, Seek, Cursor };
use std::process::{ Stdio, Command };
use std::error::Error;
use symphonia::core::io::MediaSource;
use thiserror::Error as ThisError;

static FFMPEG_ARGS: [&'static str; 9] = [
            "-f",
            "s16le",
            "-ac",
            "2",
            "-ar",
            "48000",
            "-acodec",
            "pcm_f32le",
            "-",
        ];

pub struct StreamMediaSource {
    buffer: Cursor<Vec<u8>>,
    piped_stream_reader: PipedStreamReader,
    position: u64,
    end: u64,
}

#[derive(ThisError, Debug, Clone, PartialEq, Eq)]
pub enum StreamError {
    #[error("Video formats vector is empty or no audio quality was recognized")]
    NoVideoFormats
}

async fn find_highest_quality_video(video_id: &str) -> Result<String, Box<dyn Error>> {
    let video = rusty_ytdl::Video::new(video_id)?;
    let video_basic_info = video.get_basic_info().await?;
    
    let mut stream_url = None;
    let mut max_audio_quality_num = 0;
    for video_format in video_basic_info.formats.iter() {
        if let Some(audio_quality) = &video_format.audio_quality {
            let audio_quality_num = match audio_quality.as_str() {
                "AUDIO_QUALITY_HIGH" => 3,
                "AUDIO_QUALITY_MEDIUM" => 2,
                "AUDIO_QUALITY_MIN" => 1,
                _ => 0
            };

            if audio_quality_num != 0 && audio_quality_num > max_audio_quality_num {
                stream_url = Some(video_format.url.clone());
                max_audio_quality_num = audio_quality_num;
                if audio_quality_num == 3 { break; }
            }
        }
    }

    stream_url.ok_or(StreamError::NoVideoFormats.into())
}

impl StreamMediaSource {
    pub async fn new(video_id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let audio_stream_url = find_highest_quality_video(video_id).await?;

        let child = Command::new("ffmpeg")
            .args(["-i", audio_stream_url.as_str()])
            .args(FFMPEG_ARGS)
            .stderr(Stdio::null())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()?;

        let piped_stream_reader = PipedStreamReader::new(Box::new(child.stdout.unwrap()));

        Ok(Self { buffer: Cursor::new(vec![]), piped_stream_reader, position: 0, end: 0})
    }
}

impl Read for StreamMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if let Some(result) = futures::executor::block_on(self.piped_stream_reader.receiver.recv()) {
            if let Ok(data) = result {
                self.buffer.set_position(self.end);
                let bytes_written = self.buffer.write(&data.bytes[0..data.bytes_read])?;
                self.end += bytes_written as u64;
                self.buffer.set_position(self.position);
            }
        }
        let buffer_read_out = self.buffer.read(buf)?;
        self.position += buffer_read_out as u64;
        Ok(buffer_read_out)
    }
}

use tokio::sync::mpsc::{UnboundedReceiver, unbounded_channel};
use std::thread::spawn;
pub struct PipedStreamData {
    pub bytes: [u8; 4096],
    pub bytes_read: usize
}

pub struct PipedStreamReader {
    receiver: UnboundedReceiver<Result<PipedStreamData, std::io::Error>>
}

impl PipedStreamReader {
    pub fn new(mut stream: Box<dyn Read + Send>) -> Self {
        // TODO this probably shouldn't be unbounded
        let (tx, rx) = unbounded_channel();

        spawn(move || loop {
            let mut buffer = [0; 4096];
            let bytes_read = stream.read(&mut buffer);
            match bytes_read {
                Ok(bytes_read) => {
                    if bytes_read == 0 { break; }
                    let _ = tx.send(Ok(PipedStreamData { bytes: buffer, bytes_read }));
                },
                Err(err) => {
                    let _ = tx.send(Err(err));
                }
            }
        });

        Self { receiver: rx }
    }
}

impl Seek for StreamMediaSource {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.buffer.seek(pos)
    }
}

impl MediaSource for StreamMediaSource {
    fn byte_len(&self) -> Option<u64> {
        None
    }

    fn is_seekable(&self) -> bool {
        false
    }
}