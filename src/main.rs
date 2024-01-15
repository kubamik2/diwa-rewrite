use diwa::{error::{Error, AppError}, Data};
use poise::FrameworkError;
use serenity::prelude::*;
use songbird::SerenityInit;
mod commands;
static DISCORD_TOKEN_ENV: &str = "DISCORD_TOKEN";

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv::dotenv().map_err(|_| AppError::EnvFile)?;
    let youtube_client = diwa::api_integration::youtube::YouTubeClient::new().await?;
    let spotify_client = diwa::api_integration::spotify::SpotifyClient::new()?;

    let token = std::env::var(DISCORD_TOKEN_ENV).map_err(|_| AppError::MissingEnvEntry { entry: DISCORD_TOKEN_ENV.to_owned() })?;
    let intents = GatewayIntents::GUILD_MESSAGES | GatewayIntents::MESSAGE_CONTENT
                                | GatewayIntents::GUILD_VOICE_STATES | GatewayIntents::GUILD_MEMBERS
                                | GatewayIntents::DIRECT_MESSAGES | GatewayIntents::GUILD_PRESENCES
                                | GatewayIntents::GUILDS;
    let framework: poise::FrameworkBuilder<diwa::Data, Error> = poise::Framework::builder()
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
                commands::help::help()
            ],
            prefix_options: poise::PrefixFrameworkOptions { prefix: Some("-".to_owned()), ..Default::default() },
            post_command: |ctx| Box::pin(post_command(ctx)),
            //on_error: |err| Box::pin(on_error(err)),
            event_handler: |ctx, event, framework_ctx, data| Box::pin(event_handler(ctx, event, framework_ctx, data)),
            ..Default::default()})
        .token(token)
        .intents(intents)
        .initialize_owners(true)
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                println!("{} Has Connected To Discord", ready.user.tag());
                
                poise::builtins::register_in_guild(&ctx.http, &framework.options().commands, serenity::model::id::GuildId(883721114604404757)).await?;
                Ok(diwa::Data::new(spotify_client, youtube_client))
            })
        })
        .client_settings(|client_settings| client_settings.register_songbird());
    
    framework.run().await.unwrap();
    Ok(())
}

async fn post_command<'a>(ctx: diwa::Context<'a>) {
    let mut cleanups = ctx.data().cleanups.lock().await.clone();
    cleanups.sort_by(|a, b| b.delay.cmp(&a.delay));
    let mut time_slept = std::time::Duration::ZERO;
    while let Some(cleanup) = cleanups.pop() {
        tokio::time::sleep(cleanup.delay - time_slept).await;
        time_slept = cleanup.delay;
        let _ = cleanup.message.delete(&ctx.serenity_context().http).await;
    }
}

// async fn on_error<'a>(err: FrameworkError<'a, Data, Error>) {
//     match err {
//         FrameworkError::Command { error, ctx } => {
//             let ve: Box<commands::error::VoiceError> = error.downcast().unwrap();
//             dbg!(ve);
//         },
//         _ => ()
//     }
// }

async fn event_handler<'a>(ctx: &serenity::prelude::Context, event: &poise::Event<'a>, framework_ctx: poise::dispatch::FrameworkContext<'a, Data, Error>, data: &Data) -> Result<(), Error> {
    match event {
        poise::Event::VoiceStateUpdate { old, new } => {
            if new.user_id.0 == framework_ctx.bot_id.0 { return Ok(());}
            if let Some(old) = old {
                let Some(guild_id) = new.guild_id else { return Ok(());};

                if new.channel_id.is_some() { return Ok(());}

                let Some(manager) = songbird::get(ctx).await else { return Ok(());};
                let Some(handler) = manager.get(guild_id) else { return Ok(());};

                let Some(channel_id) = old.channel_id else { return Ok(());};
                let Some(bot_channel_id) = handler.lock().await.current_channel() else { return Ok(());};

                if bot_channel_id.0 == channel_id.0 {
                    let Ok(channel) = ctx.http.get_channel(bot_channel_id.0).await else { return Ok(());};
                    if let poise::serenity_prelude::Channel::Guild(guild_channel) = channel {
                        if guild_channel.kind == poise::serenity_prelude::ChannelType::Voice {
                            let members_in_voice = guild_channel.members(ctx).await.map(|v| v.iter().filter(|p| !p.user.bot).count()).unwrap_or(0);
                            if members_in_voice == 0 {
                                let abort_handle = tokio::task::spawn(async move {
                                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                                    manager.remove(guild_id).await;
                                }).abort_handle();

                                data.afk_timeout_abort_handle_map.lock().await.insert(guild_id.0, abort_handle);
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

                if bot_channel_id.0 == channel_id.0 {
                    let mut afk_timeout_abort_handle_map_guard = data.afk_timeout_abort_handle_map.lock().await;
                    if let Some(abort_handle) = afk_timeout_abort_handle_map_guard.get(&guild_id.0) {
                        abort_handle.abort();
                        afk_timeout_abort_handle_map_guard.remove(&guild_id.0);
                    }
                }
            }
        },
        _ => ()
    }
    Ok(())
}