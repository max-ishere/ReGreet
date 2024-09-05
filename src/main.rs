// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

mod cache;
mod config;
mod constants;
mod error;
mod greetd;
mod gui;
mod sysutil;
pub mod tomlutils;

use std::collections::HashMap;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{Result as IoResult, Write};
use std::path::{Path, PathBuf};

use cache::{Cache, SessionIdOrCmdline};
use clap::{Parser, ValueEnum};
use constants::CACHE_PATH;
use file_rotate::{compression::Compression, suffix::AppendCount, ContentLimit, FileRotate};
use greetd::MockGreetd;
use gui::component::{App, AppInit, EntryOrDropDown, GreetdState};
use sysutil::SystemUsersAndSessions;
use tracing::subscriber::set_global_default;
use tracing::warn;
use tracing_appender::{non_blocking, non_blocking::WorkerGuard};
use tracing_subscriber::{
    filter::LevelFilter, fmt::layer, fmt::time::OffsetTime, layer::SubscriberExt,
};

use crate::constants::{APP_ID, CONFIG_PATH, CSS_PATH, LOG_PATH};

#[macro_use]
extern crate async_recursion;

#[cfg(test)]
#[macro_use]
extern crate test_case;

const MAX_LOG_FILES: usize = 3;
const MAX_LOG_SIZE: usize = 1024 * 1024;

#[derive(Clone, Debug, ValueEnum)]
enum LogLevel {
    Off,
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Parser, Debug)]
#[command(author, version, about)]
struct Args {
    /// The path to the log file
    #[arg(short = 'l', long, value_name = "PATH", default_value = LOG_PATH)]
    logs: PathBuf,

    /// The verbosity level of the logs
    #[arg(short = 'L', long, value_name = "LEVEL", default_value = "info")]
    log_level: LogLevel,

    /// Output all logs to stdout
    #[arg(short, long)]
    verbose: bool,

    /// The path to the config file
    #[arg(short, long, value_name = "PATH", default_value = CONFIG_PATH)]
    config: PathBuf,

    /// The path to the custom CSS stylesheet
    #[arg(short, long, value_name = "PATH", default_value = CSS_PATH)]
    style: PathBuf,

    /// Run in demo mode
    #[arg(long)]
    demo: bool,
}

fn main() {
    let Args {
        logs,
        log_level,
        verbose,
        config,
        style,
        demo,
    } = Args::parse();
    // Keep the guard alive till the end of the function, since logging depends on this.
    let _guard = init_logging(&logs, &log_level, verbose);

    // TODO: Is there a better way? we have to not start tokio until OffsetTime is initialized.
    // TODO: What on earth is this let binding?
    let (cache, SystemUsersAndSessions { users, sessions }) =
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let (cache, users) = tokio::join!(
                    Cache::load(CACHE_PATH),
                    SystemUsersAndSessions::load(Vec::new())
                );

                (
                    cache.unwrap_or_else(|err| {
                        warn!("Failed to load the cache, starting without it: {err}");
                        Cache::default()
                    }),
                    users.expect("Couldn't read available users and sessions"), // TODO: Don't panic here!
                )
            });

    let initial_user = cache
        .last_user()
        .and_then(|user| users.contains_key(user).then_some(user.to_string()))
        .unwrap_or_else(|| users.keys().next().cloned().unwrap_or_default()); // TODO: Make Init accept an option

    let mut last_user_session_cache: HashMap<_, _> = cache
        .user_to_last_sess
        .into_iter()
        .filter_map(|(username, session)| match session {
            SessionIdOrCmdline::ID(id) => sessions
                .contains_key(&id)
                .then_some((username, EntryOrDropDown::DropDown(id))),

            SessionIdOrCmdline::Command(cmd) => Some((username, EntryOrDropDown::Entry(cmd))),
        })
        .collect();

    let app = relm4::RelmApp::new(APP_ID);

    let users = users
        .into_iter()
        .map(|(sys, user)| {
            if sessions.is_empty() {
                last_user_session_cache
                    .insert(sys.clone(), EntryOrDropDown::Entry(user.shell().to_owned()));
            }
            (sys, user.full_name)
        })
        .collect();

    app.run::<App<MockGreetd>>(AppInit {
        users,
        sessions,
        initial_user,
        last_user_session_cache,
        greetd_state: GreetdState::AuthQuestion {
            session: MockGreetd {},
            credential: String::new(),
        },
    });
}

/// Initialize the log file with file rotation.
fn setup_log_file(log_path: &Path) -> IoResult<FileRotate<AppendCount>> {
    if !log_path.exists() {
        if let Some(log_dir) = log_path.parent() {
            create_dir_all(log_dir)?;
        };
    };

    // Manually write to the log file, since `FileRotate` will silently fail if the log file can't
    // be written to.
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)?;
    file.write_all(&[])?;

    Ok(FileRotate::new(
        log_path,
        AppendCount::new(MAX_LOG_FILES),
        ContentLimit::Bytes(MAX_LOG_SIZE),
        Compression::OnRotate(0),
        None,
    ))
}

/// Initialize logging with file rotation.
fn init_logging(log_path: &Path, log_level: &LogLevel, stdout: bool) -> Vec<WorkerGuard> {
    // Parse the log level string.
    let filter = match log_level {
        LogLevel::Off => LevelFilter::OFF,
        LogLevel::Error => LevelFilter::ERROR,
        LogLevel::Warn => LevelFilter::WARN,
        LogLevel::Info => LevelFilter::INFO,
        LogLevel::Debug => LevelFilter::DEBUG,
        LogLevel::Trace => LevelFilter::TRACE,
    };

    let timer = OffsetTime::local_rfc_3339().expect("Couldn't get local time offset");

    let builder = tracing_subscriber::fmt()
        .with_max_level(filter)
        .with_timer(timer.clone());

    // Log in a separate non-blocking thread, then return the guard (otherise the non-blocking
    // writer will immediately stop).
    let mut guards = Vec::new();
    match setup_log_file(log_path) {
        Ok(file) => {
            let (file, guard) = non_blocking(file);
            guards.push(guard);
            let builder = builder
                .with_writer(file)
                // Disable colouring through ANSI escape sequences in log files.
                .with_ansi(false);

            if stdout {
                let (stdout, guard) = non_blocking(std::io::stdout());
                guards.push(guard);
                set_global_default(
                    builder
                        .finish()
                        .with(layer().with_writer(stdout).with_timer(timer)),
                )
                .unwrap();
            } else {
                builder.init();
            };
        }
        Err(file_err) => {
            let (file, guard) = non_blocking(std::io::stdout());
            guards.push(guard);
            builder.with_writer(file).init();
            tracing::error!("Couldn't create log file '{LOG_PATH}': {file_err}");
        }
    };

    // Log all panics in the log file as well as stderr.
    std::panic::set_hook(Box::new(|panic| {
        tracing::error!("{panic}");
        eprintln!("{panic}");
    }));

    guards
}
