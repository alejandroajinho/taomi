use std::sync::Arc;
use tokio::sync::Mutex;
use twilight_gateway::{Event, Shard};
use twilight_http::Client;

pub async fn handler(client: Arc<Client>, shard: Arc<Mutex<Shard>>, event: Event) {
  match event {
    Event::Ready(_) => {}
    Event::GuildCreate(guild) => {
      let a = guild.id;
      println!("{a}");
    }
    _ => {}
  }
}
