// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helper for system utilities like users and sessions

use std::collections::hash_map;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::{collections::HashMap, env};

use freedesktop_entry_parser::Entry;
use pwd::Passwd;
use thiserror::Error;
use tokio::fs::{read, read_dir, read_to_string};
use tokio::task::spawn_blocking;
use tracing::{debug, warn};

/// Stores info of all regular users and sessions
#[derive(Default, Debug)]
pub struct SystemUsersAndSessions {
    /// Maps from system usename to [`User`].
    pub users: HashMap<String, User>,
    /// Maps a session's xdg desktop file id to [`SessionInfo`].
    pub sessions: HashMap<String, SessionInfo>,
}

#[derive(Default, Debug)]
pub struct User {
    pub full_name: String,
    // TODO: there should be separate UI for selecting a shell due to special meaning of the dropdown and how it translates to a cache type
    // Potentially make the session label into a dropdown that selects [session, shell, command]
    login_shell: Option<String>,
}

impl User {
    pub const DEFAULT_SHELL: &'static str = "/bin/sh";

    pub fn shell(&self) -> &str {
        self.login_shell.as_deref().unwrap_or(Self::DEFAULT_SHELL)
    }
}

impl SystemUsersAndSessions {
    const SESSION_DIRS_ENV: &'static str = "XDG_DATA_DIRS";
    const SESSION_DIRS_DEFAULT: &'static str = "/usr/local/share/:/usr/share/";

    pub async fn load(x11_prefix: &[String]) -> io::Result<Self> {
        let uid_limit = match read_to_string(NormalUser::PATH).await {
            Ok(text) => spawn_blocking(move || NormalUser::parse_login_defs(&text))
                .await
                .unwrap(),
            Err(e) => {
                warn!("{e}");

                NormalUser::default()
            }
        };

        let (users, sessions) = tokio::join!(
            spawn_blocking(move || Self::init_users(uid_limit)),
            Self::init_sessions(x11_prefix)
        );

        let users = users.unwrap().unwrap_or_default();
        let sessions = sessions.unwrap_or_default();

        Ok(Self { users, sessions })
    }

    fn init_users(uid_limit: NormalUser) -> io::Result<HashMap<String, User>> {
        debug!("{uid_limit:?}");

        let mut users = HashMap::new();

        for entry in Passwd::iter().filter(|Passwd { uid, .. }| uid_limit.is_normal_user(*uid)) {
            let full_name = entry
                .gecos
                .filter(|gecos| !gecos.is_empty())
                .as_ref()
                .map(|full_gecos| {
                    full_gecos
                        .split_once(',')
                        .map(|(name, _)| name)
                        .unwrap_or(full_gecos)
                })
                .map(str::trim)
                .map(|name| {
                    if name == "&" {
                        return capitalize(&entry.name);
                    }

                    name.to_owned()
                })
                .unwrap_or({
                    debug!(
                        "User {} has no full name specified in gecos (/etc/passwd field)",
                        entry.name
                    );
                    entry.name.clone()
                });

            let login_shell = (!entry.shell.is_empty()).then_some(entry.shell);

            users.insert(
                entry.name.clone(),
                User {
                    full_name,
                    login_shell,
                },
            );
        }

        Ok(users)
    }

    /// Get the avaliable graphical X11 and Wayland sessions. These are retrieved from [`Self::SESSION_DIRS_ENV`]
    /// (defaults to [`Self::SESSION_DIRS_DEFAULT`]). For each directory from the env, scans `/xsessions` and
    /// `/wayland-sessions` (Wayland takes priority if an x11 desktop file has the same ID). The resulting hashmap maps
    /// the desktop file ID to the information about that session file.
    ///
    /// For each X11 session, `x11_prefix` is added.
    async fn init_sessions(x11_prefix: &[String]) -> io::Result<HashMap<String, SessionInfo>> {
        let session_dirs = env::var(Self::SESSION_DIRS_ENV)
            .into_iter()
            .find(|s| !s.is_empty())
            .unwrap_or_else(|| Self::SESSION_DIRS_DEFAULT.to_string());

        let (x11_dirs, wayland_dirs): (Vec<_>, Vec<_>) = session_dirs
            .split(':')
            .map(|dir| {
                (
                    PathBuf::from(dir).join("xsessions"),
                    PathBuf::from(dir).join("wayland-sessions"),
                )
            })
            .unzip();

        let (x11_entries, wayland_entries) = tokio::join!(
            Self::get_desktop_entries_in_dirs(x11_dirs),
            Self::get_desktop_entries_in_dirs(wayland_dirs),
        );

        let mut x11_entries = x11_entries.unwrap_or_default();
        let wayland_entries = wayland_entries.unwrap_or_default();

        x11_entries.iter_mut().for_each(|(_, v)| {
            let mut command = x11_prefix.to_vec();
            command.append(&mut v.command);

            v.command = command;
        });

        x11_entries.extend(wayland_entries);
        Ok(x11_entries)
    }

    /// Given a list of directories (in order as they appear in the env var) scan those dirs recursively.
    /// For each `*.desktop` file, process it and place into the hash map.
    /// However if a desktop file's id ([as defined by the XDG spec](https://specifications.freedesktop.org/desktop-entry-spec/latest/file-naming.html#desktop-file-id))
    /// is already processed, skip the identical id.
    async fn get_desktop_entries_in_dirs<P>(
        dirs: Vec<P>,
    ) -> Result<HashMap<String, SessionInfo>, DesktopFileError>
    where
        P: AsRef<Path> + std::marker::Send + 'static + std::marker::Sync,
    {
        let mut dirs_of_files = Vec::new();

        for dir in dirs {
            let Ok(files) = Self::recursively_find_desktop_files(&dir).await else {
                // Try to collect as many entries that yield Ok without early return
                continue;
            };

            dirs_of_files.push((dir, files));
        }

        let mut map = HashMap::new();
        for (id, file) in dirs_of_files.into_iter().flat_map(|(base, files)| {
            files
                .into_iter()
                .map(move |file| (Self::desktop_file_id(&base, &file), file))
        }) {
            let map_entry = map.entry(id);

            if matches!(map_entry, hash_map::Entry::Vacant(_)) {
                let Ok(Some(entry)) = SessionInfo::load(file).await else {
                    continue;
                };

                // Cannot use or_insert_with because of async.
                map_entry.or_insert(entry);
            }
        }

        Ok(map)
    }

    /// Iterates over the directory and yields everything that has a `.desktop` extension.
    /// If the entry is a directory, recurses into it and appends all the files there to the list.
    #[async_recursion]
    async fn recursively_find_desktop_files<P>(dir: P) -> io::Result<Vec<PathBuf>>
    where
        P: AsRef<Path> + std::marker::Send,
    {
        // You will see a lot of `let Ok else continue` in this function.
        // This is because we try to ignore as many errors as possible.
        // Otherwise even a single permission error can cause the session list to be empty.
        let mut ls = read_dir(dir).await?;

        let mut files = Vec::new();
        while let Some(entry) = ls.next_entry().await? {
            let Ok(file_type) = entry.file_type().await else {
                continue;
            };

            if file_type.is_dir() {
                let Ok(mut recursed) = Self::recursively_find_desktop_files(entry.path()).await
                else {
                    continue;
                };

                files.append(&mut recursed);

                continue;
            }

            if !entry
                .path()
                .extension()
                .map(|e| e.to_string_lossy() == "desktop")
                .unwrap_or(false)
            {
                continue;
            };

            files.push(entry.path());
        }

        Ok(files)
    }

    /// Returns a dektop file id given a base directory and a path to the desktop file. The algorithm is described in
    /// the [XDG spec: Desktop File ID](https://specifications.freedesktop.org/desktop-entry-spec/latest/file-naming.html#desktop-file-id).
    ///
    /// # Panics
    ///
    /// This function requires that `base` is prefix of `file`.
    fn desktop_file_id<P1, P2>(base: P1, file: P2) -> String
    where
        P1: AsRef<Path>,
        P2: AsRef<Path>,
    {
        let base = base.as_ref();
        let file = file.as_ref();

        let path = file.strip_prefix(base).unwrap().with_extension("");

        path.iter()
            .map(OsStr::to_string_lossy)
            .fold(String::new(), |acc, item| acc + &item)
    }
}

/// Returs input, but the first character is capitalized.
fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(ch) => ch.to_uppercase().chain(chars).collect(),
    }
}

/// A named tuple of min and max that stores UID limits for normal users.
#[derive(Debug, PartialEq, Eq)]
struct NormalUser {
    min_uid: u64,
    max_uid: u64,
}

impl Default for NormalUser {
    fn default() -> Self {
        Self {
            min_uid: Self::MIN_DEFAULT,
            max_uid: Self::MAX_DEFAULT,
        }
    }
}

impl NormalUser {
    /// Path to a file that can be parsed by [`Self::parse_login_defs`].
    pub const PATH: &'static str = "/etc/login.defs";

    pub const MIN_DEFAULT: u64 = 1_000;
    pub const MAX_DEFAULT: u64 = 60_000;

    /// Parses the [`Self::PATH`] file content and looks for `UID_MIN` and `UID_MAX` definitions. If a definition is
    /// missing or causes parsing errors, the default values [`Self::MIN_DEFAULT`] and [`Self::MAX_DEFAULT`] are used.
    ///
    /// This parser is highly specific to parsing the 2 required values, thus it focuses on doing the least amout of
    /// compute required to extracting them.
    ///
    /// Errors are dropped because they are unlikely and their handling would result in the use of default values
    /// anyway.
    pub fn parse_login_defs(text: &str) -> Self {
        let mut min = None;
        let mut max = None;

        for line in text.lines().map(str::trim) {
            // We
            if let Some(min_str) = min
                .is_none()
                .then(|| line.strip_prefix("UID_MIN"))
                .flatten()
            {
                if min_str.starts_with(char::is_whitespace) {
                    min = Self::parse_number(min_str);
                }
            } else if let Some(max_str) = max
                .is_none()
                .then(|| line.strip_prefix("UID_MAX"))
                .flatten()
            {
                if max_str.starts_with(char::is_whitespace) {
                    max = Self::parse_number(max_str);
                }
            }

            if min.is_some() && max.is_some() {
                break;
            }
        }

        Self {
            min_uid: min.unwrap_or(Self::MIN_DEFAULT),
            max_uid: max.unwrap_or(Self::MAX_DEFAULT),
        }
    }

    // Returns true for regular users, false for those outside the UID limit, eg. git or root.
    pub fn is_normal_user<T>(&self, uid: T) -> bool
    where
        T: Into<u64>,
    {
        (self.min_uid..self.max_uid).contains(&uid.into())
    }

    fn parse_number(num: &str) -> Option<u64> {
        let num = num.trim();
        if num == "0" {
            return Some(0);
        }

        if let Some(octal) = num.strip_prefix('0') {
            if let Some(hex) = octal.strip_prefix('x') {
                return u64::from_str_radix(hex, 16).ok();
            }

            return u64::from_str_radix(octal, 8).ok();
        }

        num.parse().ok()
    }
}

#[derive(Debug)]
pub struct SessionInfo {
    /// The displayed name of the session
    pub name: String,
    /// The command to run when the session starts.
    pub command: Vec<String>,
}

impl SessionInfo {
    async fn load<P>(path: P) -> Result<Option<Self>, DesktopFileError>
    where
        P: AsRef<Path>,
    {
        let skip = Ok(None);

        let contents = read(path).await?;
        let desktop_file = Entry::parse(contents)?;
        let entry = desktop_file.section("Desktop Entry");

        if let Some("true") = entry.attr("Hidden") {
            return skip;
        }

        if let Some("true") = entry.attr("NoDisplay") {
            return skip;
        }

        let Some(name) = entry.attr("Name") else {
            return skip;
        };

        let Some(exec) = entry.attr("Exec") else {
            return skip;
        };

        Ok(shlex::split(exec).map(|command| Self {
            name: name.to_string(),
            command,
        }))
    }
}

/// Represents errors from loading the xdg desktop files.
#[derive(Error, Debug)]
pub enum DesktopFileError {
    #[error("I/O error occured while reading a desktop file: {0}")]
    IO(#[from] io::Error),

    #[error("XDG desktop file parsing error: {0}")]
    Xdg(#[from] freedesktop_entry_parser::ParseError),
}

#[cfg(test)]
mod tests {
    #[allow(non_snake_case)]
    mod UidLimit {
        use crate::sysutil::NormalUser;

        #[test_case(
            "UID_MIN 1
            UID_MAX 10"
            => NormalUser { min_uid: 1, max_uid: 10 };
            "both configured"
        )]
        #[test_case(
            "UID_MAX 10
            UID_MIN 1"
            => NormalUser { min_uid: 1, max_uid: 10 };
            "reverse order"
        )]
        #[test_case(
            "OTHER 20
            # Comment

            UID_MAX 10
            UID_MIN 1
            MORE_TEXT 40
            "
            => NormalUser { min_uid: 1, max_uid: 10 };
            "complex file"
        )]
        #[test_case(
            "UID_MAX10"
            => NormalUser::default();
            "no space"
        )]

        fn parse_login_defs(text: &str) -> NormalUser {
            NormalUser::parse_login_defs(text)
        }

        #[test_case("" => None; "empty")]
        #[test_case("no" => None; "string")]
        #[test_case("0" => Some(0); "zero")]
        #[test_case("0x" => None; "0x isn't a hex number")]
        #[test_case("10" => Some(10); "decimal")]
        #[test_case("0777" => Some(0o777); "octal")]
        #[test_case("0xDeadBeef" => Some(0xDeadBeef); "hex")]
        fn parse_number(num: &str) -> Option<u64> {
            NormalUser::parse_number(num)
        }
    }
}
