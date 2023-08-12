use diwa::{ Context, error::Error };
use serenity::utils::Color;

pub fn format_duration(duration: std::time::Duration, length: Option<u32>) -> String {
    let s = duration.as_secs() % 60;
    let m = duration.as_secs() / 60 % 60;
    let h = duration.as_secs() / 3600 % 24;
    let d = duration.as_secs() / 86400;
    let mut formatted_duration = format!("{:0>2}:{:0>2}:{:0>2}:{:0>2}", d, h, m, s);
    if let Some(length) = length {
        formatted_duration = formatted_duration.split_at(formatted_duration.len() - length as usize).1.to_owned();
    } else {
        while formatted_duration.len() > 5 {
            if let Some(stripped_formatted_duration) = formatted_duration.strip_prefix("00:") {
                formatted_duration = stripped_formatted_duration.to_owned();
            }
        }
    }
    formatted_duration
}

pub async fn send_timed_reply(ctx: &Context<'_>, description: String, delay: Option<std::time::Duration>) -> Result<(), Error> {
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

pub async fn send_timed_error(ctx: &Context<'_>, description: String, delay: Option<std::time::Duration>) -> Result<(), Error> {
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