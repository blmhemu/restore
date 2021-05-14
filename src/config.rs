use serde::Deserialize;
use std::env::Args;
use std::path::PathBuf;

#[derive(Deserialize, Debug)]
pub(crate) struct Config {
    pub(crate) serve_path: PathBuf,
    pub(crate) upload_limit: u64,
}

impl Config {
    pub(crate) async fn new(mut args: Args) -> Option<Self> {
        args.next();
        match args.next() {
            Some(config_file) => tokio::fs::read_to_string(config_file).await.map_or_else(
                |_| {
                    eprintln!("Check if the config file exists and is readable.");
                    None
                },
                |config_str| {
                    toml::from_str(&config_str).map_or_else(
                        |_| {
                            eprintln!("Check if the config file is of the recommended format.");
                            None
                        },
                        |config| Some(config),
                    )
                },
            ),
            None => {
                eprintln!("Usage: restore path_to_config.toml");
                None
            }
        }
    }
}
