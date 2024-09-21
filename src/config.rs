// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Configuration for the greeter

use std::path::Path;
use std::{collections::HashMap, path::PathBuf};

use relm4::gtk::ContentFit;
use relm4::spawn_blocking;
use serde::{Deserialize, Serialize};
use tokio::fs::read_to_string;

use crate::{
    constants::{GREETING_MSG, POWEROFF_CMD, REBOOT_CMD, X11_CMD_PREFIX},
    error::TomlReadError,
};

/// The configuration struct
#[derive(Default, Deserialize, Serialize)]
pub struct Config {
    #[serde(default)]
    pub appearance: AppearanceConfig,

    #[serde(default)]
    pub background: BackgroundConfig,

    #[serde(default)]
    pub commands: SystemCommandsConfig,

    #[serde(default)]
    pub env: HashMap<String, String>,
}

impl Config {
    pub async fn load<P>(path: P) -> Result<Self, TomlReadError>
    where
        P: AsRef<Path>,
    {
        let string = read_to_string(path).await?;
        let value: Self = spawn_blocking(move || toml::from_str(&string))
            .await
            .unwrap()?;

        Ok(value)
    }
}

#[derive(Deserialize, Serialize)]
pub struct AppearanceConfig {
    #[serde(default = "default_greeting_msg")]
    pub greeting_msg: String,
}

impl Default for AppearanceConfig {
    fn default() -> Self {
        AppearanceConfig {
            greeting_msg: default_greeting_msg(),
        }
    }
}

/// Struct for info about the background image
#[derive(Default, Deserialize, Serialize)]
pub struct BackgroundConfig {
    #[serde(default)]
    pub path: Option<PathBuf>,

    #[serde(default)]
    pub fit: BgFit,
}

/// Analogue to `gtk4::ContentFit`
#[derive(Default, Deserialize, Serialize)]
pub enum BgFit {
    Fill,
    #[default]
    Contain,
    Cover,
    ScaleDown,
}

impl From<BgFit> for ContentFit {
    fn from(value: BgFit) -> Self {
        match value {
            BgFit::Fill => Self::Fill,
            BgFit::Contain => Self::Contain,
            BgFit::Cover => Self::Cover,
            BgFit::ScaleDown => Self::ScaleDown,
        }
    }
}

/// Struct for reboot/poweroff commands
#[derive(Deserialize, Serialize)]
pub struct SystemCommandsConfig {
    #[serde(default = "default_reboot_command")]
    pub reboot: Vec<String>,

    #[serde(default = "default_poweroff_command")]
    pub poweroff: Vec<String>,

    #[serde(default = "default_x11_command_prefix")]
    pub x11_prefix: Vec<String>,
}

impl Default for SystemCommandsConfig {
    fn default() -> Self {
        SystemCommandsConfig {
            reboot: default_reboot_command(),
            poweroff: default_poweroff_command(),
            x11_prefix: default_x11_command_prefix(),
        }
    }
}

fn default_greeting_msg() -> String {
    GREETING_MSG.to_string()
}

fn default_reboot_command() -> Vec<String> {
    shlex::split(REBOOT_CMD).expect("Unable to lex reboot command")
}

fn default_poweroff_command() -> Vec<String> {
    shlex::split(POWEROFF_CMD).expect("Unable to lex poweroff command")
}

fn default_x11_command_prefix() -> Vec<String> {
    shlex::split(X11_CMD_PREFIX).expect("Unable to lex X11 command prefix")
}
