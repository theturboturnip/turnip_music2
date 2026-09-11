use turnip_music2::warning::{Warning, WarningSender};

#[derive(Debug)]
pub struct WarningLogger();
impl WarningSender<std::path::PathBuf> for WarningLogger {
    fn warn(&mut self, w: Warning<std::path::PathBuf>) {
        match &w {
            Warning::LibraryTomlParseFail { .. }
            | Warning::GroupTomlParseFail { .. }
            | Warning::GroupTomlAlreadyExists { .. }
            // | Warning::CannotInitLibraryToml { ..
                => {
                log::error!("{w:?}")
            }
            Warning::OrphanedSongs { folder, .. } => {
                log::warn!("Orphaned Songs in {folder:?}")
            }
            Warning::CompilationMayBeAnAlbum { .. } | Warning::AlbumMayBeACompilation { .. } | Warning::DuplicateOutputFile { .. } => log::warn!("{w:?}"),
        }
    }
}
