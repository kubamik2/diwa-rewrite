use crate::{data::Context, commands::error::CommandError};

use crate::commands::{ error::VoiceError, utils::same_voice_channel };

// skips to the next track
#[poise::command(slash_command, prefix_command, guild_only, aliases("next"))]
pub async fn skip(ctx: Context<'_>) -> Result<(), CommandError> {
    let guild = ctx.guild().unwrap().clone();
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;
    
    if let Some(handler) = manager.get(guild.id) {
        if !same_voice_channel(&guild, &ctx.author().id, handler.clone()).await { return Ok(()); }
        
        handler.lock().await.queue().skip()?;
    }
    Ok(())
}