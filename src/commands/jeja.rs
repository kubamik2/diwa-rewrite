use std::time::Duration;

use crate::{data::Context, metadata::{LazyMetadataEventHandler, LazyMetadata, TrackMetadata, VideoMetadata, UserMetadata}, utils::format_duration, lazy_queued::LazyQueued, commands::error::CommandError};
use serenity::utils::Color;
use songbird::{create_player, input::{Restartable, Input}};

use crate::commands::{ error::VoiceError, utils::{same_voice_channel, send_timed_error} };

// tells a joke from jeja.pl
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn jeja(ctx: Context<'_>) -> Result<(), CommandError> {
    let guild = ctx.guild().unwrap();
    let user_voice = guild.voice_states.get(&ctx.author().id).ok_or(VoiceError::NotConnected)?;

    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;
    let handler = manager.get_or_insert(guild.id);

    if !same_voice_channel(&guild, &ctx.author().id, handler.clone()).await {
        let _ = send_timed_error(&ctx, "You're in a different voice channel", Some(Duration::from_secs(10))).await;
        return Ok(());
    }

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

    let track_metadata = TrackMetadata {
        video_metadata: VideoMetadata {
            title: "Dowcip".to_string(),
            duration: Duration::from_secs(20),
            audio_source: crate::metadata::AudioSource::Jeja { guild_id: guild.id.0 }
        },
        added_by: UserMetadata {
            name: ctx.author().name.clone(),
            avatar_url: ctx.author().avatar_url(),
            id: ctx.author().id.0
        }
    };
    
    let input: Input = Restartable::new(LazyQueued::Metadata { metadata: track_metadata.video_metadata.clone() }, true).await?.into();
    let (track, mut track_handle) = create_player(input);
    
    track_handle.write_lazy_metadata(track_metadata.clone()).await;
    handler.lock().await.enqueue(track);

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
                    .description(format!("{} | {}", track_metadata.video_metadata.title, format_duration(track_metadata.video_metadata.duration, None)))
                    .color(Color::PURPLE))
            ).await?;
            ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
        }
    }
    Ok(())
}