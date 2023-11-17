use std::sync::Arc;
use taomi::utils;
use tokio::{
  signal,
  sync::{
    watch::{self, Receiver, Sender},
    Mutex,
  },
  task::JoinSet,
};
use twilight_gateway::{
  stream::{self, StartRecommendedError},
  CloseFrame, Config, ConfigBuilder, Intents, Shard,
};
use twilight_http::Client;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let configuration_result = utils::load_dotenv_config();

  match configuration_result {
    Ok(configuration) => {
      let shards = create_client(&configuration).await;

      match shards {
        Ok((client, shards)) => {
          let (tx, rx) = watch::channel(false);
          let set = JoinSet::new();

          handle_shards(client.clone(), shards, (tx, rx), set).await;
        }
        Err(error) => println!("{error}"),
      }
    }
    Err(error) => println!("{error}"),
  }
  Ok(())
}

async fn create_client(configuration: &utils::DotenvConfiguration) -> Result<(Arc<Client>, Vec<Shard>), StartRecommendedError> {
  let client = Arc::new(Client::new(configuration.client_token.clone()));
  let client_configuration = Config::new(configuration.client_token.clone(), Intents::GUILDS | Intents::GUILD_MESSAGES);
  let configuration_callback = |_, builder: ConfigBuilder| builder.build();

  let shards_result = stream::create_recommended(&client, client_configuration, &configuration_callback).await;

  match shards_result {
    Ok(shards) => Ok((client, shards.collect::<Vec<_>>())),
    Err(error) => Err(error),
  }
}

async fn handle_shards(client: Arc<Client>, shards: Vec<Shard>, (tx, rx): (Sender<bool>, Receiver<bool>), mut set: JoinSet<()>) {
  for shard in shards {
    let mut rx = rx.clone();
    let shard = Arc::new(Mutex::new(shard));
    let client = client.clone();

    set.spawn(async move {
      tokio::select! {
        _ = runner(client, shard.clone()) => {},
        _ = rx.changed() => {
          _ = shard.lock().await.close(CloseFrame::NORMAL).await;
        },
      }
    });
  }

  let shutdown = signal::ctrl_c().await;
  if let Ok(_) = shutdown {
    let close = tx.send(true);
    if let Err(error) = close {
      println!("An error has ocurred while clossing shards: {error}");
    }

    while set.join_next().await.is_some() {}
  }
}

async fn runner(client: Arc<Client>, shard: Arc<Mutex<Shard>>) {
  loop {
    let mut locked_shard = shard.lock().await;
    let event = match locked_shard.next_event().await {
      Ok(event) => event,
      Err(source) => {
        if source.is_fatal() {
          break;
        }

        continue;
      }
    };

    taomi::events::handler(client.clone(), shard.clone(), event).await;
  }
}
