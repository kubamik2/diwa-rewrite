use std::sync::Arc;

use crate::{data::Context, metadata::{LazyMetadata, TrackMetadata}, utils::format_duration};
use poise::{CreateReply, serenity_prelude::{ReactionType, ComponentInteraction}, ReplyHandle};
use serenity::builder::{CreateEmbed, CreateActionRow, CreateAllowedMentions, CreateButton, CreateEmbedAuthor, CreateEmbedFooter};
use futures::stream::*;
use songbird::{Call, tracks::LoopState};
use tokio::sync::Mutex;
use crate::commands::{ error::{VoiceError, CommandError}, utils::same_voice_channel };

// shows the currently playing track
#[poise::command(slash_command, prefix_command, guild_only, ephemeral, aliases("s"))]
pub async fn song(ctx: Context<'_>) -> Result<(), CommandError> {
    let guild = ctx.guild().unwrap().clone();
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;
    if let Some(handler) = manager.get(guild.id) {
        let currently_playing_msg = create_currently_playing_message(handler.clone()).await?;
        let reply_handle = ctx.send(currently_playing_msg).await?;

        let message = reply_handle.message().await?;
        let mut collector = message.await_component_interactions(ctx)
            .guild_id(guild.id)
            .channel_id(message.channel_id)
            .message_id(message.id)
            .timeout(std::time::Duration::from_secs(20))
            .author_id(ctx.author().id)
            .stream();

        while let Some(message_collector) = collector.next().await {
            match message_collector.data.custom_id.as_str() {
                "skip" => {
                    if same_voice_channel(&guild, &ctx.author().id, handler.clone()).await {
                        handler.lock().await.queue().skip()?;
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await; // waits for the queue to update
                        update_currently_playing_message(message_collector, &ctx, handler.clone(), &reply_handle).await?;
                    }
                },
                "loop" => {
                    let current_track_handle = handler.lock().await.queue().current();
                    if let Some(current_track_handle) = current_track_handle {
                        let info = current_track_handle.get_info().await?;
                        match info.loops {
                            LoopState::Infinite => current_track_handle.disable_loop()?,
                            LoopState::Finite(_) => current_track_handle.enable_loop()?
                        }
                    }
                    update_currently_playing_message(message_collector, &ctx, handler.clone(), &reply_handle).await?;
                },
                "refresh" => {
                    update_currently_playing_message(message_collector, &ctx, handler.clone(), &reply_handle).await?;
                },
                _ => ()
            }
        }
        ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(0)).await;
    }
    Ok(())
}

async fn update_currently_playing_message<'a>(message_collector: ComponentInteraction, ctx: &Context<'a>, handler: Arc<Mutex<Call>>, reply_handle: &ReplyHandle<'a>) -> Result<(), CommandError> {
    let edit = create_currently_playing_message(handler).await?;
    reply_handle.edit(ctx.clone(), edit).await?;
    let _ = message_collector.defer(ctx).await;
    Ok(())
}

async fn create_currently_playing_message<'a>(handler: Arc<Mutex<Call>>) -> Result<CreateReply, CommandError> {
    let mut currently_playing_msg = CreateReply::default().reply(true).allowed_mentions(CreateAllowedMentions::new().replied_user(true));

    let current_track_handle = handler.lock().await.queue().current(); // mutex dropped immediately
    match current_track_handle {
        Some(mut current_track_handle) => {
            let track_metadata = current_track_handle.read_generate_lazy_metadata().await?;
            match current_track_handle.get_info().await {
                Ok(info) => {
                    let looping = match info.loops {
                        LoopState::Infinite => true,
                        LoopState::Finite(_) => false
                    };

                    currently_playing_msg = currently_playing_msg
                        .embed(create_currently_playing_embed(track_metadata, info.play_time, looping))
                        .components(vec![create_buttons()]);
                },
                Err(_) => {
                    currently_playing_msg = currently_playing_msg
                        .embed(create_currently_playing_embed(track_metadata, std::time::Duration::ZERO, false))
                        .components(vec![create_buttons()]);
                }
            }
        },
        None => {
            currently_playing_msg = currently_playing_msg
                .embed(CreateEmbed::new()
                    .title("Currently Playing:")
                    .description("*Nothing*")
                    .footer(CreateEmbedFooter::new("looping: false")))
                .components(vec![create_buttons()]);
        }
    }
    Ok(currently_playing_msg)
}

fn create_buttons() -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new("skip").emoji(ReactionType::Unicode("⏭️".to_owned())),
        CreateButton::new("loop").emoji(ReactionType::Unicode("🔁".to_owned())),
        CreateButton::new("refresh").emoji(ReactionType::Unicode("🔄".to_owned())),
    ])
}

pub fn create_currently_playing_embed(track_metadata: TrackMetadata, playtime: std::time::Duration, looping: bool) -> CreateEmbed {
    let TrackMetadata { added_by, video_metadata } = track_metadata;
    let duration_string = format_duration(video_metadata.duration, None);
    let playtime_string = format_duration(playtime, Some(duration_string.len()));

    CreateEmbed::default()
    .title("Currently Playing:")
    .description( match video_metadata.audio_source {
        crate::metadata::AudioSource::File { path: _ } => {
            format!("{} | {} / {}", video_metadata.title, playtime_string, duration_string)
        },
        crate::metadata::AudioSource::YouTube { video_id } => {
            format!("[{}](https://youtu.be/{}) | {} / {}", video_metadata.title, video_id, playtime_string, duration_string)
        },
        crate::metadata::AudioSource::Jeja { .. } => video_metadata.title.clone()
    })
    .author({
        let mut author = CreateEmbedAuthor::new(added_by.name)
            .url(format!("https://discordapp.com/users/{}", added_by.id));
        if let Some(avatar_url) = added_by.avatar_url {
            author = author.icon_url(avatar_url);
        }
        author
    })
    .footer(CreateEmbedFooter::new(format!("looping: {}", looping)))
}