use diwa::{ Context, error::Error };

#[poise::command(slash_command, prefix_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    ctx.send(|reply| reply.content("Pong!")).await?;
    Ok(())
}