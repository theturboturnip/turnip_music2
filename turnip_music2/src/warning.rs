use std::collections::HashSet;

/// Structure for testable warnings.
/// Whenever there are non-fatal messages to surface to the viewer, use this enum and pipe them into a [WarningSender].
/// In testing contexts, a Vec<Warning> can be used as a sender and then tests can compare the warnings to the outcome.
/// At runtime, a different impl can log the warnings and surface them to the user as human-readable messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Warning<PathBuf: Clone + PartialEq + Eq> {
    /// Could not parse a library TOML file
    LibraryTomlParseFail {
        path: PathBuf,
        // TODO error
    },
    /// Could not parse a group TOML file
    GroupTomlParseFail {
        path: PathBuf,
        // TODO error
    },
    /// Found song files in folders that do not have a group TOML
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
