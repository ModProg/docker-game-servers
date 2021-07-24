#![feature(iter_intersperse, never_type, map_into_keys_values)]
use anyhow::{anyhow, Error, Result};
use bollard::container::ListContainersOptions;
use bollard::models::{self, ContainerSummaryInner, PortTypeEnum};
use bollard::{ClientVersion, Docker};
use clap::Clap;
use comfy_table::presets::UTF8_FULL;
use comfy_table::*;

use core::fmt::{self, Debug};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::default::Default;

use crate::cli::{Opt, ServerFilter};

#[macro_use]
mod macros;

mod cli;

const UTF8_SOLID_INNER_BORDERS: &str = "        │─         ";
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
struct Port {
    public: i64,
    private: i64,
    typ: PortTypeEnum,
}

impl TryFrom<models::Port> for Port {
    type Error = Error;

    fn try_from(value: models::Port) -> Result<Self, Self::Error> {
        match value {
            models::Port {
                private_port,
                public_port: Some(public_port),
                typ: Some(typ),
                ..
            } => Ok(Self {
                public: public_port,
                private: private_port,
                typ,
            }),
            port => Err(anyhow!("Incompatible port config: {:?}", port)),
        }
    }
}

struct Server {
    name: String,
    game: &'static Game,
    tags: Vec<String>,
    ports: Vec<Port>,
}

impl fmt::Debug for Server {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            name,
            game: Game { name: game, .. },
            tags,
            ports,
        } = self;
        write!(
            f,
            "Server {{name: {:?}, game: {:?}, tags: {:?}, ports: {:?}}}",
            name,
            game,
            tags,
            ports
                .iter()
                .map(
                    |Port {
                         public,
                         typ,
                         private,
                         ..
                     }| format!("{}:{}->{}", typ, public, private)
                )
                .collect::<Vec<_>>()
        )
    }
}

impl TryFrom<ContainerSummaryInner> for Server {
    type Error = Error;

    fn try_from(container: ContainerSummaryInner) -> Result<Self, Self::Error> {
        match container {
            ContainerSummaryInner {
                image: Some(image),
                names: Some(names),
                labels: Some(labels),
                ports: Some(ports),
                ..
            } if names.len() == 1 => Ok(Self {
                name: names[0].clone(),
                game: if let Some(game) = Game::find_by_image(&image) {
                    game
                } else {
                    return Err(anyhow!(
                        "Container image is not compatible with dgs: `{}`",
                        image
                    ));
                },
                tags: labels
                    .into_keys()
                    .filter_map(|label| {
                        if label.starts_with("dgs-") {
                            Some((&label[4..]).into())
                        } else {
                            None
                        }
                    })
                    .collect(),
                ports: ports
                    .into_iter()
                    .filter_map(|port| Port::try_from(port).ok())
                    .collect(),
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
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_SOLID_INNER_BORDERS)
                .set_content_arrangement(ContentArrangement::Dynamic)
                .set_header(
                    vec!["Name", "Game", "Tags", "Ports"]
                        .iter()
                        .map(|s| Cell::new(s).set_alignment(CellAlignment::Center)),
                );

            if !table.is_tty() {
                table.set_table_width(60);
            }

            for server in servers {
                if let Ok(Server {
                    name,
                    game: Game {
                        name: game_name, ..
                    },
                    tags,
                    ports,
                }) = Server::try_from(server.clone())
                {
                    table.add_row(vec![
                        Cell::new(name),
                        Cell::new(game_name),
                        Cell::new(
                            tags.iter()
                                .map(|tag| format!(" - {}\n", tag))
                                .collect::<String>(),
                        ),
                        Cell::new(
                            ports
                                .iter()
                                .map(|port| match port {
                                    Port {
                                        typ: PortTypeEnum::TCP,
                                        public,
                                        ..
                                    } => format!(" - {}\n", public),
                                    Port { typ, public, .. } => format!(" - {}({})\n", public, typ),
                                })
                                .collect::<String>(),
                        ),
                    ]);
                }
            }
            println!("{}", table);
        }
        cli::Command::Games => unreachable!("Already handled in pre-docker match."),
    }
}
