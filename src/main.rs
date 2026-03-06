mod backends;

use crate::backends::Backend;
use backends::dectalk::DECTalkBackend;

use std::env;

use serenity::{
    async_trait,
    client::{Client, Context, EventHandler},
    framework::{
        standard::{
            macros::{command, group},
            CommandResult, Configuration,
        },
        StandardFramework,
    },
    model::{channel::Message, gateway::Ready},
    prelude::{GatewayIntents, Mentionable},
    Result as SerenityResult,
};

use songbird::{error::JoinError, SerenityInit};

// This imports `typemap`'s `Key` as `TypeMapKey`.
use serenity::prelude::*;

struct Handler;

#[async_trait]
impl EventHandler for Handler {
    async fn ready(&self, _: Context, ready: Ready) {
        println!("{} is connected!", ready.user.name);
    }
}

struct DECTalkBackendKey;

impl TypeMapKey for DECTalkBackendKey {
    type Value = DECTalkBackend;
}

#[group]
#[commands(join, leave, speak)]
struct General;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let _ = rustls::crypto::ring::default_provider().install_default();

    // Configure the client with your Discord bot token in the environment.
    let token = env::var("DISCORD_TOKEN").expect("Expected a token in the environment");

    let framework = StandardFramework::new().group(&GENERAL_GROUP);
    framework.configure(Configuration::new().prefix("~"));

    let intents = GatewayIntents::non_privileged() | GatewayIntents::MESSAGE_CONTENT;

    let mut client = Client::builder(&token, intents)
        .event_handler(Handler)
        .framework(framework)
        .register_songbird()
        .await
        .expect("Err creating client");

    // Set up all the backends
    {
        // Get a lock on the data store
        let mut data = client.data.write().await;

        data.insert::<DECTalkBackendKey>(DECTalkBackend::new().await);
    }

    let _ = client
        .start()
        .await
        .map_err(|why| println!("Client ended: {:?}", why));
}

#[command]
#[only_in(guilds)]
async fn join(ctx: &Context, msg: &Message) -> CommandResult {
    let (guild_id, channel_id) = {
        let guild = msg.guild(&ctx.cache).unwrap();
        let channel_id = guild
            .voice_states
            .get(&msg.author.id)
            .and_then(|voice_state| voice_state.channel_id);

        (guild.id, channel_id)
    };

    let connect_to = match channel_id {
        Some(channel) => channel,
        None => {
            check_msg(msg.reply(ctx, "Not in a voice channel").await);

            return Ok(());
        }
    };

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    match manager.join(guild_id, connect_to).await {
        Ok(_handler) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, &format!("Joined {}", connect_to.mention()))
                    .await,
            );
        }
        Err(e) => {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, format!("Error joining the channel: {}", e))
                    .await,
            );

            if let JoinError::Driver(driver_e) = e {
                dbg!(driver_e);
            }
        }
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn leave(ctx: &Context, msg: &Message) -> CommandResult {
    let guild_id = msg.guild_id.unwrap();

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();
    let has_handler = manager.get(guild_id).is_some();

    if has_handler {
        if let Err(e) = manager.remove(guild_id).await {
            check_msg(
                msg.channel_id
                    .say(&ctx.http, format!("Failed: {:?}", e))
                    .await,
            );
        }

        check_msg(msg.channel_id.say(&ctx.http, "Left voice channel").await);
    } else {
        check_msg(msg.reply(ctx, "Not in a voice channel").await);
    }

    Ok(())
}

#[command]
#[only_in(guilds)]
async fn speak(ctx: &Context, msg: &Message) -> CommandResult {
    let guild_id = msg.guild_id.unwrap();

    let manager = songbird::get(ctx)
        .await
        .expect("Songbird Voice client placed in at initialisation.")
        .clone();

    if let Some(handler_lock) = manager.get(guild_id) {
        let mut handler = handler_lock.lock().await;

        let speech = {
            let data_read = ctx.data.read().await;

            let dectalk = data_read
                .get::<DECTalkBackendKey>()
                .expect("Expected DECTalkBackendKey in TypeMap.");

            dectalk.get_tts("testing dectalk").await
        };

        let speech_track = handler.play_input(speech);
        let _ = speech_track.set_volume(1.0);
    } else {
        check_msg(
            msg.channel_id
                .say(&ctx.http, "Not in a voice channel to speak in")
                .await,
        );
    }

    Ok(())
}

/// Checks that a message successfully sent; if not, then logs why to stdout.
fn check_msg(result: SerenityResult<Message>) {
    if let Err(why) = result {
        println!("Error sending message: {:?}", why);
    }
}
