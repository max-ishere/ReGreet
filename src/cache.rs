// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Utility for caching info between logins

use std::{collections::HashSet, path::Path};

use serde::{Deserialize, Serialize};
use tokio::{
    fs::{create_dir_all, read_to_string, write},
    task::spawn_blocking,
};

use crate::error::{TomlReadError, TomlWriteError};

/// Holds info needed to persist between logins
#[derive(Deserialize, Serialize, Default, Debug, PartialEq, Eq)]
pub struct Cache {
    /// An ordered map from username to the last session. First is most recent.
    #[serde(with = "tuple_vec_map")]
    pub user_to_last_sess: Vec<(String, SessionIdOrCmdline)>,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionIdOrCmdline {
    #[serde(rename = "xdg")]
    XdgDektopFile(String),

    #[serde(rename = "cmd")]
    Command(String),
}

impl Cache {
    /// Load the cache from disk with this size limit. If the size should be 0, use [`Default`].
    pub async fn load<P>(path: P, limit: usize) -> Result<Self, TomlReadError>
    where
        P: AsRef<Path>,
    {
        let string = read_to_string(path).await?;
        let mut cache: Self = spawn_blocking(move || toml::from_str(&string))
            .await
            .unwrap()?;

        cache.dedup_user_to_last_sess();
        cache.user_to_last_sess.truncate(limit);

        Ok(cache)
    }

    /// Save the cache file to disk.
    ///
    /// This function consumes self because of optimization reasons.
    ///
    /// 1. It will only be run during the shutdown process.
    /// 2. Serde calls can take a long time before this async fn yields, so self would have to be moved into
    ///    [`spawn_blocking`], and whether or not a clone of &self is cost effective is up to the caller.
    pub async fn save<P>(mut self, path: P, limit: usize) -> Result<(), TomlWriteError>
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

        self.dedup_user_to_last_sess();
        self.user_to_last_sess.truncate(limit);

        let string = spawn_blocking(move || toml::to_string(&self))
            .await
            .expect("Failed to join a Cache TOML generation task")?;

        write(path, &string).await?;
        Ok(())
    }

    pub fn last_user_session(&self, username: &str) -> Option<&SessionIdOrCmdline> {
        self.user_to_last_sess
            .iter()
            .find_map(|(user, session)| (user == username).then_some(session))
    }

    pub fn set_last_login(&mut self, username: String, session: SessionIdOrCmdline) {
        self.user_to_last_sess.insert(0, (username, session));
        self.dedup_user_to_last_sess()
    }

    pub fn last_user(&self) -> Option<&str> {
        self.user_to_last_sess
            .first()
            .map(|(username, _)| username.as_str())
    }

    fn dedup_user_to_last_sess(&mut self) {
        let mut set = HashSet::new();
        self.user_to_last_sess
            .retain(|(user, _)| set.insert(user.clone()));
    }
}

#[cfg(test)]
mod tests {

    #[allow(non_snake_case)]
    mod Cache {
        use super::super::*;
        use SessionIdOrCmdline as S;

        #[test_case(
            1
            => vec![
                (1.to_string(), "after".to_string()),
                (2.to_string(), "before".to_string()),
                (3.to_string(), "before".to_string()),
            ]
            ; "beginning"
        )]
        #[test_case(
            2
            => vec![
                (2.to_string(), "after".to_string()),
                (1.to_string(), "before".to_string()),
                (3.to_string(), "before".to_string()),
            ]
            ; "middle"
        )]
        #[test_case(
            3
            => vec![
                ("3".to_string(), "after".to_string()),
                ("1".to_string(), "before".to_string()),
                ("2".to_string(), "before".to_string()),
            ]
            ; "end"
        )]
        fn set_last_login(index: i32) -> Vec<(String, String)> {
            let mut cache = Cache {
                user_to_last_sess: (1..=3)
                    .map(|i| (i.to_string(), S::XdgDektopFile("before".to_string())))
                    .collect(),
            };

            cache.set_last_login(index.to_string(), S::XdgDektopFile("after".to_string()));
            cache
                .user_to_last_sess
                .into_iter()
                .map(|(user, session)| {
                    (user, {
                        let S::XdgDektopFile(file) = session else {
                            unreachable!();
                        };
                        file
                    })
                })
                .collect()
        }
    }
}
