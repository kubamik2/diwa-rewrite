use crate::data::Context;
use crate::commands::{error::{VoiceError, CommandError}, utils::same_voice_channel};

// leaves the voice channel
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn leave(ctx: Context<'_>) -> Result<(), CommandError> {
    let guild = ctx.guild().unwrap().clone();
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;
    
    if let Some(handler) = manager.get(guild.id) {
        if !same_voice_channel(&guild, &ctx.author().id, handler.clone()).await { return Ok(()); }
        manager.remove(guild.id).await?;
    }
    Ok(())
}