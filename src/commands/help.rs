use crate::{data::Context, commands::error::CommandError};


#[poise::command(slash_command, prefix_command, guild_only)]
pub async fn help(ctx: Context<'_>, command: Option<String>) -> Result<(), CommandError> {
    poise::builtins::help(ctx, command.as_deref(), poise::builtins::HelpConfiguration {
        extra_text_at_bottom: "",
        ephemeral: true,
        ..Default::default()
    }).await?;
    Ok(())
}