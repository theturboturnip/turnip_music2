use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use crate::data_model::{user_defined, AlbumInputGroup, CompilationInputGroup};

mod data_model;

const GROUP_FILE_NAME: &'static str = "music.tm2.toml";
const DEFAULT_MUSIC_EXTS: [&'static str; 6] = [
    "mp3",
    "ogg",
    "flac",
    "wav",
    "aiff",
    "m4a",
    // TODO m4b support one day? requires general splitting-big-file support.
];

// TODO need input file metadata parsing using rust-metaflac for FLAC and mp4ameta for m4as and id3 for others
// see docs for each crate

struct LibraryContext {
    root_path: PathBuf,
    config: user_defined::ConfigFile,

    cache: ,

    album_groups: Vec<AlbumGroup>,
    compilation_groups: Vec<CompilationGroup>,
}

struct AlbumGroup {
    path: PathBuf,
    document: toml_edit::DocumentMut,
    data: AlbumInputGroup,
}

struct CompilationGroup {
    path: PathBuf,
    document: toml_edit::DocumentMut,
    data: CompilationInputGroup,
}

impl LibraryContext {
    fn scan_library(&mut self) -> anyhow::Result<()> {
        let mut scan_stack = vec![self.root_path];
        let group_file_name = OsStr::new(GROUP_FILE_NAME);

        while let Some(dir) = scan_stack.pop() {
            let mut files = vec![];
            let mut dirs = vec![];
            let mut group = None;

            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    dirs.push(path);
                } else if path.is_file() {
                    if path.file_name() == Some(group_file_name) {
                        group = Some(user_defined::GroupFile::from_file(path)?);
                    } else {
                        files.push(path);
                    }
                }
            }

            if let Some(group) = group {
                self.scan_group(group, dirs, files)?;
            } else {
                scan_stack.extend(dirs);
            }
        }

        Ok(())
    }

    fn scan_group(&mut self, group: user_defined::GroupFile, root_dirs: Vec<PathBuf>, root_files: Vec<PathBuf>) -> anyhow::Result<()> {
        let mut scan_stack = root_dirs;
        // TODO have to include path-relative-to-root_dirs
        let mut music_files: Vec<PathBuf> = vec![];
        let scan_exts: HashSet<OsString> = group.scan_filter().map_or_else(
            || DEFAULT_MUSIC_EXTS.iter().map(|s| s.into()).collect(),
            |scan_filter| scan_filter.ext_filters.iter().map(|s| s.into()).collect()
        );

        for path in root_files {
            if let Some(ext) = path.extension() {
                if scan_exts.contains(ext) {
                    music_files.push(path);
                }
            }
        }

        while let Some(dir) = scan_stack.pop() {
            for entry in std::fs::read_dir(&dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.is_dir() {
                    scan_stack.push(path);
                } else if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if scan_exts.contains(ext) {
                            music_files.push(path);
                        }
                    }
                }
            }

        }

        todo!("Actually process these as per data_model documentation. Don't call out to MusicBrainz yet, save that for later");

        Ok(())
    }
}
// Sta
