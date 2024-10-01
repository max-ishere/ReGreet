// SPDX-FileCopyrightText: 2022 Harish Rajagopal <harish.rajagopals@gmail.com>
//
// SPDX-License-Identifier: GPL-3.0-or-later

//! Wrap cargo run with fakegreet. See `cargo fakegreet --help` or [`Args`].
//!
//! # Installation
//!
//! To install this cargo subcommand do one of the following:
//!
//! ## Install for this user
//!
//! ```
//! cargo install --path . --example cargo-fakegreet
//! ```
//!
//! ## Add examples directory to $PATH
//!
//! ```
//! cargo b --example cargo-fakegreet
//! PATH=$PATH:target/debug/examples/
//! ```

use anyhow::Context;
use clap::Parser;
use signal_hook::{consts::SIGINT, iterator::Signals};
use std::{
    env,
    fs::remove_file,
    io,
    process::{Child, Command},
};

/// A cargo subcommand to wrap `cargo run` with fakegreet.
///
/// All options are passed to `cargo run` verbatim. Under the hood this subcommand calls fakegreet like this:
///
/// fakegreet 'cargo run {cargo_run_args} -- {exe_args}'
///
/// This tool also deletes the socket file (assumes it is called greetd.sock) on exit.
#[derive(Parser, Debug)]
#[command(bin_name = "cargo fakegreet")]
struct Args {
    /// Arguments to be given to `cargo run {here}`. See `cargo help run`.
    #[arg(allow_hyphen_values = true, value_name = "options")]
    cargo_run_args: Vec<String>,

    /// Arguments to be given after `cargo run -- {here}`. See `cargo run -- --help`.
    #[arg(allow_hyphen_values = true, value_name = "args", last = true)]
    exe_args: Vec<String>,
}

fn main() -> anyhow::Result<()> {
    let Args {
        cargo_run_args,
        exe_args,
    } = if env::args().skip(1).next().as_deref() == Some("fakegreet") {
        CargoSubcommand::parse().inner
    } else {
        Args::parse()
    };

    let mut argv = vec!["cargo".to_string(), "run".to_string()];

    argv.extend(cargo_run_args);

    if !exe_args.is_empty() {
        argv.push("--".to_string());
        argv.extend(exe_args);
    }

    let arg_str = shlex::try_join(argv.iter().map(String::as_str))
        .with_context(|| format!("Failed to convert arguments to a string: {argv:?}"))?;

    let child_proc = Command::new("fakegreet")
        .arg(&arg_str)
        .spawn()
        .with_context(|| format!("Failed to spawn fakegreet {arg_str}"))?;

    let _cleanup: CleanupOnDrop = child_proc.into();

    // Block the current thread until Ctrl+C
    let _ = Signals::new([SIGINT])
        .expect("signals are valid")
        .wait()
        .next();

    Ok(())
}

#[derive(Parser)]
struct CargoSubcommand {
    /// Removes the subcommand argument given to us by cargo.
    ///
    /// When run as `cargo <sub>` the `cargo-<sub>` receives the entire `args()[1..]` from cargo (which includes the
    /// `<sub>` word in `args()[1]`). This field removes that argument from the list.
    #[doc(hidden)]
    #[arg(value_parser = ["fakegreet"], hide = true)]
    _cargo_subcmd: String,

    #[command(flatten)]
    pub inner: Args,
}

/// Performs cleanup when the application exits such as:
///
/// 1. Killing fakegreet
/// 2. Deleting the socket file (assumes it will be called greetd.sock)
struct CleanupOnDrop {
    child: Child,
}

impl Drop for CleanupOnDrop {
    fn drop(&mut self) {
        // Do not use `?` because we need to attempt all steps!
        // We emulate cool-kids (`anyhow`) error reporting by attaching a context and debug printing the error.

        if let Err(e) = self
            .child
            .kill()
            .with_context(|| "Failed to kill the child process")
        {
            println!("{e:?}");
        }

        match remove_file("greetd.sock") {
            Ok(()) => (),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => {
                let Err(e): anyhow::Result<()> =
                    Err(e).with_context(|| "Failed to remove the greetd socket file")
                else {
                    unreachable!()
                };

                println!("{e:?}");
            }
        };
    }
}

impl From<Child> for CleanupOnDrop {
    fn from(child: Child) -> Self {
        CleanupOnDrop { child }
    }
}
