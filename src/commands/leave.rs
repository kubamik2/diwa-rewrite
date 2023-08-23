use diwa::{ Context, error::Error };

use crate::commands::{ error::VoiceError, utils::same_voice_channel };

#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn leave(ctx: Context<'_>) -> Result<(), Error> {
    let guild = ctx.guild().unwrap();
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::ManagerNone)?;
    if let Some(handler) = manager.get(guild.id) {
        if !same_voice_channel(&guild, &ctx.author().id, handler.clone()).await { return Ok(()); }
        manager.remove(guild.id).await?;
    }
    Ok(())
}