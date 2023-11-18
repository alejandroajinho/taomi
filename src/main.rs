use futures_util::StreamExt;
use std::sync::Arc;
use taomi::utils;
use tokio::{
  signal,
  sync::{
    watch::{self, Receiver, Sender},
    Mutex,
  },
  task::JoinSet,
  time::{self, Duration},
};
use twilight_gateway::{
  stream::{self, ShardMessageStream, StartRecommendedError},
  CloseFrame, Config, ConfigBuilder, Intents, Shard, ShardId,
};
use twilight_http::Client;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let configuration_result = utils::load_dotenv_config();

  match configuration_result {
    Ok(configuration) => {
      let client_configuration = Config::new(configuration.client_token.clone(), Intents::GUILDS | Intents::GUILD_MESSAGES);
      let configuration_callback = |_, builder: ConfigBuilder| builder.build();
      let shards = create_client(&configuration, client_configuration.clone(), configuration_callback).await;

      match shards {
        Ok((client, mut shards)) => loop {
          let (shutdown_tx, shutdow_rx) = watch::channel(false);
          let (reshard_tx, reshard_rx) = watch::channel(false);

          let set = Arc::new(Mutex::new(JoinSet::new()));
          tokio::select! {
            _ = handle_shards(client.clone(), shards, (shutdown_tx, shutdow_rx, reshard_rx.clone()), set.clone()) => break,
            Ok(Some(new_shards)) = reshard(client.clone(), client_configuration.clone(), configuration_callback) => {
              let reshard_close = reshard_tx.send(true);
              if let Err(error) = reshard_close {
                println!("An error has ocurred while closing shards: {error}");
              }
              while set.lock().await.join_next().await.is_some() {}
                shards = new_shards;
            }
          }
        },
        Err(error) => println!("{error}"),
      }
    }
    Err(error) => println!("{error}"),
  }
  Ok(())
}

async fn create_client(configuration: &utils::DotenvConfiguration, client_configuration: Config, configuration_callback: impl Fn(ShardId, ConfigBuilder) -> Config) -> Result<(Arc<Client>, Vec<Shard>), StartRecommendedError> {
  let client = Arc::new(Client::new(configuration.client_token.clone()));

  let shards_result = stream::create_recommended(&client, client_configuration, &configuration_callback).await;

  match shards_result {
    Ok(shards) => Ok((client, shards.collect::<Vec<_>>())),
    Err(error) => Err(error),
  }
}

async fn handle_shards(client: Arc<Client>, shards: Vec<Shard>, (shutdown_tx, shutdown_rx, reshard_rx): (Sender<bool>, Receiver<bool>, Receiver<bool>), set: Arc<Mutex<JoinSet<()>>>) {
  for shard in shards {
    let mut shutdown_rx: Receiver<bool> = shutdown_rx.clone();
    let mut reshard_rx = reshard_rx.clone();
    let shard = Arc::new(Mutex::new(shard));
    let client = client.clone();

    set.lock().await.spawn(async move {
      tokio::select! {
        _ = runner(client, shard.clone()) => {},
        _ = shutdown_rx.changed() => {
          _ = shard.lock().await.close(CloseFrame::NORMAL).await;
        },
        _ = reshard_rx.changed() => {
          _ = shard.lock().await.close(CloseFrame::NORMAL).await;
        },
      }
    });
  }

  let shutdown = signal::ctrl_c().await;
  if let Ok(_) = shutdown {
    let close = shutdown_tx.send(true);
    if let Err(error) = close {
      println!("An error has ocurred while clossing shards: {error}");
    }

    while set.lock().await.join_next().await.is_some() {}
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
async fn reshard(client: Arc<Client>, config: Config, config_callback: impl Fn(ShardId, ConfigBuilder) -> Config) -> anyhow::Result<Option<Vec<Shard>>> {
  const RESHARD_DURATION: Duration = Duration::from_secs(20);

  time::sleep(RESHARD_DURATION).await;

  let mut shards = stream::create_recommended(&client, config, config_callback).await?.collect::<Vec<_>>();
  let mut identified = vec![false; shards.len()];
  let mut stream = ShardMessageStream::new(shards.iter_mut());

  // Drive the new list of shards until they are all identified.
  while !identified.iter().all(|&shard| shard) {
    match stream.next().await {
      Some((_, Err(source))) => {
        if source.is_fatal() {
          return Ok(None);
        }

        continue;
      }
      Some((shard, _)) => {
        identified[shard.id().number() as usize] = shard.status().is_identified();
      }
      None => return Ok(None),
    }
  }

  drop(stream);
  Ok(Some(shards))
}
