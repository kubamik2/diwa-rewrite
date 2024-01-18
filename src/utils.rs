use serenity::builder::CreateEmbed;

use crate::{metadata::TrackMetadata, metadata::AudioSource};

pub fn format_duration(duration: std::time::Duration, length: Option<usize>) -> String {
    let s = duration.as_secs() % 60;
    let m = duration.as_secs() / 60 % 60;
    let h = duration.as_secs() / 3600 % 24;
    let d = duration.as_secs() / 86400;
    let mut formatted_duration = format!("{:0>2}:{:0>2}:{:0>2}:{:0>2}", d, h, m, s);
    if let Some(length) = length {
        formatted_duration = formatted_duration.split_at(formatted_duration.len() - length as usize).1.to_owned();
    } else {
        while let Some(stripped_formatted_duration) = formatted_duration.strip_prefix("00:") {
            formatted_duration = stripped_formatted_duration.to_owned();
            if formatted_duration.len() == 5 {
                if formatted_duration.chars().nth(0) == Some('0') {
                    formatted_duration.remove(0);
                }
                break;
            }
        }
    }
    formatted_duration
}

pub fn create_now_playing_embed(track_metadata: TrackMetadata) -> CreateEmbed {
    let added_by = track_metadata.added_by;
    let video_metadata = track_metadata.video_metadata;
    let formatted_duration = format_duration(video_metadata.duration, None);
    let mut embed = CreateEmbed::default();
    embed.title("Now Playing:");

    match video_metadata.audio_source {
        AudioSource::YouTube { video_id } => {
            embed.description(format!("[{}](https://youtu.be/{}) | {}", video_metadata.title, video_id, formatted_duration));
        },
        AudioSource::File { .. } => {
            embed.description(format!("{} | {}", video_metadata.title, formatted_duration));
        },
        AudioSource::Jeja { .. } => {
            embed.description(video_metadata.title.clone());
        },
    }

    embed.author(|author| {author
        .url(format!("https://discordapp.com/users/{}", added_by.id))
        .name(added_by.name);
        if let Some(avatar_url) = added_by.avatar_url {
            author.icon_url(avatar_url);
        }
        author
    });
    embed
}