use std::io;
use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use anyhow::Error;
use clap::{App, Clap};

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
        /// System wide installation
        #[clap(short, long, conflicts_with = "filename", conflicts_with = "print")]
        system: bool,
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
            ShellType::Fish if !name.ends_with(".fish") => generators::Fish::file_name(name),
            ShellType::PowerShell if !name.starts_with('_') || !name.ends_with("ps1") => {
                generators::PowerShell::file_name(name)
            }
            ShellType::Zsh if !name.starts_with('_') => generators::Zsh::file_name(name),
            _ => name.into(),
        }
    }

    pub fn user_path(&self) -> PathBuf {
        use dirs::*;
        use std::env::*;
        use ShellType::*;
        let local_share = data_local_dir().expect("There should be a data_home");
        match self {
            Bash => {
                if let Some(comp_dir) = var_os("BASH_COMPLETION_USER_DIR") {
                    PathBuf::from(comp_dir)
                } else {
                    local_share.join("bash-completion/completions/")
                }
            }
            Elvish => unimplemented!(),
            Fish => local_share.join("fish/generated_completions/"),
            PowerShell => unimplemented!("Not sure where to place the generated file"),
            Zsh => unimplemented!("There is no default path for local completion files"),
        }
    }

    pub fn system_path(&self) -> PathBuf {
        use ShellType::*;
        match self {
            Bash => PathBuf::from("/usr/share/bash-completion/completions/"),
            Elvish => unimplemented!(),
            Fish => PathBuf::from("/usr/share/fish/completions/"),
            PowerShell => unimplemented!("Not sure where to place the generated file"),
            Zsh => PathBuf::from("/usr/local/share/zsh/site-functions/"),
        }
    }

    pub fn generate_completions(&self, app: &mut App, name: &str, buffer: &mut dyn io::Write) {
        use clap_generate::{generate, generators};
        match self {
            ShellType::Bash => clap_generate::generate::<generators::Bash, _>(app, name, buffer),
            ShellType::Elvish => generate::<generators::Elvish, _>(app, name, buffer),
            ShellType::Fish => {
                generate::<generators::Fish, _>(app, name, buffer);
                // Sub completions for the `completions` and `help` command
                // because clap cannot do this currently
                let shells = "shells bash fish zsh powershell";
                writeln!(buffer,
                         r#"complete -c dgs -n "__fish_seen_subcommand_from completions; and not __fish_seen_subcommand_from {}" -f -a "{}" -r"#,
                         shells, shells)
                    .expect("We should be able to add a line at the end of the completions file.");
                let commands = "completions games server servers";
                writeln!(buffer,
                         r#"complete -c dgs -n "__fish_seen_subcommand_from help; and not __fish_seen_subcommand_from {}" -f -a "{}" -r"#,
                         commands, commands)
                    .expect("We should be able to add a line at the end of the completions file.");
            }
            ShellType::PowerShell => generate::<generators::PowerShell, _>(app, name, buffer),
            ShellType::Zsh => generate::<generators::Zsh, _>(app, name, buffer),
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
