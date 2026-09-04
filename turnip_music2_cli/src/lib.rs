use turnip_music2::warning::{Warning, WarningSender};

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
