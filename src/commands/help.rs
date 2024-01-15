use diwa::{ Context, error::Error };


#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn help(ctx: Context<'_>) -> Result<(), Error> {
    poise::builtins::help(ctx, Some("play"), poise::builtins::HelpConfiguration {
        extra_text_at_bottom: "",
        ephemeral: true,
        ..Default::default()
    }).await?;
    Ok(())
}