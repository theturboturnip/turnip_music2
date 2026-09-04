use clap::Parser;
use clap::Subcommand;
use turnip_music2::fs::StdFs;
use turnip_music2_cli::CliContext;
use turnip_music2_cli::ImportMode;
use turnip_music2_cli::WarningLogger;

#[derive(Parser)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Library file to operate on. Searches for "library.tm2.toml" in current directory if not set. Error if not set and default not found.
    #[arg(short, long, value_name = "TOML")]
    pub library: Option<String>,

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
    /// Provides TUI for searching and updating
    Update {},
    Export {},
}

fn main() {
    pretty_env_logger::init_custom_env("TURNIP_MUSIC_LOG");

    let cli = Cli::parse();

    let fs = StdFs {};
    let mut warner = WarningLogger {};
    let res = {
        let mut ctx = CliContext::new(cli.library, &fs, &mut warner);

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
            Commands::Update {} => todo!(),
            Commands::Export {} => todo!(),
        }
    };
    match res {
        Ok(_) => {}
        Err(e) => log::error!("{:?}", e),
    }
}
