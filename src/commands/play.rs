use std::time::Duration;

use diwa::{ Context, error::Error, ConvertedQuery, metadata::LazyMetadata, utils::format_duration };
use serenity::utils::Color;
use songbird::create_player;
use crate::commands::{
    utils::send_timed_error,
    error::VoiceError
};

#[poise::command(slash_command, prefix_command)]
pub async fn play(ctx: Context<'_>, query: String) -> Result<(), Error> {
    let guild = ctx.guild().unwrap();
    let user_voice = guild.voice_states.get(&ctx.author().id).ok_or(VoiceError::UserNotInVoice)?;

    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::ManagerNone)?;
    let handler = manager.get_or_insert(guild.id);
    let mut handler_guard = handler.lock().await;

    let bot_current_channel_id = handler_guard.current_connection()
        .and_then(|conn| conn.channel_id
            .map(|channel_id| channel_id.0));
    let user_current_channel_id = user_voice.channel_id.map(|channel_id| channel_id.0);
        
    if bot_current_channel_id.is_some() {
        if bot_current_channel_id != user_current_channel_id {
            send_timed_error(&ctx, "You're in a different voice channel", Some(Duration::from_secs(10))).await;
        }
    }

    handler_guard.join(user_voice.channel_id.unwrap()).await?;

    let converted_query = ctx.data().convert_query(&query, ctx.author().into()).await?;
    let mut was_empty = handler_guard.queue().is_empty();

    match converted_query{
        ConvertedQuery::LiveVideo(metainput) => {
            let input = metainput.input;
            let track_metadata = metainput.track_metadata;
            
            let (track, mut track_handle) = create_player(input);
            track_handle.write_lazy_metadata(track_metadata.clone()).await;
            handler_guard.enqueue(track);

            let video_metadata = track_metadata.video_metadata;
            let description = match video_metadata.audio_source {
                diwa::AudioSource::YouTube { video_id } => format!("[{}](https://youtu.be/{}) | {}", video_metadata.title, video_id, format_duration(video_metadata.duration, None)),
                diwa::AudioSource::File { path: _ } => format!("{} | {}", video_metadata.title, format_duration(video_metadata.duration, None))
            };
            let reply_handle = ctx.send(|msg| msg
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(|mentions| mentions.replied_user(true))
                .embed(|embed| embed
                    .title("Added track:")
                    .description(description)
                    .color(Color::PURPLE))
            ).await?;
            ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
        },
        ConvertedQuery::LivePlaylist(metainputs) => {
            let metainputs_len = metainputs.len();
            for metainput in metainputs {
                let input = metainput.input;
                let metadata = metainput.track_metadata;
                let (track, mut track_handle) = create_player(input);
                track_handle.write_lazy_metadata(metadata).await;
                handler_guard.enqueue(track);
            }
            let reply_handle = ctx.send(|msg| msg
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(|mentions| mentions.replied_user(true))
                .embed(|embed| embed
                    .title(format!("Added {} tracks:", metainputs_len))
                    .color(Color::PURPLE))
            ).await?;
            ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
        },
        ConvertedQuery::PendingPlaylist(pending_metainputs) => {
            let metainputs_len = pending_metainputs.len();
            for pending_metainput in pending_metainputs.into_iter() {
                let input = pending_metainput.input;
                let added_by = pending_metainput.added_by;
                let (track, mut track_handle) = create_player(input);
                track_handle.write_added_by(added_by).await;
                if was_empty {
                    let metadata = track_handle.generate_lazy_metadata().await?;
                    track_handle.write_lazy_metadata(metadata).await;
                    handler_guard.enqueue(track);
                    was_empty = false;
                    continue;
                }
                handler_guard.enqueue(track);
            }
            let reply_handle = ctx.send(|msg| msg
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(|mentions| mentions.replied_user(true))
                .embed(|embed| embed
                    .title(format!("Added {} tracks:", metainputs_len))
                    .color(Color::PURPLE))
            ).await?;
            ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
        }
    }

    Ok(())
}