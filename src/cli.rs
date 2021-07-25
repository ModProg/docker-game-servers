use std::ops::Deref;
use std::str::FromStr;

use clap::Clap;

#[derive(Clap)]
#[clap(version = "0.1", author = "ModProg <dev@modprog.de>")]
pub struct Opt {
    #[clap(short, long)]
    pub podman_user: bool,
    #[clap(short = 'P', long, conflicts_with = "podman-user")]
    pub podman_system: bool,
    #[clap(subcommand)]
    pub cmd: Command,
}

#[derive(Clap)]
pub enum Command {
    /// Lists availible images
    Games,
    /// List running servers
    Servers(ServerFilter),
}

#[derive(Clap)]
pub struct ServerFilter {
    /// Only servers matching the name will be returned.
    #[clap(short, long)]
    pub name: Option<String>,
    /// Only servers with a matching game name will be returned.
    #[clap(short, long)]
    pub game: Option<LowerCaseString>,
    /// Only servers with these tags (case is ignored) will be returned.
    ///
    /// Usage: `-t first_tag -t second_tag`
    #[clap(short, long = "tag")]
    pub tags: Vec<LowerCaseString>,
}

pub struct LowerCaseString(String);

impl FromStr for LowerCaseString {
    type Err = !;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_lowercase()))
    }
}

impl From<&str> for LowerCaseString {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl From<String> for LowerCaseString {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<LowerCaseString> for String {
    fn from(lcs: LowerCaseString) -> Self {
        lcs.0
    }
}

impl Deref for LowerCaseString {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
