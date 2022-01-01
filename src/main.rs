#![feature(async_closure)]

mod resolve_image;

use dotenv::dotenv;

use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler as BaseEventHandler};
use serenity::framework::standard::{
    Args,
    StandardFramework,
    CommandGroup,
    CommandResult,
    HelpOptions,
    help_commands,
    macros::{command, help, group},
};
use serenity::model::{channel::Message, gateway::Ready, id::UserId};

use std::collections::hash_set::HashSet;

#[group]
#[commands(ping)]
struct Miscellaneous;

struct EventHandler;

#[async_trait]
impl BaseEventHandler for EventHandler {
    async fn ready(&self, _: Context, data: Ready) {
        println!("Logged in as {} ({})", data.user.tag(), data.user.id);
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
                    config.prefix("pt").allow_dm(false)
                )
                .group(&MISCELLANEOUS_GROUP)
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
