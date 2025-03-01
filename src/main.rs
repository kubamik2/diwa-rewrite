pub mod error;
pub mod api_integration;
pub mod scrapers;
pub mod convert_query;
pub mod metadata;
pub mod utils;
pub mod data;

use commands::error::CommandError;
use error::{DynError, AppError};
use data::{Data, Context};
use poise::FrameworkError;
use serenity::prelude::*;
use songbird::SerenityInit;
mod commands;

#[tokio::main]
async fn main() -> Result<(), DynError> {
    if let Some(path_string) = std::env::args().nth(1) {
        let path = std::path::Path::new(&path_string);
        if !path.exists() { return Err(AppError::EnvFile.into()) }
        dotenv::from_path(path).map_err(|_| AppError::EnvFile)?;
    } else {
        dotenv::dotenv().map_err(|_| AppError::EnvFile)?;
    }
    env_logger::init();
    
    let youtube_client = api_integration::youtube::YouTubeClient::new().await?;
    let spotify_client = api_integration::spotify::SpotifyClient::new()?;

    let token = std::env::var("DISCORD_TOKEN").map_err(|_| AppError::EnvVarsMissing { var: vec!["DISCORD_TOKEN".to_string()] })?;
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT
                                | GatewayIntents::GUILD_VOICE_STATES | GatewayIntents::GUILD_MEMBERS
                                | GatewayIntents::DIRECT_MESSAGES | GatewayIntents::GUILD_PRESENCES
                                | GatewayIntents::GUILDS;
    let framework: poise::Framework<Data, CommandError> = poise::Framework::builder()
        .options(poise::FrameworkOptions { 
            commands: vec![
                commands::play::play(),
                commands::queue::queue(),
                commands::song::song(),
                commands::leave::leave(),
                commands::pause::pause(),
                commands::resume::resume(),
                commands::skip::skip(),
                commands::jeja::jeja(),
                commands::_loop::_loop(),
                commands::stop::stop(),
                commands::register::register(),
                commands::help::help(),
                commands::join::join()
            ],
            prefix_options: poise::PrefixFrameworkOptions { prefix: Some("-".to_owned()), ..Default::default() },
            post_command: |ctx| Box::pin(post_command(ctx)),
            on_error: |err| Box::pin(on_error(err)),
            event_handler: |ctx, event, framework_ctx, data| Box::pin(event_handler(ctx, event, framework_ctx, data)),
            ..Default::default()})
        .initialize_owners(true)
        .setup(|_, ready, _| {
            Box::pin(async move {
                println!("{} Has Connected To Discord", ready.user.tag());
                Ok(Data::new(spotify_client, youtube_client))
            })
        })
        .build();

    let mut client = poise::serenity_prelude::ClientBuilder::new(token, intents).register_songbird().framework(framework).await?;
    client.start().await?;
    Ok(())
}

async fn post_command<'a>(ctx: Context<'a>) {
    let mut cleanups = ctx.data().cleanups.lock().await.clone();
    cleanups.sort_by(|a, b| b.delay.cmp(&a.delay));
    let mut time_slept = std::time::Duration::ZERO;
    while let Some(cleanup) = cleanups.pop() {
        tokio::time::sleep(cleanup.delay - time_slept).await;
        time_slept = cleanup.delay;
        let _ = cleanup.message.delete(&ctx.serenity_context().http).await;
    }
}

async fn on_error<'a>(err: FrameworkError<'a, Data, CommandError>) {
    log::error!("{:?}", err.to_string());
    match err {
        FrameworkError::Command { error, ctx, .. } => {
            let message = error.to_string();
            if message.is_empty() {
                let _ = commands::utils::send_timed_error(&ctx, "An error has occured, try again", Some(std::time::Duration::from_secs(10))).await;
            } else {
                let _ = commands::utils::send_timed_error(&ctx, message, Some(std::time::Duration::from_secs(10))).await;
            }
        },
        _ => ()
    }
}

async fn event_handler<'a>(ctx: &serenity::prelude::Context, event: &poise::serenity_prelude::FullEvent, framework_ctx: poise::dispatch::FrameworkContext<'a, Data, CommandError>, data: &Data) -> Result<(), CommandError> {
    match event {
        poise::serenity_prelude::FullEvent::VoiceStateUpdate { old, new } => {
            if new.user_id == framework_ctx.bot_id { return Ok(());}
            if let Some(old) = old {
                let Some(guild_id) = new.guild_id else { return Ok(());};

                if new.channel_id.is_some() { return Ok(());}

                let Some(manager) = songbird::get(ctx).await else { return Ok(());};
                let Some(handler) = manager.get(guild_id) else { return Ok(());};

                let Some(channel_id) = old.channel_id else { return Ok(());};
                let Some(bot_channel_id) = handler.lock().await.current_channel() else { return Ok(());};

                if bot_channel_id.0.get()== channel_id.get() {
                    let Ok(channel) = ctx.http.get_channel(serenity::all::ChannelId::new(bot_channel_id.0.into())).await else { return Ok(());};
                    if let poise::serenity_prelude::Channel::Guild(guild_channel) = channel {
                        if guild_channel.kind == poise::serenity_prelude::ChannelType::Voice {
                            let members_in_voice = guild_channel.members(ctx).map(|v| v.iter().filter(|p| !p.user.bot).count()).unwrap_or(0);
                            if members_in_voice == 0 {
                                let abort_handle = tokio::task::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                                    let _ = manager.remove(guild_id).await;
                                }).abort_handle();

                                data.afk_timeout_abort_handle_map.lock().await.insert(guild_id.get(), abort_handle);
                            }
                        }
                    }
                }
            } else {
                if new.channel_id.is_none() { return Ok(());}

                let Some(guild_id) = new.guild_id else { return Ok(());};
                
                let Some(manager) = songbird::get(ctx).await else { return Ok(());};
                let Some(handler) = manager.get(guild_id) else { return Ok(());};

                let Some(channel_id) = new.channel_id else { return Ok(());};
                let Some(bot_channel_id) = handler.lock().await.current_channel() else { return Ok(());};

                if bot_channel_id.0.get() == channel_id.get() {
                    let mut afk_timeout_abort_handle_map_guard = data.afk_timeout_abort_handle_map.lock().await;
                    if let Some(abort_handle) = afk_timeout_abort_handle_map_guard.get(&guild_id.get()) {
                        abort_handle.abort();
                        afk_timeout_abort_handle_map_guard.remove(&guild_id.get());
                    }
                }
            }
        },
        _ => ()
    }
    Ok(())
}
