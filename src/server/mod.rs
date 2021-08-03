pub mod ls;

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use bollard::models::PortBinding;
use bollard::Docker;
use chrono::prelude::*;
use clap::Clap;

pub use ls::{ls, ServerFilter};
use portpicker::pick_unused_port;

use crate::Game;
type ContainerId = String;

#[derive(Clap)]
pub enum ServerCmd {
    /// Run a temporary server
    ///
    /// This wont have persistant storage and stop when exited (e.g. with <^C>)
    Tmp(Tmp),
    Ls(ServerFilter),
}

#[derive(Clap)]
pub struct Tmp {
    game: &'static Game,
}

pub async fn tmp(docker: &Docker, Tmp { game }: Tmp) -> Result<()> {
    let container_id = create(docker, game).await?;
    start(docker, &container_id).await?;

    pause();

    // TODO option to attach to console
    stop(docker, &container_id).await?;
    rm(docker, &container_id).await?;

    Ok(())
}

async fn create(docker: &Docker, game: &'static Game) -> Result<ContainerId> {
    use bollard::container::{Config, CreateContainerOptions};
    use bollard::models::HostConfig;
    let options = Some(CreateContainerOptions {
        name: format!(
            "dgs-tmp_{}_{}",
            game.name,
            Local::now().format("%Y-%m-%d_%H-%M-%S%.3f")
        ),
    });
    let mut pb: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    match game.ports {
        crate::PortConfiguration::NonConfigurable(_) => todo!(),
        crate::PortConfiguration::SinglePort(port, protocol) => {
            let host_port = pick_unused_port().ok_or(anyhow!("Did not find any open port LUL."))?;
            println!("Running on Port: `{}`", host_port);
            pb.insert(
                format!("{}/{}", port, protocol),
                Some(vec![PortBinding {
                    host_ip: None,
                    host_port: Some(host_port.to_string()),
                }]),
            );
        }
    }
    let config = Config {
        image: Some(game.image),
        env: Some(game.envs.into()),
        host_config: Some(HostConfig {
            port_bindings: Some(pb),
            ..Default::default()
        }),

        ..Default::default()
    };

    Ok(docker.create_container(options, config).await?.id)
}

async fn start(docker: &Docker, container_id: &ContainerId) -> Result<()> {
    use bollard::container::StartContainerOptions;
    let options = Some(StartContainerOptions { detach_keys: "" });

    Ok(docker.start_container(container_id, options).await?)
}

async fn stop(docker: &Docker, container_id: &ContainerId) -> Result<()> {
    Ok(docker.stop_container(container_id, None).await?)
}
async fn rm(docker: &Docker, container_id: &ContainerId) -> Result<()> {
    Ok(docker.remove_container(container_id, None).await?)
}
fn pause() {
    use std::io::{stdin, stdout, Write};
    use termion::input::TermRead;
    use termion::raw::IntoRawMode;

    println!("Press any key to quit the server...");
    let mut stdout = stdout().into_raw_mode().unwrap();
    stdout.flush().unwrap();
    stdin().events().next();
}
