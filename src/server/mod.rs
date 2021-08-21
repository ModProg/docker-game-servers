pub mod ls;
mod tmp;

use std::collections::HashMap;

use anyhow::bail;
use anyhow::{anyhow, Result};
use bollard::models::PortBinding;
use bollard::Docker;
use chrono::prelude::*;
use clap::Clap;

use futures_util::TryStreamExt;
pub use ls::{ls, ServerFilter};
pub use tmp::{tmp, Tmp, GameOptions};
use portpicker::pick_unused_port;

use crate::{Game, VersionConfiguration};

#[derive(Clap)]
pub enum ServerCmd {
    /// Run a temporary server
    ///
    /// This wont have persistant storage and stop when exited (e.g. with <^C>)
    Tmp(Tmp),
    Ls(ServerFilter),
}

async fn create(docker: &Docker, game: &'static Game, options: GameOptions) -> Result<String> {
    use bollard::container::{Config, CreateContainerOptions};
    use bollard::models::HostConfig;
    let mut pb: HashMap<String, Option<Vec<PortBinding>>> = HashMap::new();
    match game.ports {
        crate::PortConfiguration::NonConfigurable(_) => todo!(),
        crate::PortConfiguration::SinglePort(port, protocol) => {
            let host_port =
                pick_unused_port().ok_or_else(|| anyhow!("Did not find any open port LUL."))?;
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
    let mut envs: Vec<_> = game.envs.into();
    let v = if let (VersionConfiguration::Env(name), Some(version)) =
        (game.version.config.clone(), options.version)
    {
        Some(format!("{}={}", name, version))
    } else {
        None
    };
    let v = v.as_deref();

    if v.is_some() {
        envs.push(v.unwrap());
    }
    let config = Config {
        image: Some(game.image),
        env: Some(envs),
        host_config: Some(HostConfig {
            port_bindings: Some(pb),
            ..Default::default()
        }),
        labels: {
            let mut labels = HashMap::new();
            labels.insert("dgs", "dgs");
            Some(labels)
        },

        ..Default::default()
    };

    Ok(docker
        .create_container(
            Some(CreateContainerOptions {
                name: format!(
                    "dgs-tmp_{}_{}",
                    game.name,
                    Local::now().format("%Y-%m-%d_%H-%M-%S%.3f")
                ),
            }),
            config,
        )
        .await?
        .id)
}

async fn start(docker: &Docker, container_id: &str) -> Result<()> {
    use bollard::container::StartContainerOptions;
    let options = Some(StartContainerOptions { detach_keys: "" });

    Ok(docker.start_container(container_id, options).await?)
}

async fn stop(docker: &Docker, container_id: &str) -> Result<()> {
    Ok(docker.stop_container(container_id, None).await?)
}
async fn rm(docker: &Docker, container_id: &str) -> Result<()> {
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

async fn pull(docker: &Docker, image_name: &str, tag: Option<&str>) -> Result<()> {
    use bollard::image::CreateImageOptions;
    use bollard::models::ProgressDetail;
    println!("Pulling {}", image_name);
    let mut options = CreateImageOptions {
        from_image: image_name,
        repo: "docker.io",
        ..Default::default()
    };
    if let Some(tag) = tag {
        options.tag = tag
    }
    docker
        .create_image(Some(options), None, None)
        .err_into::<anyhow::Error>()
        .try_for_each(|progress| async move {
            if let Some(error) = progress.error {
                bail!(error);
            }
            let status = progress
                .status
                .unwrap_or_else(|| "Downloading image".into());
            if let Some(ProgressDetail {
                current: Some(current),
                total: Some(total),
            }) = progress.progress_detail
            {
                println!("{}: ({}/{})", status, current, total);
            } else {
                println!("{}", status);
            }
            Ok(())
        })
        .await?;
    Ok(())
}
