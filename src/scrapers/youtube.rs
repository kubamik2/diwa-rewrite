use reqwest::{Client, Url, Method};
use nom::{ IResult, bytes::complete::{tag, take_until}, combinator::into, sequence::preceded};
use crate::metadata::AudioSource;
use thiserror::Error as ThisError;

#[derive(Debug, ThisError)]
pub enum YoutubeScrapeError {
    #[error("")]
    DurationString { duration_string: String },
    #[error("")]
    DurationPage,
    #[error("")]
    VideoId,
    #[error("")]
    Title, 
    #[error("")]
    Request(#[from] reqwest::Error),
    #[error("")]
    Url(#[from] url::ParseError),
}

fn parse_title(input: &str) -> IResult<&str, String> {
    preceded(
        take_until("\"title\":"), preceded(
            take_until("\"text\":\""), preceded(tag("\"text\":\""), into(take_until::<&str, &str, nom::error::Error<&str>>("\"}")))
        )
    )(input)
}

fn parse_video_id(input: &str) -> IResult<&str, String> {
    preceded(preceded(take_until("\"videoId\":\""), tag("\"videoId\":\"")), into(take_until::<&str, &str, nom::error::Error<&str>>("\"")))(input)
}

fn parse_duration_string(input: &str) -> IResult<&str, &str> {
    preceded(take_until("\"lengthText\":"), preceded(take_until("\"simpleText\":\""), preceded(tag("\"simpleText\":\""), take_until("\""))))(input)
}

fn string_to_duration(input: &str) -> Result<std::time::Duration, YoutubeScrapeError> {
    let time_sections = input.split(":").collect::<Vec<&str>>();
    let mut seconds: u64 = 0;
    let mut multiplier = 1;
    for time_section in time_sections.iter().rev() {
        seconds += time_section.parse::<u64>().map_err(|_| YoutubeScrapeError::DurationString { duration_string: input.to_owned() })? * multiplier;
        multiplier *= 60;
    }
    
    Ok(std::time::Duration::from_secs(seconds))
}

pub async fn search(query: &str) -> Result<crate::metadata::VideoMetadata, YoutubeScrapeError> {
    let client = Client::new();
    let url = Url::parse("https://www.youtube.com/results")?;
    let request = client.request(Method::GET, url).query(&[("search_query", query)]).build()?;

    let mut doc = client.execute(request).await?.text().await?;
    let (rest, video_id) = parse_video_id(&doc).map_err(|_| YoutubeScrapeError::VideoId)?;
    doc = rest.to_owned();
    let (rest, title) = parse_title(&doc).map_err(|_| YoutubeScrapeError::Title)?;
    let title = serde_json::from_str(&format!("\"{title}\"")).unwrap_or("ERROR".to_string());
    doc = rest.to_owned();
    let (_, duration_string) = parse_duration_string(&doc).map_err(|_| YoutubeScrapeError::DurationPage)?;
    let duration = string_to_duration(duration_string)?;
    let audio_source = AudioSource::YouTube { video_id };

    Ok(crate::metadata::VideoMetadata { title, duration, audio_source })
}