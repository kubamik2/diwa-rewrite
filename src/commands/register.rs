use diwa::{ Context, error::Error };

// skips to the next track
#[poise::command(slash_command, prefix_command, owners_only)]
pub async fn register(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::register_application_commands_buttons(ctx).await;
    Ok(())
}