use diwa::{ Context, error::Error };
use serenity::utils::Color;

pub async fn send_timed_reply<S: ToString>(ctx: &Context<'_>, description: S, delay: Option<std::time::Duration>) -> Result<(), Error> {
    let reply_handle = ctx.send(|msg| msg
        .ephemeral(true)
        .reply(true)
        .allowed_mentions(|mentions| mentions.replied_user(true))
        .embed(|embed| embed
            .description(description)
            .color(Color::PURPLE))
    ).await?;
    ctx.data().add_to_cleanup(reply_handle, delay.unwrap_or(std::time::Duration::from_secs(5))).await;
    Ok(())
}

pub async fn send_timed_error<S: ToString>(ctx: &Context<'_>, description: S, delay: Option<std::time::Duration>) -> Result<(), Error> {
    let reply_handle = ctx.send(|msg| msg
        .ephemeral(true)
        .reply(true)
        .allowed_mentions(|mentions| mentions.replied_user(true))
        .embed(|embed| embed
            .title("Error")
            .description(description)
            .color(Color::RED))
    ).await?;
    ctx.data().add_to_cleanup(reply_handle, delay.unwrap_or(std::time::Duration::from_secs(5))).await;
    Ok(())
}