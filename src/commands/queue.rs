use crate::{data::Context, metadata::LazyMetadata};
use poise::{serenity_prelude::{ReactionType, ComponentInteraction}, ReplyHandle, CreateReply};
use serenity::{ builder::{ CreateEmbed, CreateActionRow, CreateEmbedFooter, CreateButton, CreateAllowedMentions }, model::Color };
use std::{ sync::Arc, time::Duration };
use tokio::sync::Mutex;
use songbird::{ Call, tracks::LoopState };
use crate::commands::error::{VoiceError, CommandError};
use futures::stream::*;

const TRACKS_PER_PAGE: usize = 7;
const CHARACTERS_PER_FIELD_LINE: usize = 1024 / TRACKS_PER_PAGE - 3; // -3 to account enumeration formatting and a new line

// shows the queue
#[poise::command(slash_command, prefix_command, guild_only, ephemeral, aliases("q"))]
pub async fn queue(ctx: Context<'_>, page: Option<usize>) -> Result<(), CommandError> {
    let mut page = page.unwrap_or(1).max(1);
    page -= 1; // represent the page as an index

    let guild = ctx.guild().unwrap().clone();
    let manager = songbird::get(&ctx.serenity_context()).await.ok_or(VoiceError::NoManager)?;

    if let Some(handler) = manager.get(guild.id) {
        let (queue_embed, mut last_page) = assemble_embed(handler.clone(), page).await;
        let reply_handle = ctx.send(CreateReply::default()
            .reply(true)
            .allowed_mentions(CreateAllowedMentions::new()
                .replied_user(true))
            .embed(queue_embed)
            .components(vec![create_buttons(page, last_page)])
        ).await?;

        let mut collector = reply_handle.message().await?.await_component_interactions(ctx)
            .timeout(Duration::from_secs(30))
            .author_id(ctx.author().id)
            .stream();

        while let Some(message_collector) = collector.next().await {
            match message_collector.data.custom_id.as_str() { // ?? ignore error or return
                "prev" => {
                    page -= 1;
                    update_queue_embed(page, &mut last_page, ctx, handler.clone(), &reply_handle, message_collector).await?;
                },
                "next" => {
                    page += 1;
                    update_queue_embed(page, &mut last_page, ctx, handler.clone(), &reply_handle, message_collector).await?;
                },
                "reload" => {
                    update_queue_embed(page, &mut last_page, ctx, handler.clone(), &reply_handle, message_collector).await?;
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
    let mut embed = CreateEmbed::default()
        .color(Color::PURPLE)
        .title("Queue")
        .footer(CreateEmbedFooter::new(format!("Page: {}/{}  Tracks: {}   looping: {}", page + 1, last_page.max(1), queue_len, looping)));

    let mut next_up = String::new();
    embed = embed.field("Currently Playing:", stringified_metadatas.first().unwrap_or(&"*Nothing*".to_owned()), false);

    if stringified_metadatas.len() > 1 {
        for (i, stringified_metadata) in stringified_metadatas.iter().enumerate().skip(1) {
            next_up.push_str(&format!("{}. {}\n", i + (page * TRACKS_PER_PAGE), stringified_metadata));
        }
    } else {
        next_up = "*Nothing*".to_owned();
    }

    embed = embed.field("Next Up:", next_up, false);
    embed
}

async fn assemble_embed(handler: Arc<Mutex<Call>>, page: usize) -> (CreateEmbed, usize) {
    search_burst(handler.clone(), page).await;

    let handler_guard = handler.lock().await;
    let queue_len = handler_guard.queue().len();
    let queue = handler_guard.queue().current_queue().into_iter().skip(1 + (TRACKS_PER_PAGE * page) as usize);
    let current_track = handler_guard.queue().current();
    drop(handler_guard);

    let mut stringified_metadatas: Vec<String> = vec![];
    let mut looping = false;

    // assemble the currently playing field
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
        let stringified_metadata = track_metadata.video_metadata.to_queue_string(playtime, None);
        stringified_metadatas.push(stringified_metadata);
    }

    // assemble queued tracks
    for (i, track_handle) in queue.enumerate() {
        if i == TRACKS_PER_PAGE { break; }
        let track_metadata = match track_handle.read_lazy_metadata().await {
            Some(track_metadata) => track_metadata,
            None => continue
        };

        let max_num_on_page_length = (TRACKS_PER_PAGE + (page * TRACKS_PER_PAGE)) / 100 + 1; // we do this to ensure that the queue_string length limit is correct event with high track enumeration values
        let stringified_metadata = track_metadata.video_metadata.to_queue_string(None, Some(CHARACTERS_PER_FIELD_LINE - max_num_on_page_length));
        stringified_metadatas.push(stringified_metadata);
    }
    let last_page = ((queue_len.max(1) - 1) as f32 / TRACKS_PER_PAGE as f32).ceil() as usize;
    (create_queue_embed(stringified_metadatas, page, last_page, queue_len, looping), last_page)
}

pub fn create_buttons(page: usize, last_page: usize) -> CreateActionRow {
    CreateActionRow::Buttons(vec![
        CreateButton::new("prev").emoji(ReactionType::Unicode("◀️".to_owned())).disabled(page == 0),
        CreateButton::new("next").emoji(ReactionType::Unicode("▶️".to_owned())).disabled(page + 1 >= last_page),
        CreateButton::new("reload").emoji(ReactionType::Unicode("🔄".to_owned()))
    ])
}

pub async fn update_queue_embed<'a>(page: usize, last_page: &mut usize, ctx: Context<'a>, handler: Arc<Mutex<Call>>, reply_handle: &ReplyHandle<'a>, message_collector: ComponentInteraction) -> Result<(), CommandError> {
    let (new_queue_embed, new_last_page) = assemble_embed(handler, page).await;
    *last_page = new_last_page;
    let _ = reply_handle.edit(ctx.clone(), CreateReply::default().embed(new_queue_embed).components(vec![create_buttons(page, *last_page)])).await?;
    let _ = message_collector.defer(ctx).await;
    Ok(())
}