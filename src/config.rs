use serde::Deserialize;
use std::env;
use std::path::PathBuf;

#[derive(Deserialize, Debug)]
pub(crate) struct Config {
    pub(crate) serve_path: PathBuf,
    pub(crate) upload_limit: u64,
}

fn print_cli_help() {
    println!("Usage: restore -config path_to_config.toml");
}

pub(crate) async fn get_config() -> Option<Config> {
    let args: Vec<String> = env::args().collect();
    match (args.len(), args[1].as_str()) {
        (3, "-config") => tokio::fs::read_to_string(&args[2]).await.map_or_else(
            |_| {
                println!("Error reading file");
                None
            },
            |config_str| {
                toml::from_str(&config_str).map_or_else(
                    |_| {
                        println!("Error parsing file");
                        None
                    },
                    |config| Some(config),
                )
            },
        ),
        _ => {
            print_cli_help();
            None
        }
    }
}
