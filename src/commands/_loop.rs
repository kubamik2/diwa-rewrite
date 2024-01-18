use crate::data::Context;
use songbird::tracks::LoopState;

use crate::commands::{error::VoiceError, utils::same_voice_channel, error::CommandError};

// loops the first track
#[poise::command(slash_command, prefix_command, guild_only, rename = "loop")]
pub async fn _loop(ctx: Context<'_>) -> Result<(), CommandError> {
    let guild = ctx.guild().unwrap();
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(CommandError::Voice(VoiceError::NoManager))?;
    
    if let Some(handler) = manager.get(guild.id) {
        if !same_voice_channel(&guild, &ctx.author().id, handler.clone()).await { return Ok(()); }
        
        let Some(current_track) = handler.lock().await.queue().current() else { return Ok(()); };
        let info = current_track.get_info().await?;
        if info.loops == LoopState::Infinite {
            let _ = current_track.disable_loop();
        } else {
           let _ =  current_track.enable_loop();
        }
    }
    Ok(())
}