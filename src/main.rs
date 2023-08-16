use diwa::error::{Error, AppError};
use serenity::prelude::*;
use songbird::SerenityInit;
mod commands;
static DISCORD_TOKEN_ENV: &str = "DISCORD_TOKEN_TESTS";

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv::dotenv()?;
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
                commands::queue::queue()
            ],
            prefix_options: poise::PrefixFrameworkOptions { prefix: Some("-".to_owned()), ..Default::default() },
            post_command: |ctx| Box::pin(post_command(ctx)),
            ..Default::default()})
        .token(token)
        .intents(intents)
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                println!("{} Has Connected To Discord", ready.user.tag());
                poise::builtins::register_in_guild(&ctx.http, &framework.options().commands, serenity::model::id::GuildId(883721114604404757)).await?;
                Ok(diwa::Data { cleanups: tokio::sync::Mutex::new(vec![]), spotify_client, youtube_client })
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