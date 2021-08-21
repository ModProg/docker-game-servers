use std::collections::HashMap;
use std::convert::TryFrom;
use std::iter;

use anyhow::{bail, Result};
use bollard::container::ListContainersOptions;
use bollard::models::{ContainerStateStatusEnum, PortTypeEnum};
use bollard::Docker;
use clap::Clap;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};

use crate::cli::LowerCaseString;
use crate::{BasicServerInfo, GAMES, Game, GameName, Port, UTF8_SOLID_INNER_BORDERS};
#[derive(Clap, Default)]
pub struct ServerFilter {
    /// Only servers matching the name will be returned.
    #[clap(short, long)]
    pub name: Option<String>,
    /// Only servers with a matching game name will be returned.
    #[clap(short, long, arg_enum)]
    pub game: Option<GameName>,
    /// Only servers with these tags (case is ignored) will be returned.
    ///
    /// Usage: `-t first_tag -t second_tag`.
    /// This would return all servers that have both `first_tag` and `second_tag`.
    #[clap(short, long = "tag")]
    pub tags: Vec<LowerCaseString>,
    /// Only servers with this state are returned
    #[clap(short, long)]
    pub state: Option<ContainerStateStatusEnum>,
}
pub async fn ls(
    ServerFilter {
        name,
        game,
        tags,
        state: status,
    }: ServerFilter,
    docker: &Docker,
) -> Result<()> {
    let mut filters = HashMap::new();
    filters.insert(
        "label".to_owned(),
        if tags.is_empty() {
            tags.iter()
                .map(|tag| "dgs-".to_owned() + tag)
                // The default Tag every server has
                .chain(iter::once("dgs".into()))
                .collect()
        } else {
            vec!["dgs".into()]
        },
    );
    if let Some(game_name) = game {
        let game = GAMES.iter().find(|game| game.name == game_name);
        let game = game.ok_or_else(|| {
            let games: Vec<_> = GAMES
                .iter()
                .filter(|game| game.name.contains(&*game_name))
                .collect();
            match games.len() {
                1 => Ok(games[0]),
                0 => bail!("Unable to find a matching game for: `{}`", &*game_name),

                _ => bail!(
                    "Unable to find unique matching game for: `{}`, found: {}",
                    &*game_name,
                    games
                        .iter()
                        .map(|game| "`".to_owned() + &game.name + "`")
                        .intersperse(", ".to_owned())
                        .collect::<String>()
                ),
            }
        });
        if let Ok(game) = game {
            filters.insert("ancestor".into(), vec![game.image.into()]);
        }
    };
    if let Some(status) = status {
        filters.insert("status".into(), vec![status.to_string().to_lowercase()]);
    }
    let search_name = name.map(|s| s.to_lowercase()).unwrap_or_default();
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
            vec!["Name", "Game", "Tags", "Ports", "Status"]
                .iter()
                .map(|s| Cell::new(s).set_alignment(CellAlignment::Center)),
        );

    if !table.is_tty() {
        table.set_table_width(60);
    }

    for server in servers {
        if let Ok(BasicServerInfo {
            name,
            game: Game {
                name: game_name, ..
            },
            tags,
            ports,
            status,
        }) = BasicServerInfo::try_from(server.clone())
        {
            if name.to_lowercase().contains(&search_name) {
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
                                Port { typ, public, .. } => {
                                    format!(" - {}({})\n", public, typ)
                                }
                            })
                            .collect::<String>(),
                    ),
                    Cell::new(format!("{:?}", status)),
                ]);
            }
        }
    }
    println!("{}", table);
    Ok(())
}
