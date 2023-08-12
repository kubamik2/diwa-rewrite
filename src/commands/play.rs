use diwa::{ Context, error::{Error, VoiceError}, ConvertedQuery, LazyMetadata };
use serenity::utils::Color;
use songbird::create_player;



#[poise::command(slash_command, prefix_command)]
pub async fn play(ctx: Context<'_>, query: String) -> Result<(), Error> {
    let guild = ctx.guild().unwrap();
    let user_voice = guild.voice_states.get(&ctx.author().id).ok_or(VoiceError::UserNotInVoice)?;

    let manager = songbird::get(&ctx.serenity_context()).await.unwrap(); // TODO custom error?
    let handler = manager.get_or_insert(guild.id);
    let mut handler_guard = handler.lock().await;

    let bot_current_channel_id = handler_guard.current_connection()
        .and_then(|conn| conn.channel_id
            .map(|channel_id| channel_id.0));
    let user_current_channel_id = user_voice.channel_id.map(|channel_id| channel_id.0);
        
    if bot_current_channel_id.is_some() {
        if bot_current_channel_id != user_current_channel_id {
            // TODO diff channel err
        }
    }

    handler_guard.join(user_voice.channel_id.unwrap()).await?; // TODO custom error?

    let converted_query = ctx.data().convert_query(&query, Some(ctx.author().id)).await?;
    let mut was_empty = handler_guard.queue().is_empty();

    match converted_query{
        ConvertedQuery::LiveVideo(metainput) => {
            let input = metainput.input;
            let metadata = metainput.metadata;
            let (track, mut track_handle) = create_player(input);
            track_handle.write_lazy_metadata(metadata.clone()).await;
            handler_guard.enqueue(track);

            let description = match metadata.audio_source {
                diwa::AudioSource::YouTube { video_id } => format!("[{}](https://youtu.be/{}) | {}", metadata.title, video_id, format_duration(metadata.duration, None)),
                diwa::AudioSource::File { path: _ } => format!("{} | {}", metadata.title, format_duration(metadata.duration, None))
            };
            ctx.send(|msg| msg
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(|mentions| mentions.replied_user(true))
                .embed(|embed| embed
                    .title("Added track:")
                    .description(description)
                    .color(Color::PURPLE))
            ).await?;
        },
        ConvertedQuery::LivePlaylist(metainputs) => {
            let metainputs_len = metainputs.len();
            for metainput in metainputs {
                let input = metainput.input;
                let metadata = metainput.metadata;
                let (track, mut track_handle) = create_player(input);
                track_handle.write_lazy_metadata(metadata).await;
                handler_guard.enqueue(track);
            }
            ctx.send(|msg| msg
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(|mentions| mentions.replied_user(true))
                .embed(|embed| embed
                    .title(format!("Added {} tracks:", metainputs_len))
                    .color(Color::PURPLE))
            ).await?;
        },
        ConvertedQuery::PendingPlaylist(pending_metainputs) => {
            let metainputs_len = pending_metainputs.len();
            for pending_metainput in pending_metainputs.into_iter() {
                let input = pending_metainput.input;
                let user_id = pending_metainput.user_id;
                if was_empty {
                    let (track, mut track_handle) = create_player(input);
                    let mut metadata = track_handle.generate_lazy_metadata().await?;
                    metadata.added_by = user_id;
                    track_handle.write_lazy_metadata(metadata).await;
                    handler_guard.enqueue(track);
                    was_empty = false;
                    continue;
                }
                let (track, _) = create_player(input);
                handler_guard.enqueue(track);
            }
            ctx.send(|msg| msg
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(|mentions| mentions.replied_user(true))
                .embed(|embed| embed
                    .title(format!("Added {} tracks:", metainputs_len))
                    .color(Color::PURPLE))
            ).await?;
        }
    }

    Ok(())
}

pub fn format_duration(duration: std::time::Duration, length: Option<u32>) -> String {
    let s = duration.as_secs() % 60;
    let m = duration.as_secs() / 60 % 60;
    let h = duration.as_secs() / 3600 % 24;
    let d = duration.as_secs() / 86400;
    let mut formatted_duration = format!("{:0>2}:{:0>2}:{:0>2}:{:0>2}", d, h, m, s);
    if let Some(length) = length {
        formatted_duration = formatted_duration.split_at(formatted_duration.len() - length as usize).1.to_owned();
    } else {
        while formatted_duration.len() > 5 {
            if let Some(stripped_formatted_duration) = formatted_duration.strip_prefix("00:") {
                formatted_duration = stripped_formatted_duration.to_owned();
            }
        }
    }
    formatted_duration
}