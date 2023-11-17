use std::sync::Arc;
use taomi::utils;
use tokio::{signal, sync::watch, sync::Mutex, task::JoinSet};
use twilight_gateway::{stream, CloseFrame, Config, ConfigBuilder, Intents, Shard};
use twilight_http::Client;

#[tokio::main]
async fn main() -> std::io::Result<()> {
  let configuration_result = utils::load_dotenv_config();

  match configuration_result {
    Ok(configuration) => {
      create_client(configuration).await;
    }
    Err(error) => println!("{error}"),
  }

  Ok(())
}

async fn create_client(configuration: utils::DotenvConfiguration) {
  let client = Arc::new(Client::new(configuration.client_token.clone()));
  let client_configuration = Config::new(configuration.client_token.clone(), Intents::GUILDS | Intents::GUILD_MESSAGES);
  let config_callback = |_, builder: ConfigBuilder| builder.build();

  let shards_result = stream::create_recommended(&client, client_configuration, &config_callback).await;

  match shards_result {
    Ok(shards) => {
      manage_shards(client, shards).await;
    }
    Err(error) => println!("{error}"),
  }
}

async fn manage_shards<T>(client: Arc<Client>, shards: T)
where
  T: Iterator<Item = Shard>,
{
  let (tx, rx) = watch::channel(false);
  let mut shard_set = JoinSet::new();

  for shard in shards {
    let mut rx = rx.clone();
    let atomic_client = client.clone();
    let atomic_shard = Arc::new(Mutex::new(shard));

    shard_set.spawn(async move {
      tokio::select! {
        _ = runner(atomic_client, atomic_shard.clone()) => {},
        _ = rx.changed() => {
            _ = atomic_shard.lock().await.close(CloseFrame::NORMAL).await;
        }
      }
    });
  }

  let shutdown = signal::ctrl_c().await;
  if let Ok(_) = shutdown {
    let close = tx.send(true);
    if let Err(error) = close {
      println!("An error has ocurred while clossing shards: {error}");
    }
    while shard_set.join_next().await.is_some() {}
  }
}

async fn runner(client: Arc<Client>, shard: Arc<Mutex<Shard>>) {
  loop {
    let mut shard = shard.lock().await;
    let event = match shard.next_event().await {
      Ok(event) => event,
      Err(source) => {
        if source.is_fatal() {
          break;
        }

        continue;
      }
    };
  }
}
