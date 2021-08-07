use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Error;
use clap::Clap;

use crate::server::{ServerCmd, ServerFilter};

#[derive(Clap)]
#[clap(version = "0.1", author = "ModProg <dev@modprog.de>", bin_name = "dgs")]
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
    /// Output shell completions
    Completions {
        /// The shell to generate completions for
        ///
        /// e.g. fish/zsh/powershell
        shell: ShellType,
        /// Prints completions to console
        #[clap(short, long, conflicts_with = "filename")]
        print: bool,
        /// Name or Directory to store completions file
        filename: Option<PathBuf>,
    },
    /// List servers
    Servers(ServerFilter),
    /// Manage servers
    Server {
        #[clap(subcommand)]
        cmd: Option<ServerCmd>,
    },
}

#[derive(Clone, Copy)]
pub enum ShellType {
    Bash,
    Elvish,
    Fish,
    PowerShell,
    Zsh,
}

impl ShellType {
    pub fn file_name(&self, name: &str) -> String {
        use clap_generate::{generators, Generator};
        match self {
            ShellType::Bash if !name.ends_with(".bash") => generators::Bash::file_name(name),
            ShellType::Elvish if !name.ends_with(".elv") => generators::Elvish::file_name(name),
            ShellType::Zsh if !name.starts_with('_') => generators::Zsh::file_name(name),
            ShellType::PowerShell if !name.starts_with('_') || !name.ends_with("ps1") => {
                generators::PowerShell::file_name(name)
            }
            ShellType::Fish if !name.ends_with(".fish") => generators::Fish::file_name(name),
            _ => name.into(),
        }
    }
}

impl FromStr for ShellType {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        use ShellType::*;
        Ok(match s.to_ascii_lowercase().as_str() {
            "bash" => Bash,
            "elvish" => Elvish,
            "fish" => Fish,
            "powershell" => PowerShell,
            "zsh" => Zsh,
            &_ => anyhow::bail!("`{}` is not a supported shell", s),
        })
    }
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
