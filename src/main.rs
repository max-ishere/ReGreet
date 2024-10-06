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

use std::collections::HashMap;
use std::env;
use std::fs::{create_dir_all, OpenOptions};
use std::io::{Result as IoResult, Write};
use std::path::{Path, PathBuf};

use cache::{Cache, SessionIdOrCmdline};
use clap::{Parser, ValueEnum};
use config::{AppearanceConfig, BackgroundConfig, Config, SystemCommandsConfig};
use constants::CACHE_PATH;
use file_rotate::{compression::Compression, suffix::AppendCount, ContentLimit, FileRotate};
use greetd::{DemoGreetd, Greetd};
use gtk4::glib::markup_escape_text;
use gtk4::MessageType;
use gui::component::{App, AppInit, EntryOrDropDown, GreetdState, NotificationItemInit};
use relm4::RelmApp;
use sysutil::SystemUsersAndSessions;
use tokio::net::UnixStream;
use tracing::subscriber::set_global_default;
use tracing::{error, warn};
use tracing_appender::{non_blocking, non_blocking::WorkerGuard};
use tracing_subscriber::{
    filter::LevelFilter, fmt::layer, fmt::time::OffsetTime, layer::SubscriberExt,
};

use crate::constants::{APP_ID, CONFIG_PATH, LOG_PATH};

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
        demo,
    } = Args::parse();
    // Keep the guard alive till the end of the function, since logging depends on this.
    let (_guard, errors) = init_logging(&logs, &log_level, verbose);

    // We cannot use #[tokio::main] because init_logging uses OffsetTime, which requires it be init'd before tokio or
    // threads are created.
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main(config, demo, errors));
}

async fn async_main(config: PathBuf, demo: bool, mut errors: Vec<NotificationItemInit>) {
    let (cache, mut config, users, new_errors) = load_files(config).await;
    errors.extend(new_errors);

    let app = RelmApp::new(APP_ID);

    if demo {
        config.commands.reboot = vec![];
        config.commands.poweroff = vec![];

        let greetd_state = GreetdState::AuthQuestion {
            session: DemoGreetd {},
            credential: String::new(),
        };

        app.run::<App<DemoGreetd>>(mk_app_init(greetd_state, cache, users, config, errors));

        return;
    }

    let socket_path = env::var("GREETD_SOCK").unwrap();

    let socket = UnixStream::connect(socket_path).await.unwrap();

    let greetd_state = GreetdState::NotCreated(socket);

    app.run::<App<UnixStream>>(mk_app_init(greetd_state, cache, users, config, errors));
}

async fn load_files<P>(
    config: P,
) -> (
    Cache,
    Config,
    SystemUsersAndSessions,
    Vec<NotificationItemInit>,
)
where
    P: AsRef<Path>,
{
    let mut errors = vec![];

    let (cache, config) = tokio::join!(Cache::load(CACHE_PATH), Config::load(config),);

    let cache = cache.unwrap_or_else(|err| {
        let warning = format!("Failed to load the cache, starting without it: {err}");
        warn!(warning);
        errors.push(NotificationItemInit {
            markup_text: markup_escape_text(&warning).to_string(),
            message_type: MessageType::Warning,
        });

        Cache::default()
    });

    let config = config.unwrap_or_else(|err| {
        let warning = format!("Failed to load the config file, starting with the defaults: {err}");
        warn!(warning);
        errors.push(NotificationItemInit {
            markup_text: markup_escape_text(&warning).to_string(),
            message_type: MessageType::Warning,
        });

        Config::default()
    });

    let users = SystemUsersAndSessions::load(&config.commands.x11_prefix)
        .await
        .unwrap_or_else(|err| {
            let warning = format!("Failed to the list of users and sessions on this system, starting with no options: {err}");
            warn!(warning);
            errors.push(NotificationItemInit {
                markup_text: markup_escape_text(&warning).to_string(),
                message_type: MessageType::Warning,
            });

            SystemUsersAndSessions::default()
        });

    (cache, config, users, errors)
}

fn mk_app_init<Client>(
    greetd_state: GreetdState<Client>,
    cache: Cache,
    users: SystemUsersAndSessions,
    config: Config,
    errors: Vec<NotificationItemInit>,
) -> AppInit<Client>
where
    Client: Greetd,
{
    let SystemUsersAndSessions { users, sessions } = users;
    let Config {
        appearance,
        background,
        commands,
        env,
    } = config;

    let BackgroundConfig { path: picture, fit } = background;
    let AppearanceConfig { greeting_msg } = appearance;
    let SystemCommandsConfig {
        reboot,
        poweroff,
        x11_prefix: _,
    } = commands;

    let initial_user = cache
        .last_user()
        .and_then(|user| users.contains_key(user).then_some(user.to_string()))
        .unwrap_or_else(|| users.keys().next().cloned().unwrap_or_default());

    let mut last_user_session_cache: HashMap<_, _> = cache
        .user_to_last_sess
        .into_iter()
        .filter_map(|(username, session)| match session {
            SessionIdOrCmdline::XdgDektopFile(id) => sessions
                .contains_key(&id)
                .then_some((username, EntryOrDropDown::DropDown(id))),

            SessionIdOrCmdline::Command(cmd) => Some((username, EntryOrDropDown::Entry(cmd))),
        })
        .collect();

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

    AppInit {
        users,
        sessions,
        env,
        initial_user,
        last_user_session_cache,
        greetd_state,
        picture,
        fit: fit.into(),
        title_message: greeting_msg,
        reboot_cmd: reboot,
        poweroff_cmd: poweroff,

        notifications: errors,
    }
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
fn init_logging(
    log_path: &Path,
    log_level: &LogLevel,
    stdout: bool,
) -> (Vec<WorkerGuard>, Vec<NotificationItemInit>) {
    let mut errors = vec![];

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

            let error = format!("Couldn't create log file '{LOG_PATH}': {file_err}");
            error!(error);
            errors.push(NotificationItemInit {
                markup_text: markup_escape_text(&error).to_string(),
                message_type: MessageType::Error,
            })
        }
    };

    // Log all panics in the log file as well as stderr.
    std::panic::set_hook(Box::new(|panic| {
        tracing::error!("{panic}");
        eprintln!("{panic}");
    }));

    (guards, errors)
}
