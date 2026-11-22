#![forbid(unsafe_code)]

pub mod build;
mod error;
pub mod scaffold;
pub mod target;

pub use error::{Error, Result};

use std::path::PathBuf;

use clap::{CommandFactory, Parser, Subcommand};

/// Build and scaffold Notwig extensions.
#[derive(Debug, Parser)]
#[command(
    name = "notwig-ext-cli",
    version,
    about = "Build and scaffold Notwig extensions",
    long_about = "Build source extension projects into target-specific ZIP packages, or scaffold a new extension with a working device, manifest, and icon."
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a new extension project.
    New {
        /// Directory to create.
        path: PathBuf,
        /// Cargo package name (defaults to the directory name).
        #[arg(long)]
        name: Option<String>,
        /// Extension id (defaults to com.example.<name>).
        #[arg(long = "id")]
        id: Option<String>,
    },
    /// Add extension files to an existing directory.
    Init {
        /// Directory to initialize (defaults to the current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Cargo package name (defaults to the directory name).
        #[arg(long)]
        name: Option<String>,
        /// Extension id (defaults to com.example.<name>).
        #[arg(long = "id")]
        id: Option<String>,
    },
    /// Build one entrypoint binary or package existing binaries into a ZIP.
    ///
    /// Use `--entrypoint <target>` on each target VM. Copy the produced
    /// binaries into the same Cargo target directory, then run `--package` to
    /// bundle every available target together.
    Build {
        /// Build only this target's entrypoint binary.
        #[arg(long, value_name = "TARGET", conflicts_with = "package")]
        entrypoint: Option<String>,
        /// Package the target binaries already present in Cargo's target directory.
        #[arg(long)]
        package: bool,
        /// Targets to include when packaging (defaults to all available binaries).
        #[arg(value_name = "TARGET")]
        targets: Vec<String>,
        /// Extension directory containing extension.json (defaults to the current directory).
        #[arg(long, default_value = ".")]
        path: PathBuf,
        /// Directory for the generated ZIP.
        #[arg(long, default_value = "dist")]
        out_dir: PathBuf,
        /// ZIP file name (defaults to <extension-id>.zip, or <extension-id>-<target>.zip for one entrypoint).
        #[arg(long)]
        name: Option<String>,
        /// Do not run `rustup target add` for missing Rust targets.
        #[arg(long)]
        no_install_targets: bool,
    },
    /// List supported build targets.
    Targets,
}

pub fn run() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::New { path, name, id }) => scaffold::new(path, name, id),
        Some(Command::Init { path, name, id }) => scaffold::init(path, name, id),
        Some(Command::Build {
            entrypoint,
            package,
            targets,
            path,
            out_dir,
            name,
            no_install_targets,
        }) => {
            if let Some(target) = entrypoint {
                if !targets.is_empty() {
                    return Err(Error::message(
                        "--entrypoint does not accept extra target arguments",
                    ));
                }
                if name.is_some() {
                    return Err(Error::message("--name is only valid with --package"));
                }
                build::build_entrypoint(&target, Some(&path), !no_install_targets)?;
                Ok(())
            } else if package {
                build::package_targets(&targets, Some(&path), &out_dir, name.as_deref())?;
                Ok(())
            } else {
                Err(Error::message(
                    "build requires --entrypoint <target> or --package",
                ))
            }
        }
        Some(Command::Targets) => {
            for target in target::all() {
                println!(
                    "{:<16} {:<28} .{}",
                    target.canonical, target.rust_triple, target.extension
                );
            }
            Ok(())
        }
        None => {
            let mut command = Cli::command();
            command
                .print_help()
                .map_err(|error| Error::message(error.to_string()))?;
            println!();
            Ok(())
        }
    }
}
