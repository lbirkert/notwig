#![forbid(unsafe_code)]

pub mod build;
mod error;
pub mod scaffold;
pub mod target;

pub use error::{Error, Result};

pub const USAGE: &str = "\
notwig-ext-cli — build and scaffold Notwig extensions

Usage:
  notwig-ext-cli new <path> [--name <package>] [--id <extension-id>]
  notwig-ext-cli init [path] [--name <package>] [--id <extension-id>]
  notwig-ext-cli build <target> [--path <dir>] [--out-dir <dir>] [--name <zip-name>]
  notwig-ext-cli targets
  notwig-ext-cli help

Build looks for extension.json in the supplied path or its parents, runs
`cargo build --release` for the requested Rust target, adds the generated
entrypoint to extension.json, and writes a ZIP package.
";

pub fn run(args: &[String]) -> Result<()> {
    let Some(command) = args.first().map(String::as_str) else {
        print!("{USAGE}");
        return Ok(());
    };
    match command {
        "new" => scaffold::new_command(&args[1..]),
        "init" => scaffold::init_command(&args[1..]),
        "build" => build::command(&args[1..]),
        "targets" => {
            for target in target::all() {
                println!(
                    "{:<16} {:<28} .{}",
                    target.canonical, target.rust_triple, target.extension
                );
            }
            Ok(())
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(Error::message(format!(
            "unknown command `{other}` (try `notwig-ext-cli help`)"
        ))),
    }
}
