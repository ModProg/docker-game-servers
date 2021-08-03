#![feature(iter_intersperse, never_type, map_into_keys_values)]
use anyhow::{anyhow, Error, Result};
use bollard::models::{self, ContainerStateStatusEnum, ContainerSummaryInner, PortTypeEnum};
use bollard::{ClientVersion, Docker};
use clap::Clap;

use core::fmt::{self, Debug};
use std::convert::{TryFrom, TryInto};
use std::process::exit;
use std::str::FromStr;

use crate::cli::Opt;
use crate::server::{ls, tmp};

use self::cli::LowerCaseString;
use self::server::ServerFilter;

#[macro_use]
mod macros;

mod cli;
mod server;

const UTF8_SOLID_INNER_BORDERS: &str = "        │─         ";

#[derive(Debug, Clone)]
pub enum PortConfiguration {
    NonConfigurable(&'static [(u16, PortTypeEnum)]),
    SinglePort(u16, PortTypeEnum),
}

#[derive(Debug, Clone)]
struct Game {
    name: &'static str,
    image: &'static str,
    ports: PortConfiguration,
    envs: &'static [&'static str],
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
    fn find_by_name(game_name: &LowerCaseString) -> Option<&'static Self> {
        GAMES
            .iter()
            .find(|Game { name, .. }| game_name == name.to_lowercase())
    }
}

impl FromStr for &Game {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Game::find_by_name(&s.into()).ok_or(anyhow!("Unable to find a game matching `{}`", s))
    }
}

#[derive(Debug)]
struct Port {
    public: u16,
    private: u16,
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
                public: public_port.try_into()?,
                private: private_port.try_into()?,
                typ,
            }),
            port => Err(anyhow!("Incompatible port config: {:?}", port)),
        }
    }
}

struct BasicServerInfo {
    name: String,
    game: &'static Game,
    tags: Vec<String>,
    ports: Vec<Port>,
    status: ContainerStateStatusEnum,
}

impl fmt::Debug for BasicServerInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            name,
            game: Game { name: game, .. },
            tags,
            ports,
            status,
        } = self;
        write!(
            f,
            "Server {{name: {:?}, game: {:?}, tags: {:?}, ports: {:?}, status: {:?}}}",
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
                .collect::<Vec<_>>(),
            status
        )
    }
}

impl TryFrom<ContainerSummaryInner> for BasicServerInfo {
    type Error = Error;

    fn try_from(container: ContainerSummaryInner) -> Result<Self, Self::Error> {
        match container {
            ContainerSummaryInner {
                image: Some(image),
                names: Some(names),
                labels: Some(labels),
                ports: Some(ports),
                state: Some(state),
                ..
            } if names.len() == 1 => Ok(Self {
                status: ContainerStateStatusEnum::from_str(&state)
                    .map_err(|e| anyhow!("Invalid container state: `{:?}`", e))?,
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
        ports: PortConfiguration::SinglePort(25565, PortTypeEnum::TCP),
        envs: &["EULA=TRUE"],
    },
    Game {
        name: "factorio",
        image: "docker.io/factoriotools/factorio",
        ports: PortConfiguration::SinglePort(34197, PortTypeEnum::UDP),
        envs: &[],
    },
    // TODO investigate how to handle the Ports here
    Game {
        name: "valheim",
        image: "docker.io/lloesche/valheim-server",
        ports: PortConfiguration::NonConfigurable(&[
            (2456, PortTypeEnum::UDP),
            (2457, PortTypeEnum::UDP),
            (2458, PortTypeEnum::UDP),
        ]),
        envs: &[],
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
            return;
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
        eprintln!("Unable to connect with Docker: \n {}", error);
        exit(1);
    };

    if let Err(e) = match opt.cmd {
        cli::Command::Games => unreachable!("Already handled in pre-docker match."),
        cli::Command::Server { cmd: None } => {
            ls(
                ServerFilter {
                    state: Some(ContainerStateStatusEnum::RUNNING),
                    ..Default::default()
                },
                &docker,
            )
            .await
        }
        cli::Command::Server { cmd: Some(cmd) } => match cmd {
            server::ServerCmd::Tmp(config) => tmp(&docker, config).await,
            server::ServerCmd::Ls(filter) => ls(filter, &docker).await,
        },
        cli::Command::Servers(server) => ls(server, &docker).await,
    } {
        eprintln!("It died: {}", e);
        exit(1);
    }
}
