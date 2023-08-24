use std::io::{ Write, Read, Seek, Cursor };
use std::process::{ Stdio, Command, ChildStdout };
use symphonia::core::io::MediaSource;

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
    stdout: ChildStdout,
    position: u64,
    end: u64
}

impl StreamMediaSource {
    pub async fn new(video_id: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let id = rustube::Id::from_string(video_id.to_owned())?;
        let video = rustube::Video::from_id(id).await?;
        let best_audio = video.best_audio().unwrap();
        let audio_stream_url = best_audio.signature_cipher.url.clone();

        let child = Command::new("ffmpeg")
            .args(["-i", audio_stream_url.as_str()])
            .args(FFMPEG_ARGS).stderr(Stdio::null())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .spawn()?;
        Ok(Self { buffer: Cursor::new(vec![]), stdout: child.stdout.unwrap(), position: 0, end: 0 })
    }
}

impl Read for StreamMediaSource {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        // grab at most 2048 bytes from ffmpeg's stdout
        let mut transcode_buf = [0; 2048];
        let bytes_read = self.stdout.read(&mut transcode_buf)?;

        // append retrieved stdout snippet to buffer
        self.buffer.set_position(self.end);
        let bytes_written = self.buffer.write(&transcode_buf[0..bytes_read])?;
        self.end += bytes_written as u64;
        self.buffer.set_position(self.position);

        let buffer_read_out = self.buffer.read(buf)?;
        self.position += buffer_read_out as u64;
        
        Ok(buffer_read_out)
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