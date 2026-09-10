use std::ffi::OsString;

use clap::ArgAction;
use clap::Parser;
use clap::Subcommand;
use turnip_music2::cli::{CliContext, ImportMode};
use turnip_music2::export::ToOsString;
use turnip_music2::fs::StdFs;
use turnip_music2_cli::WarningLogger;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Library file to operate on. Searches for "library.tm2.toml" in current directory if not set. Error if not set and default not found.
    #[arg(short, long, value_name = "TOML")]
    pub library: Option<String>,

    #[arg(long, default_value_t = false, action=ArgAction::SetTrue)]
    pub dry_run: bool,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Creates a library file
    Init {
        search_paths: Vec<String>,
        #[arg(long, default_value_t = true)]
        generate_basic_exports: bool,
    },
    /// Creates an album group file for a given folder with source songs.
    ImportAlbum {
        folders: Vec<String>,
        formats: Option<Vec<String>>,
        #[arg(long, default_value_t = true)]
        native_metadata: bool,
        // TODO integrate album global metadata?
    },
    /// Creates a compilation group file for a given folder with source songs.
    ImportCompilation {
        folders: Vec<String>,
        formats: Option<Vec<String>>,
        #[arg(long, default_value_t = true)]
        native_metadata: bool,
        // TODO integrate compilation global metadata?
    },
    /// Scans all folders with existing group files, if there are new files that adhere to the formats, add them to the TOML
    Update {
        folders: Vec<String>,
        formats: Option<Vec<String>>,
        #[arg(long, default_value_t = true)]
        native_metadata: bool,
    },
    /// Open TUI for updating metadata based on [origin] tag in TOML. Also allows adding to the [origin] tag.
    Edit {},
    /// Run an export job for a given config
    Export {
        config: String,
        /// Defaults to "ffmpeg"
        ffmpeg: Option<OsString>,
    },
}

fn main() {
    pretty_env_logger::init_custom_env("TURNIP_MUSIC_LOG");

    let cli = Cli::parse();

    let mut fs = StdFs {
        dry_run: cli.dry_run,
    };
    let mut warner = WarningLogger {};
    let res = {
        let mut ctx = CliContext::new(cli.library, &mut fs, &mut warner);

        match cli.command {
            Commands::Init {
                search_paths,
                generate_basic_exports,
            } => ctx.init(search_paths, generate_basic_exports),
            Commands::ImportAlbum {
                folders,
                formats,
                native_metadata,
            } => ctx.import(
                &folders,
                formats.as_ref().map(|fs| fs.as_slice()),
                native_metadata,
                ImportMode::Album,
            ),
            Commands::ImportCompilation {
                folders,
                formats,
                native_metadata,
            } => ctx.import(
                &folders,
                formats.as_ref().map(|fs| fs.as_slice()),
                native_metadata,
                ImportMode::Compilation,
            ),
            Commands::Update { .. } => todo!(),
            Commands::Edit { .. } => todo!(),
            // Run internal closure to allow bailing if the first step fails
            Commands::Export { config, ffmpeg } => || -> anyhow::Result<()> {
                let (export_context, ffmpeg_args) = ctx.prep_export(&config)?;

                todo!("Generate output directories");

                let ffmpeg = match ffmpeg {
                    Some(f) => f,
                    None => "ffmpeg".to_os_string(),
                };
                ctx.execute_ffmpegs(&ffmpeg, ffmpeg_args)?;

                todo!("Test all of this");
                Ok(())
            }(),
        }
    };
    match res {
        Ok(_) => {}
        Err(e) => log::error!("{:?}", e),
    }
}
