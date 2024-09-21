// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Utility for caching info between logins

use std::path::Path;

use relm4::spawn_blocking;
use serde::{Deserialize, Serialize};
use tokio::fs::{create_dir_all, read_to_string, write};
use tracing::info;

use crate::error::{TomlReadError, TomlWriteError};

/// Holds info needed to persist between logins
#[derive(Deserialize, Serialize, Default)]
pub struct Cache {
    /// An ordered map from username to the last session. First is most recent.
    #[serde(with = "tuple_vec_map")]
    pub user_to_last_sess: Vec<(String, SessionIdOrCmdline)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionIdOrCmdline {
    #[serde(rename = "xdg")]
    XdgDektopFile(String),

    #[serde(rename = "cmd")]
    Command(String),
}

impl Cache {
    /// Load the cache from disk with this size limit. If the size should be 0, use [`Default`].
    pub async fn load<P>(path: P) -> Result<Self, TomlReadError>
    where
        P: AsRef<Path>,
    {
        let string = read_to_string(path).await?;
        let value: Self = tokio::task::spawn_blocking(move || toml::from_str(&string))
            .await
            .unwrap()?;

        Ok(value)
    }

    /// Save the cache file to disk.
    ///
    /// This function consumes self because of optimization reasons.
    ///
    /// 1. It will only be run during the shutdown process.
    /// 2. Serde calls can take a long time before this async fn yields, so self would have to be moved into
    ///    [`spawn_blocking`], and whether or not a clone of &self is cost effective is up to the caller.
    pub async fn save<P>(self, path: P) -> Result<(), TomlWriteError>
    where
        P: AsRef<Path>,
    {
        let path = path.as_ref();

        if !path.exists() {
            if let Some(dir) = path.parent() {
                info!("Creating missing cache directory: {}", dir.display());
                create_dir_all(dir).await?;
            };
        }

        let string = spawn_blocking(move || toml::to_string(&self))
            .await
            .expect("Failed to join a Cache TOML generation task")?;

        write(path, &string).await?;
        Ok(())
    }

    pub fn last_user(&self) -> Option<&str> {
        self.user_to_last_sess
            .first()
            .map(|(username, _)| username.as_str())
    }
}
