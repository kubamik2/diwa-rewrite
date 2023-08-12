use diwa::error::{Error, AppError};
use serenity::prelude::*;
use songbird::SerenityInit;
mod commands;
static DISCORD_TOKEN_ENV: &str = "DISCORD_TOKEN_TESTS";

#[tokio::main]
async fn main() -> Result<(), Error> {
    dotenv::dotenv()?;
    std::fs::File::create("test3").unwrap();
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
                commands::ping::ping(),
                commands::play::play()
            ],
            prefix_options: poise::PrefixFrameworkOptions { prefix: Some("-".to_owned()), ..Default::default() },
            ..Default::default()})
        .token(token)
        .intents(intents)
        .setup(|ctx, ready, framework| {
            Box::pin(async move {
                println!("{} Has Connected To Discord", ready.user.tag());
                poise::builtins::register_in_guild(&ctx.http, &framework.options().commands, serenity::model::id::GuildId(883721114604404757)).await?;
                Ok(diwa::Data { spotify_client, youtube_client })
            })
        })
        .client_settings(|client_settings| client_settings.register_songbird());
    framework.run().await.unwrap();
    Ok(())
}