use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning<PathBuf: Clone + PartialEq + Eq> {
    LibraryTomlParseFail {
        path: PathBuf,
        // TODO error
    },
    GroupTomlParseFail {
        path: PathBuf,
        // TODO error
    },
    OrphanedSongs {
        folder: PathBuf,
        files: Vec<PathBuf>,
    },
    /// Cannot import a directory when the TOML file already exists
    GroupTomlAlreadyExists { path: PathBuf },
    /// All the songs in a compilation are in the same album?
    CompilationMayBeAnAlbum { path: PathBuf, common_album: String },
    /// Not all the songs in an album are tagged with the same album?
    AlbumMayBeACompilation {
        path: PathBuf,
        different_albums: HashSet<String>,
    },
}

pub trait WarningSender<PathBuf: Clone + PartialEq + Eq> {
    fn warn(&mut self, w: Warning<PathBuf>);
}

impl<PathBuf: Clone + PartialEq + Eq> WarningSender<PathBuf> for Vec<Warning<PathBuf>> {
    fn warn(&mut self, w: Warning<PathBuf>) {
        self.push(w);
    }
}

pub struct WarningLogger();
impl<PathBuf: Clone + PartialEq + Eq + std::fmt::Debug> WarningSender<PathBuf> for WarningLogger {
    fn warn(&mut self, w: Warning<PathBuf>) {
        match &w {
            Warning::LibraryTomlParseFail { .. }
            | Warning::GroupTomlParseFail { .. }
            | Warning::GroupTomlAlreadyExists { .. }
            // | Warning::CannotInitLibraryToml { ..
                => {
                log::error!("{w:?}")
            }
            Warning::OrphanedSongs { .. } | Warning::CompilationMayBeAnAlbum { .. } | Warning::AlbumMayBeACompilation { .. } => log::warn!("{w:?}"),
        }
    }
}
