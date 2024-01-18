use crate::{data::Context, convert_query::ConvertedQuery, metadata::{LazyMetadata, LazyMetadataEventHandler}, utils::format_duration};
use serenity::utils::Color;
use songbird::create_player;
use crate::commands::{
    utils::same_voice_channel,
    error::{VoiceError, CommandError}
};

// plays audio from an url or a search query
#[poise::command(slash_command, prefix_command, guild_only, aliases("p"))]
pub async fn play(ctx: Context<'_>, query: Vec<String>) -> Result<(), CommandError> {
    if query.len() == 0 { return Err(CommandError::InvalidQuery) }
    let query = query.join(" "); // represent query as a string vector so spaces are allowed
    let guild = ctx.guild().unwrap();
    let user_voice = guild.voice_states.get(&ctx.author().id).ok_or(VoiceError::NotConnected)?;
    
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;
    let handler = manager.get_or_insert(guild.id);

    if !same_voice_channel(&guild, &ctx.author().id, handler.clone()).await { return Err(VoiceError::DifferentVoiceChannel.into()) }
    
    let mut handler_guard = handler.lock().await;
    let bot_current_channel_id = handler_guard.current_connection().and_then(|conn| conn.channel_id);

    // add event handler upon joining a channel
    if bot_current_channel_id.is_none() { 
        handler_guard.add_global_event(songbird::Event::Track(songbird::TrackEvent::Play), LazyMetadataEventHandler { handler: handler.clone(), channel_id: ctx.channel_id(), http: ctx.serenity_context().http.clone() });
    }

    handler_guard.join(user_voice.channel_id.unwrap()).await?;
    let _ = handler_guard.deafen(true).await; 
    let was_empty = handler_guard.queue().is_empty();
    drop(handler_guard);

    let converted_query = ctx.data().convert_query(&query, ctx.author().into()).await?;

    match converted_query{
        ConvertedQuery::LiveVideo(metainput) => {
            let input = metainput.input;
            let track_metadata = metainput.track_metadata;
            
            let (track, mut track_handle) = create_player(input);
            track_handle.write_lazy_metadata(track_metadata.clone()).await;
            handler.lock().await.enqueue(track);

            let video_metadata = &track_metadata.video_metadata;
            let description = match &video_metadata.audio_source {
                crate::metadata::AudioSource::YouTube { video_id } => format!("[{}](https://youtu.be/{}) | {}", video_metadata.title, video_id, format_duration(video_metadata.duration, None)),
                crate::metadata::AudioSource::File { .. } => format!("{} | {}", video_metadata.title, format_duration(video_metadata.duration, None)),
                crate::metadata::AudioSource::Jeja { .. } => video_metadata.title.clone()
            };
            match was_empty {
                true => {
                    let now_playing_embed = crate::utils::create_now_playing_embed(track_metadata);
                    let reply_handle = ctx.send(|msg| msg
                        .embed(|embed| {embed.clone_from(&now_playing_embed); embed})).await?;
                    ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
                },
                false => {
                    let reply_handle = ctx.send(|msg| msg
                        .ephemeral(true)
                        .reply(true)
                        .allowed_mentions(|mentions| mentions.replied_user(true))
                        .embed(|embed| embed
                            .title("Added Track:")
                            .description(description)
                            .color(Color::PURPLE))
                    ).await?;
                    ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
                }
            }
        },
        ConvertedQuery::LivePlaylist(metainputs) => {
            let mut converted_tracks = vec![]; // we use a buffer to minimize handler lock time
            let metainputs_len = metainputs.len();

            let first_track_handle = match was_empty {
                true => { // if the queue was empty we immediately enqueue the first track and push the rest to a buffer
                    let mut metainputs_iter = metainputs.into_iter();
                    let metainput = metainputs_iter.next().ok_or(CommandError::EmptyPlaylist)?;
                    let input = metainput.input;
                    let metadata = metainput.track_metadata;
                    let (track, mut first_track_handle) = create_player(input);
                    handler.lock().await.enqueue(track);
                    first_track_handle.write_lazy_metadata(metadata).await;

                    for metainput in metainputs_iter {
                        let input = metainput.input;
                        let metadata = metainput.track_metadata;
                        let (track, mut track_handle) = create_player(input);
                        track_handle.write_lazy_metadata(metadata).await;
                        converted_tracks.push(track);
                    }
                    Some(first_track_handle)
                },
                false => { // else we push everything to a buffer
                    for metainput in metainputs {
                        let input = metainput.input;
                        let metadata = metainput.track_metadata;
                        let (track, mut track_handle) = create_player(input);
                        track_handle.write_lazy_metadata(metadata).await;
                        converted_tracks.push(track);
                    }
                    None
                }
            };

            // enqueue the buffer
            let mut handler_guard = handler.lock().await;
            for track in converted_tracks {
                handler_guard.enqueue(track);
            }
            drop(handler_guard);

            let reply_handle = ctx.send(|msg| msg
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(|mentions| mentions.replied_user(true))
                .embed(|embed| embed
                    .title(format!("Added {} Tracks:", metainputs_len))
                    .color(Color::PURPLE))
            ).await?;
            ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
            if let Some(first_track_handle) = first_track_handle {
                let track_metadata = first_track_handle.read_lazy_metadata().await.unwrap(); // always holds
                let now_playing_embed = crate::utils::create_now_playing_embed(track_metadata);
                let reply_handle = ctx.send(|msg| msg
                    .embed(|embed| {embed.clone_from(&now_playing_embed); embed})
                ).await?;
                ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
            }
        },
        ConvertedQuery::PendingPlaylist(pending_metainputs) => {
            let mut converted_tracks = vec![]; // we use a buffer to minimize handler lock time
            let metainputs_len = pending_metainputs.len();

            let first_track_handle = match was_empty {
                true => { // if the queue was empty we immediately generate metadata, enqueue the first track and push the rest to a buffer
                    let mut pending_metainputs_iter = pending_metainputs.into_iter();
                    
                    let pending_metainput = pending_metainputs_iter.next().ok_or(CommandError::EmptyPlaylist)?;

                    let input = pending_metainput.input;
                    let added_by = pending_metainput.added_by;
                    let (track, mut first_track_handle) = create_player(input);
                    first_track_handle.write_added_by(added_by).await;
                    handler.lock().await.enqueue(track);
                    let metadata = first_track_handle.generate_lazy_metadata().await?;
                    first_track_handle.write_lazy_metadata(metadata).await;
                    
                    
                    for pending_metainput in pending_metainputs_iter {
                        let input = pending_metainput.input;
                        let added_by = pending_metainput.added_by;
                        let (track, mut track_handle) = create_player(input);
                        track_handle.write_added_by(added_by).await;
                        converted_tracks.push(track);
                    }
                    Some(first_track_handle)
                },
                false => { // else we push everything to a buffer
                    for pending_metainput in pending_metainputs {
                        let input = pending_metainput.input;
                        let added_by = pending_metainput.added_by;
                        let (track, mut track_handle) = create_player(input);
                        track_handle.write_added_by(added_by).await;
                        converted_tracks.push(track);
                    }
                    None
                }
            };
            
            // enqueue the buffer
            let mut handler_guard = handler.lock().await;
            for track in converted_tracks {
                handler_guard.enqueue(track);
            }
            drop(handler_guard);
            
            let reply_handle = ctx.send(|msg| msg
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(|mentions| mentions.replied_user(true))
                .embed(|embed| embed
                    .title(format!("Added {} Tracks:", metainputs_len))
                    .color(Color::PURPLE))
            ).await?;
            ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;

            if let Some(first_track_handle) = first_track_handle {
                let track_metadata = first_track_handle.read_lazy_metadata().await.unwrap(); // always holds
                let now_playing_embed = crate::utils::create_now_playing_embed(track_metadata);
                let reply_handle = ctx.send(|msg| msg
                    .embed(|embed| {embed.clone_from(&now_playing_embed); embed})
                ).await?;
                ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
            }
        }
    }
    
    Ok(())
}