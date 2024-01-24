#![allow(dead_code)]

use std::sync::Arc;
use crate::{data::Context, error::DynError};
use poise::{serenity_prelude::{Guild, UserId}, CreateReply};
use serenity::{model::Color, builder::{CreateAllowedMentions, CreateEmbed}};
use songbird::Call;
use tokio::sync::Mutex;

pub async fn send_timed_reply<S: Into<String>>(ctx: &Context<'_>, description: S, delay: Option<std::time::Duration>) -> Result<(), DynError> {
    let reply_handle = ctx.send(
        CreateReply::default()
        .ephemeral(true)
        .reply(true)
        .allowed_mentions(CreateAllowedMentions::new()
            .replied_user(true))
        .embed(CreateEmbed::new()
            .description(description)
            .color(Color::PURPLE))
    ).await?;
    ctx.data().add_to_cleanup(reply_handle, delay.unwrap_or(std::time::Duration::from_secs(5))).await;
    Ok(())

    
}

pub async fn send_timed_error<S: Into<String>>(ctx: &Context<'_>, description: S, delay: Option<std::time::Duration>) -> Result<(), DynError> {
    let reply_handle = ctx.send(
        CreateReply::default()
        .ephemeral(true)
        .reply(true)
        .allowed_mentions(CreateAllowedMentions::new()
            .replied_user(true))
        .embed(CreateEmbed::new()
            .title("Error")
            .description(description)
            .color(Color::from_rgb(255, 0, 0)))
    ).await?;
    ctx.data().add_to_cleanup(reply_handle, delay.unwrap_or(std::time::Duration::from_secs(5))).await;
    Ok(())
}

pub async fn same_voice_channel(guild: &Guild, user_id: &UserId, handler: Arc<Mutex<Call>>) -> bool {
    // user voice info
    let Some(user_voice) = guild.voice_states.get(user_id) else {
        return false;
    };
    let Some(user_voice_channel_id) = user_voice.channel_id else {
        return false;
    };

    // bot voice info
    let Some(bot_voice) = ({let handler_guard = handler.lock().await; handler_guard.current_connection().cloned()}) else {
        return false;
    };
    let Some(bot_voice_channel_id) = bot_voice.channel_id else {
        return false;
    };

    user_voice_channel_id.get() == bot_voice_channel_id.0.get()
}

pub async fn should_move_channels(ctx: &Context<'_>, guild: &Guild, user_voice: &serenity::model::voice::VoiceState) -> bool {
    // user voice info
    let Some(user_voice_channel_id) = user_voice.channel_id else {
        return false;
    };

    // bot voice info
    let Some(bot_voice) = guild.voice_states.get(&ctx.framework().bot_id) else {
        return true;
    };
    let Some(bot_voice_channel_id) = bot_voice.channel_id else {
        return true;
    };

    // channel id to guild voice channel
    let Ok(channel) = ctx
        .http()
        .get_channel(bot_voice_channel_id).await 
    else {
        return false;
    };
    let serenity::model::channel::Channel::Guild(guild_channel) = channel else {
        return false;
    };

    // get count of non bot users in voice
    let Ok(count) = guild_channel
        .members(ctx.cache())
        .map(|f| f.iter().filter(|p| !p.user.bot).count()) 
    else {
        return false;
    };
    
    // if count > 0 {we check if channels match} else {the channel is empty so we can move}
    if count > 0 {
        return user_voice_channel_id == bot_voice_channel_id;
    }

    true
}