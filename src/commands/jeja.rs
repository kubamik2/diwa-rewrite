use std::time::Duration;

use crate::{data::Context, metadata::{LazyMetadataEventHandler, LazyMetadata, TrackMetadata, VideoMetadata, UserMetadata}, commands::error::CommandError};
use poise::CreateReply;
use serenity::{builder::{CreateAllowedMentions, CreateEmbed}, model::Color};
use crate::commands::{error::VoiceError, utils::should_move_channels};

// tells a joke from jeja.pl
#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn jeja(ctx: Context<'_>) -> Result<(), CommandError> {
    let guild = ctx.guild().unwrap().clone();
    let user_voice = guild.voice_states.get(&ctx.author().id).ok_or(VoiceError::NotConnected)?;

    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;
    let handler = manager.get_or_insert(guild.id);

    if !should_move_channels(&ctx, &guild, user_voice).await { return Err(VoiceError::DifferentVoiceChannel.into()) }

    let connection = handler.lock().await.current_connection().and_then(|conn| conn.channel_id);

    let handler = manager.join(guild.id, user_voice.channel_id.unwrap()).await?;

    let was_empty = {
        let mut handler_guard = handler.lock().await;

        // add event handler upon joining a channel
        if connection.is_none() { 
            handler_guard.add_global_event(songbird::Event::Track(songbird::TrackEvent::Play), LazyMetadataEventHandler { handler: handler.clone(), channel_id: ctx.channel_id(), http: ctx.serenity_context().http.clone() });
        }

        let _ = handler_guard.deafen(true).await; 

        handler_guard.queue().is_empty()
    };

    let track_metadata = TrackMetadata {
        video_metadata: VideoMetadata {
            title: "Dowcip".to_string(),
            duration: Duration::from_secs(0),
            audio_source: crate::metadata::AudioSource::Jeja { filename: format!("{}.mp3", guild.id.get()) }
        },
        added_by: UserMetadata {
            name: ctx.author().name.clone(),
            avatar_url: ctx.author().avatar_url(),
            id: ctx.author().id.get()
        }
    };

    let input = crate::convert_query::YouTubeComposer::Metadata { metadata: track_metadata.video_metadata.clone(), client: ctx.data().reqwest_client.clone() }.into();
    let mut track_handle = handler.lock().await.enqueue_input(input).await;
    track_handle.write_lazy_metadata(track_metadata.clone()).await;

    if !was_empty {
        let reply_handle = ctx.send(CreateReply::default()
                .ephemeral(true)
                .reply(true)
                .allowed_mentions(CreateAllowedMentions::new()
                    .replied_user(true))
                .embed(CreateEmbed::new()
                    .title("Added Track:")
                    .description(track_metadata.video_metadata.title)
                    .color(Color::PURPLE))
            ).await?;
            ctx.data().add_to_cleanup(reply_handle, std::time::Duration::from_secs(10)).await;
    }
    Ok(())
}