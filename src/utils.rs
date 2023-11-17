use dotenv::dotenv;
use serde::Deserialize;
use serde_env::from_env;
use std::fmt;

#[derive(Debug)]
pub struct DotenvError {
  pub name: &'static str,
  pub description: &'static str,
}

impl fmt::Display for DotenvError {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "{}: {}", self.name, self.description)
  }
}

#[derive(Deserialize)]
pub struct DotenvConfiguration {
  pub client_token: String,
  pub client_id: String,
}

pub fn load_dotenv_config() -> Result<DotenvConfiguration, DotenvError> {
  dotenv().ok();
  let configuration_result: Result<DotenvConfiguration, _> = from_env();
  match configuration_result {
    Ok(configuration) => Ok(configuration),
    Err(error) => {
      println!("{error}");
      Err(DotenvError {
        name: "Dotenv error",
        description: "An error has ocurred while loading .env file",
      })
    }
  }
}
