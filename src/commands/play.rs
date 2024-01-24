use std::sync::Arc;

use crate::{data::Context, convert_query::{ConvertedQuery, MetaInput, PendingMetaInput}, metadata::{LazyMetadata, LazyMetadataEventHandler}, utils::format_duration};
use poise::CreateReply;
use serenity::{model::Color, builder::{CreateEmbed, CreateAllowedMentions}};
use songbird::{Call, tracks::TrackHandle};
use tokio::sync::Mutex;
use crate::commands::{
    utils::should_move_channels,
    error::{VoiceError, CommandError}
};

// plays audio from an url or a search query
#[poise::command(slash_command, prefix_command, guild_only, aliases("p"))]
pub async fn play(ctx: Context<'_>, query: Vec<String>) -> Result<(), CommandError> {
    if query.len() == 0 { return Err(CommandError::InvalidQuery) }
    let query = query.join(" "); // represent query as a string vector so spaces are allowed
    let guild = ctx.guild().unwrap().clone();
    let user_voice = guild.voice_states.get(&ctx.author().id).ok_or(VoiceError::NotConnected)?;
    
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;
    let handler = manager.get_or_insert(guild.id);

    if !should_move_channels(&ctx, &guild, user_voice).await { return Err(VoiceError::DifferentVoiceChannel.into()) }
   
    let connection = handler.lock().await.current_connection().and_then(|conn| conn.channel_id);

    manager.join(guild.id, user_voice.channel_id.unwrap()).await?;

    let was_empty = {
        let mut handler_guard = handler.lock().await;

        // add event handler upon joining a channel
        if connection.is_none() { 
            handler_guard.add_global_event(songbird::Event::Track(songbird::TrackEvent::Play), LazyMetadataEventHandler { handler: handler.clone(), channel_id: ctx.channel_id(), http: ctx.serenity_context().http.clone() });
        }

        let _ = handler_guard.deafen(true).await; 

        handler_guard.queue().is_empty()
    };
    
    let converted_query = ctx.data().convert_query(&query, ctx.author().into()).await?;

    match converted_query {
        ConvertedQuery::LiveVideo(metainput) => {
            let track_metadata = metainput.track_metadata.clone();

            add_live_video(handler.clone(), metainput).await;

            let video_metadata = &track_metadata.video_metadata;
            let description = match &video_metadata.audio_source {
                crate::metadata::AudioSource::YouTube { video_id } => format!("[{}](https://youtu.be/{}) | {}", video_metadata.title, video_id, format_duration(video_metadata.duration, None)),
                crate::metadata::AudioSource::File { .. } => format!("{} | {}", video_metadata.title, format_duration(video_metadata.duration, None)),
                crate::metadata::AudioSource::Jeja { .. } => video_metadata.title.clone()
            };
            if !was_empty {
                let reply_handle = ctx.send(
                    CreateReply::default()
                    .ephemeral(true)
                    .reply(true)
                    .allowed_mentions(CreateAllowedMentions::new()
                        .replied_user(true))
                    .embed(CreateEmbed::new()
                        .title("Added Track:")
                        .description(description)
                        .color(Color::PURPLE))
                ).await?;
                ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
            }
        },
        ConvertedQuery::LivePlaylist(metainputs) => {
            let metainputs_len = metainputs.len();

            match was_empty {
                true => { // if the queue was empty we immediately enqueue the first track and push the rest to a buffer
                    let mut metainputs_iter = metainputs.into_iter();
                    let metainput = metainputs_iter.next().ok_or(CommandError::EmptyPlaylist)?;

                    add_live_video(handler.clone(), metainput).await;

                    add_live_videos(handler, metainputs_iter).await;
                },
                false => { // else we push everything to a buffer
                    add_live_videos(handler, metainputs.into_iter()).await;
                }
            };

            let reply_handle = ctx.send(
                CreateReply::default()
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(CreateAllowedMentions::new()
                    .replied_user(true))
                .embed(CreateEmbed::new()
                    .title(format!("Added {} Tracks:", metainputs_len))
                    .color(Color::PURPLE))
            ).await?;

            ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
        },
        ConvertedQuery::PendingPlaylist(pending_metainputs) => {
            let metainputs_len = pending_metainputs.len();

            match was_empty {
                true => { // if the queue was empty we immediately generate metadata, enqueue the first track and push the rest to a buffer
                    let mut pending_metainputs_iter = pending_metainputs.into_iter();
                    
                    let pending_metainput = pending_metainputs_iter.next().ok_or(CommandError::EmptyPlaylist)?;

                    let mut first_track_handle = add_pending_video(handler.clone(), pending_metainput).await;
                    first_track_handle.awake_lazy_metadata().await?;
                    
                    add_pending_videos(handler, pending_metainputs_iter).await;
                },
                false => { // else we push everything to a buffer
                    add_pending_videos(handler, pending_metainputs.into_iter()).await;
                }
            };
            
            let reply_handle = ctx.send(
                CreateReply::default()
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(CreateAllowedMentions::new()
                    .replied_user(true))
                .embed(CreateEmbed::new()
                    .title(format!("Added {} Tracks:", metainputs_len))
                    .color(Color::PURPLE))
            ).await?;
            ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
        }
    }
    
    Ok(())
}

async fn add_live_video(handler: Arc<Mutex<Call>>, metainput: MetaInput) -> TrackHandle {
    let MetaInput { input, track_metadata } = metainput;
    let mut track_handle = handler.lock().await.enqueue_input(input).await;
    track_handle.write_lazy_metadata(track_metadata).await;
    track_handle
}

async fn add_pending_video(handler: Arc<Mutex<Call>>, pending_metainput: PendingMetaInput) -> TrackHandle {
    let PendingMetaInput { input, query, added_by } = pending_metainput;
    let mut track_handle = handler.lock().await.enqueue_input(input).await;
    track_handle.write_added_by(added_by).await;
    track_handle.write_query(query).await;
    track_handle
}

async fn add_live_videos(handler: Arc<Mutex<Call>>, metainputs: std::vec::IntoIter<MetaInput>) {
    let mut handler_guard = handler.lock().await;
    for metainput in metainputs {
        let mut track_handle = handler_guard.enqueue_input(metainput.input).await;
        track_handle.write_lazy_metadata(metainput.track_metadata).await;
    }
}

async fn add_pending_videos(handler: Arc<Mutex<Call>>, metainputs: std::vec::IntoIter<PendingMetaInput>) {
    let mut handler_guard = handler.lock().await;
    for metainput in metainputs {
        let mut track_handle = handler_guard.enqueue_input(metainput.input).await;
        track_handle.write_added_by(metainput.added_by).await;
        track_handle.write_query(metainput.query).await;
    }
}