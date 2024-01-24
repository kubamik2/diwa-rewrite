use crate::{data::Context, metadata::LazyMetadataEventHandler, commands::error::CommandError};

use crate::commands::{error::VoiceError, utils::should_move_channels};

// joins the voice channel
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn join(ctx: Context<'_>) -> Result<(), CommandError> {
    let guild = ctx.guild().unwrap().clone();
    let user_voice = guild.voice_states.get(&ctx.author().id).ok_or(VoiceError::NotConnected)?;

    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;
    let handler = manager.get_or_insert(guild.id);

    if !should_move_channels(&ctx, &guild, user_voice).await { return Err(VoiceError::DifferentVoiceChannel.into()) }

    let connection = handler.lock().await.current_connection().and_then(|conn| conn.channel_id);

    let handler = manager.join(guild.id, user_voice.channel_id.unwrap()).await?;

    let mut handler_guard = handler.lock().await;

    // add event handler upon joining a channel
    if connection.is_none() { 
        handler_guard.add_global_event(songbird::Event::Track(songbird::TrackEvent::Play), LazyMetadataEventHandler { handler: handler.clone(), channel_id: ctx.channel_id(), http: ctx.serenity_context().http.clone() });
    }

    let _ = handler_guard.deafen(true).await; 

    Ok(())
}