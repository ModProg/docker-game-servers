#[derive(Clap)]
pub struct Tmp {
    game: &'static Game,
    #[clap(flatten)]
    options: GameOptions,
}

#[derive(Clap)]
pub struct GameOptions {
    #[clap(long, short)]
    version: Option<String>,
}

pub async fn tmp(docker: &Docker, Tmp { game, options }: Tmp) -> Result<()> {
    pull(
        docker,
        game.image,
        if game.version.config == VersionConfiguration::Tag {
            options.version.as_deref()
        } else {
            None
        },
    )
    .await?;
    let container_id = create(docker, game, options).await?;
    start(docker, &container_id).await?;

    pause();

    // TODO option to attach to console
    stop(docker, &container_id).await?;
    rm(docker, &container_id).await?;

    Ok(())
}