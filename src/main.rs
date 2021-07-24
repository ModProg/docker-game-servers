#![feature(iter_intersperse, never_type)]
use anyhow::{anyhow, Error, Result};
use bollard::container::ListContainersOptions;
use bollard::models::ContainerSummaryInner;
use bollard::{ClientVersion, Docker};
use clap::Clap;

use std::collections::HashMap;
use std::convert::TryFrom;
use std::default::Default;

use crate::cli::{Opt, ServerFilter};

#[macro_use]
mod macros;

mod cli;

const DOCKER_ERROR: i32 = 1;
const USER_ERROR: i32 = 2;

#[derive(Debug, Clone)]
struct Game {
    name: &'static str,
    image: &'static str,
}

impl Game {
    fn find_by_image(image_name: &str) -> Option<&'static Self> {
        let image_name = if let Some((image_name, _)) = image_name.split_once(':') {
            image_name
        } else {
            image_name
        };
        GAMES.iter().find(|Game { image, .. }| *image == image_name)
    }
}

#[derive(Debug)]
struct Server {
    game: &'static Game,
    tags: Vec<String>,
}

impl TryFrom<ContainerSummaryInner> for Server {
    type Error = Error;

    fn try_from(container: ContainerSummaryInner) -> Result<Self, Self::Error> {
        match container {
            ContainerSummaryInner {
                image: Some(image),
                names: Some(names),
                ..
            } if names.len() == 1 => Ok(Self {
                game: if let Some(game) = Game::find_by_image(&image) {
                    game
                } else {
                    return Err(anyhow!(
                        "Container image is not compatible with dgs: `{}`",
                        image
                    ));
                },
                tags: vec![],
            }),
            _ => Err(anyhow!("Container is not compatible with dgs")),
        }
    }
}

const GAMES: &[Game] = &[
    Game {
        name: "minecraft",
        image: "docker.io/itzg/minecraft-server",
    },
    Game {
        name: "factorio",
        image: "docker.io/factoriotools/factorio",
    },
    Game {
        name: "valheim",
        image: "docker.io/lloesche/valheim-server",
    },
];
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
        exit!(DOCKER_ERROR, "Unable to connect with Docker: \n {}", error);
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
                    match games.len() {
                        1 => Some(games[0]),
                        0 => exit!(
                            USER_ERROR,
                            "Unable to find a matching game for: `{}`",
                            &*game_name
                        ),
                        _ => exit!(
                            USER_ERROR,
                            "Unable to find unique matching game for: `{}`, found: {}",
                            &*game_name,
                            games
                                .iter()
                                .map(|game| "`".to_owned() + game.name + "`")
                                .intersperse(", ".to_owned())
                                .collect::<String>()
                        ),
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
                if let Ok(server) = Server::try_from(server.clone()) {
                    println!("-> {:?}", server);
                }
            }
        }
        cli::Command::Games => unreachable!("Already handled in pre-docker match."),
    }
}
