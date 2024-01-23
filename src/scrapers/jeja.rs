use nom::{IResult, sequence::preceded, bytes::complete::take_until};
use thiserror::Error as ThisError;

pub async fn scrape_joke(client: reqwest::Client) -> Result<String, TTSError> {
    let mut text = String::new();

    let request = client.get("https://dowcipy.jeja.pl/losowe").build()?;
    let response = client.execute(request).await?;

    let html_text = response.text().await?;
    let html_fragment = parse_html(&html_text).map_err(|_| TTSError::Parse)?.1.to_string();

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

#[derive(ThisError, Debug)]
pub enum TTSError {
    #[error("")]
    Parse,
    #[error("")]
    Save { message: String },
    #[error("")]
    JokeEmpty,
    #[error("")]
    Scrape(#[from] reqwest::Error)
}

const TRIES: usize = 50;
pub async fn tts_download(filename: &str, client: reqwest::Client) -> Result<(), TTSError> {
    let mut text = String::new();

    for i in 0..TRIES {
        match scrape_joke(client.clone()).await {
            Ok(joke) => {
                if joke.len() <= 100 && joke.len() > 0 { text = joke; break;}
            },
            Err(err) => {
                if i + 1 == TRIES { return Err(err);}
            }
        }
    }
    if text.is_empty() { return Err(TTSError::JokeEmpty.into()) }
    let gtts_client = tts_rust::tts::GTTSClient::new(1.0, tts_rust::languages::Languages::Polish, "com");
    gtts_client.save_to_file(&text, filename).map_err(|message| TTSError::Save { message }.into())
}