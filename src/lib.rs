use crate::data_model::native_metadata::NATIVE_MUSIC_EXTS;
use crate::data_model::{
    AlbumInputGroup, CompilationInputGroup, CompilationInputSong, metadata, user_defined,
};
use async_trait::async_trait;
use std::collections::{HashMap, HashSet};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

mod data_model;

const GROUP_FILE_NAME: &'static str = "music.tm2.toml";

// see docs for each crate

pub struct LibraryGatherer {
    root_path: PathBuf,
    config: user_defined::ConfigFile,

    album_groups: Vec<AlbumGroup>,
    compilation_groups: Vec<CompilationGroup>,
}

pub struct LibraryMetadataApplier {
    root_path: PathBuf,
    config: user_defined::ConfigFile,

    album_groups: Vec<AlbumGroup>,
    compilation_groups: Vec<CompilationGroup>,

    deriver: Box<dyn MetadataDeriver>,
    // output_lib: ?,
}

/// TODO all of the things here need to timestamp their derived metadata sources and cached metadatas,
/// or include hashes of the input data, or something
#[async_trait]
pub trait MetadataDeriver {
    /// Retrieve a stored derived-metadata-source for a given Album if one exists
    fn get_derived_album(
        &self,
        album_path: &Path,
    ) -> Option<metadata::album::DerivedMetadataSource> {
        None
    }
    /// Figure out the derived metadata for the Album and its Songs
    /// e.g. take the origin MBID and pass it through, or take the origin CDDB ID and best-effort look up what it is
    async fn try_rederive_album(
        &mut self,
        album: &AlbumInputGroup,
    ) -> Option<metadata::album::DerivedMetadataSource> {
        None
    }
    /// After finding a derived-metadata-source for an album, look up if we have cached metadata for it
    fn get_cached_album(
        &self,
        src: metadata::album::DerivedMetadataSource,
    ) -> Option<metadata::album::Cached> {
        None
    }
    /// Using a derived-metadata-source for an album, re-lookup the metadata
    async fn try_recache_album(
        &mut self,
        src: metadata::album::DerivedMetadataSource,
    ) -> Option<metadata::album::Cached> {
        None
    }

    fn get_derived_compilation_song(
        &self,
        song_path: &Path,
    ) -> Option<metadata::song::CompilationDerivedMetadataSource> {
        None
    }
    async fn try_rederive_compilation_song(
        &mut self,
        song_path: &Path,
    ) -> Option<metadata::song::CompilationDerivedMetadataSource> {
        None
    }
    fn get_cached_compilation_song(
        &self,
        src: metadata::song::CompilationDerivedMetadataSource,
    ) -> Option<metadata::song::Cached> {
        None
    }
    async fn try_recache_compilation_song(
        &self,
        src: metadata::song::CompilationDerivedMetadataSource,
    ) -> Option<metadata::song::Cached> {
        None
    }
}

struct AlbumGroup {
    path: PathBuf,
    // document: toml_edit::DocumentMut,
    data: AlbumInputGroup,
}

struct CompilationGroup {
    path: PathBuf,
    // document: toml_edit::DocumentMut,
    data: CompilationInputGroup,
}

impl LibraryGatherer {
    pub fn scan_library(&mut self) -> anyhow::Result<()> {
        let mut scan_stack = vec![self.root_path.clone()];
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
                        group = Some((user_defined::GroupFile::from_file(&path)?, path));
                    } else {
                        files.push(path);
                    }
                }
            }

            if let Some((group, path)) = group {
                self.scan_group(path, group, dirs, files)?;
            } else {
                scan_stack.extend(dirs);
            }
        }

        Ok(())
    }

    fn scan_group(
        &mut self,
        root_path: PathBuf,
        group: user_defined::GroupFile,
        root_dirs: Vec<PathBuf>,
        root_files: Vec<PathBuf>,
    ) -> anyhow::Result<()> {
        let mut scan_stack = root_dirs;
        // TODO have to include path-relative-to-root_dirs
        let mut music_files: Vec<PathBuf> = vec![];
        let scan_exts: HashSet<OsString> = group.scan_filter().map_or_else(
            || NATIVE_MUSIC_EXTS.iter().map(|s| s.into()).collect(),
            |scan_filter| scan_filter.ext_filters.iter().map(|s| s.into()).collect(),
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

        // Build up the grups

        match group {
            user_defined::GroupFile::Compilation {
                origin,
                scan_filter,
                title,
                songs,
            } => {
                self.compilation_groups.push(CompilationGroup {
                    data: CompilationInputGroup::new(
                        &root_path,
                        origin,
                        scan_filter,
                        title,
                        songs,
                        music_files,
                    ),
                    path: root_path,
                });
            }
            user_defined::GroupFile::Album {
                origin,
                scan_filter,
                album_art_rel_path,
                override_metadata,
                songs,
            } => {
                self.album_groups.push(AlbumGroup {
                    data: AlbumInputGroup::new(
                        &root_path,
                        origin,
                        override_metadata,
                        scan_filter,
                        album_art_rel_path,
                        songs,
                        music_files,
                    ),
                    path: root_path,
                });
            }
        }

        Ok(())
    }
}
