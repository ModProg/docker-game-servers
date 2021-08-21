#![feature(iter_intersperse, never_type, in_band_lifetimes)]
use anyhow::{anyhow, Error, Result};
use bollard::models::{self, ContainerStateStatusEnum, ContainerSummaryInner, PortTypeEnum};
use bollard::{ClientVersion, Docker};
use clap::{ArgEnum, Clap};
use cli::Command;

use core::fmt::{self, Debug};
use std::convert::{TryFrom, TryInto};
use std::fmt::Display;
use std::fs::{create_dir_all, File};
use std::io::Write;
use std::ops::Deref;
use std::process::exit;
use std::str::FromStr;

use crate::cli::Opt;
use crate::server::{ls, tmp};

use self::cli::LowerCaseString;
use self::server::ServerFilter;

mod cli;
mod server;

const UTF8_SOLID_INNER_BORDERS: &str = "        │─         ";

#[derive(Debug, Clone)]
pub enum PortConfiguration {
    NonConfigurable(&'static [(u16, PortTypeEnum)]),
    SinglePort(u16, PortTypeEnum),
}

#[derive(Debug, Clone)]
pub struct Version {
    config: VersionConfiguration,
    ls: VersionLs,
}
#[derive(Debug, Clone, PartialEq)]
pub enum VersionConfiguration {
    Tag,
    Env(&'static str),
    None,
}
#[derive(Debug, Clone)]
pub enum VersionLs {
    Help(&'static str),
    None,
}

#[derive(Debug, Clone)]
struct Game {
    name: GameName,
    image: &'static str,
    ports: PortConfiguration,
    envs: &'static [&'static str],
    version: Version,
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
        Game::find_by_name(&s.into())
            .ok_or_else(|| anyhow!("Unable to find a game matching `{}`", s))
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
                    .filter_map(|label| label.strip_prefix("dgs-").map(|label| label.into()))
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

#[derive(Clone, Copy, Debug, ArgEnum, PartialEq)]
pub enum GameName {
    Minecraft,
    Factorio,
    Valheim,
}

impl Display for GameName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", *self)
    }
}

impl Deref for GameName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        use GameName::*;
        match self {
            Minecraft => "minecraft",
            Factorio => "factorio",
            Valheim => "valheim",
        }
    }
}

const GAMES: &[Game] = &[
    Game {
        name: GameName::Minecraft,
        image: "docker.io/itzg/minecraft-server",
        ports: PortConfiguration::SinglePort(25565, PortTypeEnum::TCP),
        envs: &["EULA=TRUE"],
        version: Version {
            config: VersionConfiguration::Env("VERSION"),
            ls: VersionLs::Help("You can either specify `LATEST` (the default) to run the latest stable version, `SNAPSHOT` to run the latest snapshot, or you can specify the version directly e.g. `1.7.2` or `21w11a` ")
        }
    },
    Game {
        name: GameName::Factorio,
        image: "docker.io/factoriotools/factorio",
        ports: PortConfiguration::SinglePort(34197, PortTypeEnum::UDP),
        envs: &[],
        version: Version {
            config: VersionConfiguration::Tag,
            ls: VersionLs::Help("You can either specify `latest` (the default) to run the latest (maybe experimental) version, `stable` to run the latest stable version, or you can specify the version directly e.g. `1.1` or `0.15.40`. You can also look for availible versions at https://hub.docker.com/r/factoriotools/factorio/tags.")
        }
    },
    // TODO investigate how to handle the Ports here
    Game {
        name: GameName::Valheim,
        image: "docker.io/lloesche/valheim-server",
        ports: PortConfiguration::NonConfigurable(&[
            (2456, PortTypeEnum::UDP),
            (2457, PortTypeEnum::UDP),
            (2458, PortTypeEnum::UDP),
        ]),
        envs: &[],
        version: Version {
            config: VersionConfiguration::None,
            ls: VersionLs::None
        }
    },
];
const TIME_OUT: u64 = 5;

#[tokio::main]
async fn main() -> Result<()> {
    let opt = Opt::parse();
    // Handle non Docker dependent commands first
    match opt.cmd {
        Command::Games => {
            println!(
                "Availible Games:\n{}",
                GAMES
                    .iter()
                    .map(|v| &*v.name)
                    .intersperse("\n")
                    .collect::<String>()
            );
            return Ok(());
        }
        Command::Completions {
            shell,
            print,
            ref filename,
            system,
        } => {
            let mut app = {
                use clap::IntoApp;
                Opt::into_app()
            };
            let name = app
                .get_bin_name()
                .expect("bin_name is set on struct")
                .to_string();
            let mut buffer: Box<dyn Write> = if print {
                Box::new(std::io::stdout())
            } else {
                let filename = if let Some(filename) = filename {
                    if filename.is_dir() {
                        filename.join(shell.file_name(&name))
                    } else {
                        filename.with_file_name(
                            shell.file_name(
                                &filename
                                    .file_name()
                                    .expect("The passed Filename should be a valid string")
                                    .to_string_lossy(),
                            ),
                        )
                    }
                } else {
                    let path = if system {
                        shell.system_path()
                    } else {
                        shell.user_path()
                    };
                    create_dir_all(&path)?;
                    path.join(shell.file_name(&name))
                };
                Box::new(File::create(filename).unwrap())
            };
            shell.generate_completions(&mut app, &name, &mut buffer);
            return Ok(());
        }
        _ => {}
    }

    let docker = match (opt.podman_system, opt.podman_user) {
        (false, true) => Docker::connect_with_socket(
            {
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
        Command::Games | Command::Completions { .. } => {
            unreachable!("Already handled in pre-docker match.")
        }
        Command::Server { cmd: None } => {
            ls(
                ServerFilter {
                    state: Some(ContainerStateStatusEnum::RUNNING),
                    ..Default::default()
                },
                &docker,
            )
            .await
        }
        Command::Server { cmd: Some(cmd) } => match cmd {
            server::ServerCmd::Tmp(config) => tmp(&docker, config).await,
            server::ServerCmd::Ls(filter) => ls(filter, &docker).await,
        },
        Command::Servers(server) => ls(server, &docker).await,
    } {
        eprintln!("It died: {}", e);
        exit(1);
    };
    Ok(())
}
