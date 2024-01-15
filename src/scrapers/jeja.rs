use nom::{IResult, sequence::preceded, bytes::complete::take_until};
use thiserror::Error as ThisError;

use crate::error::Error;

pub async fn scrape_joke() -> Result<String, Error> {
    let client = reqwest::Client::new();
    let mut text = String::new();

    let request = client.get("https://dowcipy.jeja.pl/losowe").build()?;
    let response = client.execute(request).await?;

    let html_text = response.text().await?;
    let html_fragment = parse_html(&html_text).map_err(|_| TTSError::ParseError)?.1.to_string();

    let doc = scraper::Html::parse_fragment(&html_fragment);

    for node in doc.tree {
        match node {
            scraper::node::Node::Text(text_node) => {
                text.push_str(&text_node);
            },
            _ => ()
        }
    }
    text = text.replace('\n', "");
    text = text.replace('\t', "");

    Ok(text)
}

fn parse_html(input: &str) -> IResult<&str, &str> {
    preceded(
        take_until("<h1>Losowe dowcipy</h1>"), 
            preceded(
                take_until("<div class=\"dow-left-text\">"),
                take_until("<div class=\"ob-left-down-autor\">")
            )
    )(input)
}

#[derive(ThisError, Debug, Clone, PartialEq, Eq)]
pub enum TTSError {
    #[error("Error while parsing page")]
    ParseError,
    #[error("Error while saving tts file: {message}")]
    SaveError { message: String }
}

pub async fn tts_download(guild_id: u64) -> Result<(), Error> {
    let mut text = String::new();
    let mut tries = 5;
    loop {
        match scrape_joke().await {
            Ok(joke) => {
                if joke.len() <= 100 && joke.len() > 0 { text = joke; break;}
            },
            Err(err) => {
                tries -= 1;
                if tries == 0 { return Err(err);}
            }
        }
    }
    let client = tts_rust::tts::GTTSClient::new(1.0, tts_rust::languages::Languages::Polish, "com");
    client.save_to_file(&text, &format!("{}.mp4", guild_id)).map_err(|message| TTSError::SaveError { message }.into())
}