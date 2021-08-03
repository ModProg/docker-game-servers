use std::ops::Deref;
use std::str::FromStr;

use clap::Clap;

use crate::server::{ServerCmd, ServerFilter};

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
    /// List servers
    Servers(ServerFilter),
    /// Manage servers
    Server {
        #[clap(subcommand)]
        cmd: Option<ServerCmd>,
    },
}

#[derive(Clone)]
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

impl PartialEq<String> for LowerCaseString {
    fn eq(&self, other: &String) -> bool {
        *other == self.0
    }
}
impl PartialEq<String> for &LowerCaseString {
    fn eq(&self, other: &String) -> bool {
        *other == self.0
    }
}
