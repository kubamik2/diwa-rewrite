use diwa::{ Context, error::Error, metadata::LazyMetadata };
use poise::serenity_prelude::{ ReactionType, MessageComponentInteraction };
use serenity::{ builder::{ CreateEmbed, CreateActionRow }, utils::Color };
use std::{ sync::Arc, time::Duration };
use tokio::sync::Mutex;
use songbird::{ Call, tracks::LoopState };
use crate::commands::error::VoiceError;
use futures::stream::*;

static TRACKS_PER_PAGE: usize = 10;

#[poise::command(slash_command, prefix_command, guild_only, ephemeral)]
pub async fn queue(ctx: Context<'_>, page: Option<usize>) -> Result<(), Error> {
    let mut page = page.unwrap_or(1).max(1);
    page -= 1;
    let guild = ctx.guild().unwrap();
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::ManagerNone)?;
    if let Some(handler) = manager.get(guild.id) {
        let (queue_embed, mut last_page) = assemble_embed(handler.clone(), page).await;
        let reply_handle = ctx.send(|msg| msg
            .reply(true)
            .allowed_mentions(|mentions| mentions.replied_user(true))
            .embed(|embed| {embed.clone_from(&queue_embed); embed})
            .components(|components| components.set_action_row(create_buttons(page, last_page))) 
        ).await?;
        let mut collector = reply_handle.message().await?.await_component_interactions(ctx)
            .timeout(Duration::from_secs(30))
            .author_id(ctx.author().id)
            .build();
        while let Some(message_collector) = collector.next().await {
            match message_collector.data.custom_id.as_str() {
                "prev" => {
                    page -= 1;
                    update_queue_embed(page, &mut last_page, message_collector, ctx, handler.clone()).await;
                },
                "next" => {
                    page += 1;
                    update_queue_embed(page, &mut last_page, message_collector, ctx, handler.clone()).await;
                },
                "reload" => {
                    update_queue_embed(page, &mut last_page, message_collector, ctx, handler.clone()).await;
                }
                _ => ()
            }
        } 
        ctx.data().add_to_cleanup(reply_handle, Duration::ZERO).await;
    }
    Ok(())
}

async fn search_burst(handler: Arc<Mutex<Call>>, page: usize) {
    let mut threads = vec![];
    let queue = handler.lock().await.queue().current_queue().into_iter().skip(1 + (TRACKS_PER_PAGE * page) as usize);
    for (i, mut track_handle) in queue.enumerate() {
        if i == TRACKS_PER_PAGE { break; }
        threads.push(tokio::task::spawn(async move {
            let _ = track_handle.awake_lazy_metadata().await;
        }));
    }

    for thread in threads {
        let _ = thread.await;
    }
}

pub fn create_queue_embed(stringified_metadatas: Vec<String>, page: usize, last_page: usize, queue_len: usize, looping: bool) -> CreateEmbed {
    let mut embed = CreateEmbed::default();
    embed.color(Color::PURPLE);
    embed.title("Queue").footer(|footer| footer.text(format!("Page: {}/{}  Tracks: {}   looping: {}", page + 1, last_page.max(1), queue_len, looping)));
    let mut next_up = String::new();
    embed.field("Currently Playing:", stringified_metadatas.first().unwrap_or(&"*Nothing*".to_owned()), false);

    if stringified_metadatas.len() > 1 {
        for (i, stringified_metadata) in stringified_metadatas.iter().enumerate().skip(1) {
            next_up.push_str(&format!("{}. {}\n", i + (page * TRACKS_PER_PAGE), stringified_metadata));
        }
    } else {
        next_up = "*Nothing*".to_owned();
    }
    embed.field("Next Up:", next_up, false);
    embed
}

async fn assemble_embed(handler: Arc<Mutex<Call>>, page: usize) -> (CreateEmbed, usize) {
    search_burst(handler.clone(), page).await;
    let handler_guard = handler.lock().await;
    let queue_len = handler_guard.queue().len();
    let mut stringified_metadatas: Vec<String> = vec![];
    let queue = handler_guard.queue().current_queue().into_iter().skip(1 + (TRACKS_PER_PAGE * page) as usize);
    let current_track = handler_guard.queue().current();
    drop(handler_guard);
    let mut looping = false;
    if let Some(current_track) = current_track {
        let track_metadata = current_track.read_lazy_metadata().await.unwrap_or_default();
        let playtime = match current_track.get_info().await {
            Ok(info) => {
                match info.loops{
                    LoopState::Infinite => looping = true,
                    _ => ()
                }
                Some(info.play_time)
            },
            Err(_) => None
        };
        let stringified_metadata = track_metadata.video_metadata.to_queue_string(playtime);
        stringified_metadatas.push(stringified_metadata);
    }

    for (i, track_handle) in queue.enumerate() {
        if i == TRACKS_PER_PAGE { break; }
        let track_metadata = match track_handle.read_lazy_metadata().await {
            Some(track_metadata) => track_metadata,
            None => continue
        };
        let stringified_metadata = track_metadata.video_metadata.to_queue_string(None);
        stringified_metadatas.push(stringified_metadata);
    }
    let last_page = ((queue_len - 1) as f32 / TRACKS_PER_PAGE as f32).ceil() as usize;
    (create_queue_embed(stringified_metadatas, page, last_page, queue_len, looping), last_page)
}

pub fn create_buttons(page: usize, last_page: usize) -> CreateActionRow {
    let mut components = CreateActionRow::default();
    components.create_button(|button| button.custom_id("prev").emoji(ReactionType::Unicode("◀️".to_owned())).disabled(page == 0));
    components.create_button(|button| button.custom_id("next").emoji(ReactionType::Unicode("▶️".to_owned())).disabled(page + 1 >= last_page));
    components.create_button(|button| button.custom_id("reload").emoji(ReactionType::Unicode("🔄".to_owned())));
    components
}

pub async fn update_queue_embed(page: usize, last_page: &mut usize, message_collector: Arc<MessageComponentInteraction>, ctx: Context<'_>, handler: Arc<Mutex<Call>>) {
    let channel_id = message_collector.channel_id.0;
    let message_id = message_collector.message.id.0;
    if let Ok(mut message) = ctx.serenity_context().http.get_message(channel_id, message_id).await {
        let (new_queue_embed, new_last_page) = assemble_embed(handler, page).await;
        *last_page = new_last_page;
        let _ = message.edit(ctx, |f| f.set_embed(new_queue_embed).components(|components| components.set_action_row(create_buttons(page, *last_page)))).await;
        let _ = message_collector.defer(ctx).await;
    }
}