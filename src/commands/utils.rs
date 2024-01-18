#![allow(dead_code)]

use std::sync::Arc;
use crate::{data::Context, error::DynError};
use poise::serenity_prelude::{Guild, UserId};
use serenity::utils::Color;
use songbird::Call;
use tokio::sync::Mutex;

pub async fn send_timed_reply<S: ToString>(ctx: &Context<'_>, description: S, delay: Option<std::time::Duration>) -> Result<(), DynError> {
    let reply_handle = ctx.send(|msg| msg
        .ephemeral(true)
        .reply(true)
        .allowed_mentions(|mentions| mentions.replied_user(true))
        .embed(|embed| embed
            .description(description)
            .color(Color::PURPLE))
    ).await?;
    ctx.data().add_to_cleanup(reply_handle, delay.unwrap_or(std::time::Duration::from_secs(5))).await;
    Ok(())
}

pub async fn send_timed_error<S: ToString>(ctx: &Context<'_>, description: S, delay: Option<std::time::Duration>) -> Result<(), DynError> {
    let reply_handle = ctx.send(|msg| msg
        .ephemeral(true)
        .reply(true)
        .allowed_mentions(|mentions| mentions.replied_user(true))
        .embed(|embed| embed
            .title("Error")
            .description(description)
            .color(Color::RED))
    ).await?;
    ctx.data().add_to_cleanup(reply_handle, delay.unwrap_or(std::time::Duration::from_secs(5))).await;
    Ok(())
}

pub async fn same_voice_channel(guild: &Guild, user_id: &UserId, handler: Arc<Mutex<Call>>) -> bool {
    if let Some(user_voice) = guild.voice_states.get(user_id) {
        if let Some(user_voice_channel_id) = user_voice.channel_id {
            if let Some(bot_voice) = handler.lock().await.current_connection() {
                if let Some(bot_voice_channel_id) = bot_voice.channel_id {
                    return user_voice_channel_id.0 == bot_voice_channel_id.0;
                }
            } else {
                return true;
            }
        }
    }
    false
}