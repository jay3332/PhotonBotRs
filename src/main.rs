#![feature(async_closure)]

mod resolve_image;
use resolve_image::ImageResolver;

use dotenv::dotenv;
use image::codecs::{png::PngEncoder, gif::{GifDecoder, GifEncoder}};
use image::AnimationDecoder;

use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler as BaseEventHandler};
use serenity::framework::standard::{
    Args,
    StandardFramework,
    CommandGroup,
    CommandResult,
    HelpOptions,
    help_commands,
    macros::{command, help, hook, group},
};
use serenity::model::{channel::Message, gateway::Ready, id::UserId};

use std::collections::hash_set::HashSet;

#[group]
#[commands(ping)]
struct Miscellaneous;

#[group]
#[commands(try_image, invert)]
struct Imaging;

struct EventHandler;

#[async_trait]
impl BaseEventHandler for EventHandler {
    async fn ready(&self, _: Context, data: Ready) {
        println!("Logged in as {} ({})", data.user.tag(), data.user.id);
    }
}

#[hook]
async fn after_hook(ctx: &Context, message: &Message, cmd_name: &str, result: CommandResult) {
    if let Err(why) = result {
        let _ = message.reply(ctx, format!("Error occured in `{}`: {}", cmd_name, why)).await;
    }
}

#[tokio::main]
async fn main() {
    dotenv().ok();

    let token = std::env::var("TOKEN")
        .expect("Missing environment variable 'TOKEN'");

    Client::builder(token)
        .application_id(914283059501735977_u64)
        .event_handler(EventHandler)
        .framework(
            StandardFramework::new()
                .configure(|config|
                    config.prefix("pt").allow_dm(false).with_whitespace(true)
                )
                .after(after_hook)
                .group(&MISCELLANEOUS_GROUP)
                .group(&IMAGING_GROUP)
                .help(&HELP_COMMAND)
        )
        .intents(serenity::client::bridge::gateway::GatewayIntents::non_privileged())
        .await
        .expect("Could not configure client")
        .start()
        .await
        .expect("Could not start client");
}

#[help]
async fn help_command(
   context: &Context,
   message: &Message,
   args: Args,
   help_options: &'static HelpOptions,
   groups: &[&'static CommandGroup],
   owners: HashSet<UserId>,
) -> CommandResult {
    help_commands::with_embeds(context, message, args, help_options, groups, owners).await;
    Ok(())
}

#[command]
async fn ping(ctx: &Context, message: &Message) -> CommandResult {
    let instant = std::time::Instant::now();

    let mut msg = message.reply(ctx, "Please wait...").await?;
    msg.edit(ctx, |m| m.content(format!("Pong! Latency: {} ms", instant.elapsed().as_millis()))).await?;

    Ok(())
}

#[command]
async fn try_image(ctx: &Context, message: &Message, mut args: Args) -> CommandResult {
    let resolver = ImageResolver::new();
    let query = args.single_quoted::<String>().ok();
    
    let result = resolver.resolve(ctx, message, query).await?;
    message.channel_id.send_message(ctx, |m| m.add_file((result.as_slice(), "my_file.gif"))).await?;

    Ok(())
}

fn is_gif(data: &Vec<u8>) -> bool {
    &data[0..6] == b"\x47\x49\x46\x38\x39\x61" || &data[0..6] == b"\x47\x49\x46\x38\x37\x61"
}

#[command]
async fn invert(ctx: &Context, message: &Message, mut args: Args) -> CommandResult {
    let resolver = ImageResolver::new();
    let query = args.single_quoted::<String>().ok();
    
    let typing = message.channel_id.start_typing(&ctx.http)?;
    let result = resolver.resolve(ctx, message, query).await?;
    if is_gif(&result) {
        let data = tokio::task::spawn_blocking(move || -> CommandResult<std::io::Cursor<Vec<u8>>> {
            let decoder = GifDecoder::new(result.as_slice()).unwrap();
            let frames = decoder.into_frames().filter(|f| f.is_ok()).map(|f| {
                let mut frame = f.unwrap().clone();
                let buffer = frame.buffer_mut();
                image::imageops::invert(buffer);
                frame
            });
    
            let mut buffer = std::io::Cursor::new(vec![]);
            GifEncoder::new(&mut buffer).encode_frames(frames).unwrap();

            Ok(buffer.clone())
        }).await?.unwrap();

        let encoded = data.into_inner();
        message.channel_id.send_message(ctx, |m| m.add_file((encoded.as_slice(), "my_file.gif"))).await?;

        typing.stop();
        return Ok(());
    }

    let mut img = image::load_from_memory(result.as_slice())?.into_rgba8();

    image::imageops::invert(&mut img);

    let mut buffer = std::io::Cursor::new(vec![]);
    let encoder = PngEncoder::new(&mut buffer);

    let (width, height) = img.dimensions();
    let bytes = img.into_raw();

    let _ = encoder.encode(
        bytes.as_slice(),
        width,
        height,
        image::ColorType::Rgba8,
    );

    let encoded = buffer.into_inner();

    message.channel_id.send_message(ctx, |m| m.add_file((encoded.as_slice(), "invert.png"))).await?;

    typing.stop();
    Ok(())
}
