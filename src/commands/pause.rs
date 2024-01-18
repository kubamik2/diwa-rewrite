use crate::data::Context;

use crate::commands::{ error::{VoiceError, CommandError}, utils::same_voice_channel };

// pauses playback
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn pause(ctx: Context<'_>) -> Result<(), CommandError> {
    let guild = ctx.guild().unwrap();
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;
    if let Some(handler) = manager.get(guild.id) {
        if !same_voice_channel(&guild, &ctx.author().id, handler.clone()).await { return Ok(()); }
        
        let Some(current_track_handle) = handler.lock().await.queue().current() else { return Ok(()); };
        current_track_handle.pause()?;
    }
    Ok(())
}