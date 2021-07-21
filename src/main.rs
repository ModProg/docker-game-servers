#![feature(iter_intersperse)]
use bollard::container::ListContainersOptions;
use bollard::image::ListImagesOptions;
use bollard::{ClientVersion, Docker};
use clap::Clap;

use std::collections::HashMap;
use std::default::Default;

use crate::cli::{Opt, ServerFilter};

#[macro_use]
mod macros;

mod cli;

struct Game {
    name: &'static str,
    image: &'static str,
}

const GAMES: &[Game] = &[Game {
    name: "minecraft",
    image: "docker.io/itzg/minecraft-server:latest",
}];
const TIME_OUT: u64 = 5;

#[tokio::main]
async fn main() {
    let opt = Opt::parse();
    // Handle non Docker dependent commands first
    match opt.cmd {
        cli::Command::Games => {
            println!(
                "Availible Games:\n{}",
                GAMES
                    .iter()
                    .map(|v| v.name)
                    .intersperse("\n")
                    .collect::<String>()
            );
            exit!()
        }
        _ => (),
    }

    let docker = match (opt.podman_system, opt.podman_user) {
        (false, true) => Docker::connect_with_socket(
            &{
                let mut rt_dir =
                    dirs::runtime_dir().expect("There should be a runtime dir ($XDG_RUNTIME_DIR)");
                rt_dir.push("podman/podman.sock");
                rt_dir
            }
            .to_str()
            .expect("The runtime dir ($XDG_RUNTIME_DIR) is a valid str"),
            TIME_OUT,
            &ClientVersion {
                major_version: 1,
                minor_version: 40,
            },
        ),
        (true, false) => Docker::connect_with_socket(
            "/var/run/podman/podman.sock",
            TIME_OUT,
            &ClientVersion {
                major_version: 1,
                minor_version: 40,
            },
        ),
        _ => Docker::connect_with_local_defaults(),
    }
    .expect("Setup Docker connection (cannot error currently)");
    // Try connection to fail with a reasonable error:
    if let Err(error) = docker.ping().await {
        exit!(1, "Unable to connect with Docker: \n {}", error);
    };

    match opt.cmd {
        cli::Command::Servers(ServerFilter { name, game, tags }) => {
            let mut filters = HashMap::new();
            // TODO add a default label to only find those created by dgs
            if tags.len() > 0 {
                filters.insert(
                    "label".to_owned(),
                    tags.iter().map(|tag| "dgs-".to_owned() + tag).collect(),
                );
            }
            if let Some(game_name) = game {
                let game = GAMES.iter().find(|game| game.name == &*game_name);
                let game = game.or_else(|| {
                    let games: Vec<_> = GAMES
                        .iter()
                        .filter(|game| game.name.contains(&*game_name))
                        .collect();
                    if games.len() == 1 {
                        Some(games[0])
                    } else {
                        None
                    }
                });
                if let Some(game) = game {
                    filters.insert("ancestor".into(), vec![game.image.into()]);
                }
            };
            let servers = &docker
                .list_containers(Some(ListContainersOptions::<String> {
                    all: true,
                    filters,
                    ..Default::default()
                }))
                .await
                .unwrap();

            for server in servers {
                println!("-> {:?}", server);
            }
        }
        cli::Command::Games => unreachable!("Already handled in pre docker match."),
    }
}
