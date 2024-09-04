// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Helper for system utilities like users and sessions

use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::fs::read;
use std::io;
use std::io::Result as IOResult;
use std::path::Path;
use std::str::from_utf8;

use glob::glob;
use pwd::Passwd;
use regex::Regex;
use relm4::spawn_blocking;
use thiserror::Error;
use tokio::fs::read_to_string;
use tracing::{debug, info, warn};

use crate::constants::SESSION_DIRS;

const XDG_DIR_ENV_VAR: &'static str = "XDG_DATA_DIRS";

type SessionMap = HashMap<String, Vec<String>>;

/// Stores info of all regular users and sessions
pub struct SystemUsersAndSessions {
    /// Maps from system usename to [`User`].
    pub users: HashMap<String, User>,
    /// Maps a session's full name to its command
    pub sessions: SessionMap,
}

pub struct User {
    pub full_name: String,
    pub login_shell: Option<String>,
}

impl User {
    pub const DEFAULT_SHELL: &'static str = "/bin/sh";

    pub fn shell(&self) -> &str {
        self.login_shell.as_deref().unwrap_or(Self::DEFAULT_SHELL)
    }
}

impl SystemUsersAndSessions {
    pub async fn load() -> IOResult<(Self, Vec<NonFatalError>)> {
        let mut non_fatal_errors = Vec::new();

        let uid_limit = match read_to_string(NormalUser::PATH).await {
            Ok(text) => spawn_blocking(move || NormalUser::parse_login_defs(&text))
                .await
                .unwrap(),
            Err(e) => {
                let e = NonFatalError::UidLimitRead(e);
                warn!("{e}");
                non_fatal_errors.push(e);

                NormalUser::default()
            }
        };

        let users = Self::init_users(uid_limit)?;

        Ok((
            Self {
                users,
                sessions: Self::init_sessions()?,
            },
            non_fatal_errors,
        ))
    }

    fn init_users(uid_limit: NormalUser) -> IOResult<HashMap<String, User>> {
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

    /// Get available X11 and Wayland sessions.
    ///
    /// These are defined as either X11 or Wayland session desktop files stored in specific
    /// directories.
    fn init_sessions() -> IOResult<SessionMap> {
        let mut found_session_names = HashSet::new();
        let mut sessions = HashMap::new();

        // Use the XDG spec if available, else use the one that's compiled.
        // The XDG env var can change after compilation in some distros like NixOS.
        let session_dirs = if let Ok(sess_parent_dirs) = env::var(XDG_DIR_ENV_VAR) {
            debug!("Found XDG env var {XDG_DIR_ENV_VAR}: {sess_parent_dirs}");
            match sess_parent_dirs
                .split(':')
                .map(|parent_dir| format!("{parent_dir}/xsessions:{parent_dir}/wayland-sessions"))
                .reduce(|a, b| a + ":" + &b)
            {
                None => SESSION_DIRS.to_string(),
                Some(dirs) => dirs,
            }
        } else {
            SESSION_DIRS.to_string()
        };

        for sess_dir in session_dirs.split(':') {
            let sess_parent_dir = if let Some(sess_parent_dir) = Path::new(sess_dir).parent() {
                sess_parent_dir
            } else {
                warn!("Session directory does not have a parent: {sess_dir}");
                continue;
            };
            debug!("Checking session directory: {sess_dir}");
            // Iterate over all '.desktop' files.
            for glob_path in glob(&format!("{sess_dir}/*.desktop"))
                .expect("Invalid glob pattern for session desktop files")
            {
                let path = match glob_path {
                    Ok(path) => path,
                    Err(err) => {
                        warn!("Error when globbing: {err}");
                        continue;
                    }
                };
                info!("Now scanning session file: {}", path.display());

                let contents = read(&path)?;
                let text = from_utf8(contents.as_slice()).unwrap_or_else(|err| {
                    panic!("Session file '{}' is not UTF-8: {}", path.display(), err)
                });

                let fname_and_type = match path.strip_prefix(sess_parent_dir) {
                    Ok(fname_and_type) => fname_and_type.to_owned(),
                    Err(err) => {
                        warn!("Error with file name: {err}");
                        continue;
                    }
                };

                if found_session_names.contains(&fname_and_type) {
                    debug!(
                        "{fname_and_type:?} was already found elsewhere, skipping {}",
                        path.display()
                    );
                    continue;
                };

                // The session launch command is specified as: Exec=command arg1 arg2...
                let cmd_regex =
                    Regex::new(r"Exec=(.*)").expect("Invalid regex for session command");
                // The session name is specified as: Name=My Session
                let name_regex = Regex::new(r"Name=(.*)").expect("Invalid regex for session name");

                // Hiding could be either as Hidden=true or NoDisplay=true
                let hidden_regex = Regex::new(r"Hidden=(.*)").expect("Invalid regex for hidden");
                let no_display_regex =
                    Regex::new(r"NoDisplay=(.*)").expect("Invalid regex for no display");

                let hidden: bool = if let Some(hidden_str) = hidden_regex
                    .captures(text)
                    .and_then(|capture| capture.get(1))
                {
                    hidden_str.as_str().parse().unwrap_or(false)
                } else {
                    false
                };

                let no_display: bool = if let Some(no_display_str) = no_display_regex
                    .captures(text)
                    .and_then(|capture| capture.get(1))
                {
                    no_display_str.as_str().parse().unwrap_or(false)
                } else {
                    false
                };

                if hidden | no_display {
                    found_session_names.insert(fname_and_type);
                    continue;
                };

                // Parse the desktop file to get the session command.
                let cmd = if let Some(cmd_str) =
                    cmd_regex.captures(text).and_then(|capture| capture.get(1))
                {
                    if let Some(cmd) = shlex::split(cmd_str.as_str()) {
                        cmd
                    } else {
                        warn!(
                            "Couldn't split command of '{}' into arguments: {}",
                            path.display(),
                            cmd_str.as_str()
                        );
                        // Skip the desktop file, since a missing command means that we can't
                        // use it.
                        continue;
                    }
                } else {
                    warn!("No command found for session: {}", path.display());
                    // Skip the desktop file, since a missing command means that we can't use it.
                    continue;
                };

                // Get the full name of this session.
                let name = if let Some(name) =
                    name_regex.captures(text).and_then(|capture| capture.get(1))
                {
                    debug!(
                        "Found name '{}' for session '{}' with command '{:?}'",
                        name.as_str(),
                        path.display(),
                        cmd
                    );
                    name.as_str()
                } else if let Some(stem) = path.file_stem() {
                    // Get the stem of the filename of this desktop file.
                    // This is used as backup, in case the file name doesn't exist.
                    if let Some(stem) = stem.to_str() {
                        debug!(
                            "Using file stem '{stem}', since no name was found for session: {}",
                            path.display()
                        );
                        stem
                    } else {
                        warn!("Non-UTF-8 file stem in session file: {}", path.display());
                        // No way to display this session name, so just skip it.
                        continue;
                    }
                } else {
                    warn!("No file stem found for session: {}", path.display());
                    // No file stem implies no file name, which shouldn't happen.
                    // Since there's no full name nor file stem, just skip this anomalous
                    // session.
                    continue;
                };
                found_session_names.insert(fname_and_type);
                sessions.insert(name.to_string(), cmd);
            }
        }

        Ok(sessions)
    }
}

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

/// Represents an error that is not considered fatal for loading the user and session information, however the
/// assumptions that the loading process makes may cause unexpected behavior.
///
/// This should be shown to the user in the UI.
#[derive(Error, Debug)]
pub enum NonFatalError {
    #[error("Failed to read UID limits defined in '{}': {0}", NormalUser::PATH)]
    UidLimitRead(#[from] io::Error),
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
