use std::{collections::HashSet, ffi::OsStr};

use anyhow::anyhow;
use indexmap::IndexMap;
use turnip_music2::{
    data_model::{
        native_metadata::NativeMetadata,
        user_defined::{self, ConfigFile, ExportParams},
    },
    fs::{Fs, FsPathBuf},
    scanner::{Group, scan_dir, scan_library},
    warning::{Warning, WarningSender},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Album,
    Compilation,
}

fn where_all_equal<T, K: PartialEq + Clone, F: FnMut(&T) -> &K>(ts: &[T], mut f: F) -> Option<K> {
    let mut i = ts.into_iter();

    let k = match i.next() {
        Some(t) => f(t),
        None => return None,
    };

    while let Some(t) = i.next() {
        if f(t) != k {
            return None;
        }
    }

    Some(k.clone())
}

pub struct Library<F: Fs> {
    library_file: user_defined::ConfigFile,
    group_files: Vec<Group<F>>,
}

pub struct CliContext<'a, F: Fs, W: WarningSender<F::PathBuf>> {
    library_path: F::PathBuf,
    fs: &'a F,
    warner: &'a mut W,

    loaded_library: Option<Library<F>>,
}
impl<'a, F: Fs, W: WarningSender<F::PathBuf>> CliContext<'a, F, W> {
    pub fn new(library: Option<String>, fs: &'a F, warner: &'a mut W) -> Self {
        let library_path = match library {
            Some(p) => {
                let p = F::PathBuf::parse_path_from_str(&p);
                if fs.is_dir(&p) || fs.path_ext(p.as_ref()).is_none() {
                    p.joined(ConfigFile::TOML_FILE_NAME)
                } else {
                    p
                }
            }
            None => F::PathBuf::parse_path_from_str(ConfigFile::TOML_FILE_NAME),
        };

        Self {
            library_path,
            fs,
            warner,

            loaded_library: None,
        }
    }

    /// Reload the library config file and rescan from that
    pub fn reload_library(&mut self) -> anyhow::Result<()> {
        let (_library_doc, library_file) = self.fs.parse_config_file(self.library_path.as_ref())?;
        self.loaded_library = Some(Library {
            library_file,
            group_files: vec![],
        });
        self.rescan_library()
    }

    /// Rescan paths for the existing library file
    pub fn rescan_library(&mut self) -> anyhow::Result<()> {
        match self.loaded_library.as_mut() {
            Some(l) => {
                let mut groups = vec![];
                for s in l.library_file.search_paths.iter().flatten() {
                    groups.extend(scan_library(
                        self.fs,
                        self.warner,
                        self.library_path.clone().joined(s.as_ref()),
                    )?);
                }
                l.group_files = groups;
            }
            None => anyhow::bail!("No library loaded"),
        }

        Ok(())
    }

    /// Initialize the library config file at self.library_path and scan all folders indicated.
    /// Does NOT load the library into memory yet...
    pub fn init(
        &mut self,
        search_paths: Vec<String>,
        generate_basic_exports: bool,
    ) -> anyhow::Result<()> {
        if self.fs.is_file(&self.library_path) {
            anyhow::bail!("File already exists")
        }

        self.fs.write_config_file(
            &self.library_path,
            ConfigFile {
                search_paths: Some(search_paths.clone()),
                exports: if generate_basic_exports {
                    indexmap::indexmap! {
                        "mp3".to_string() => ExportParams {
                            output_path: "output".to_string(),
                            target_format: vec!["mp3".to_string()],
                            reencode_params: None,
                            target_bitrate: Some(128),
                            max_bitrate: Some(320)
                        }
                    }
                } else {
                    IndexMap::new()
                },
            },
        )?;

        // Scan library to alert the user where unhandled songs are
        for s in search_paths {
            scan_library(self.fs, self.warner, self.library_path.clone().joined(&s))?;
        }

        Ok(())
    }

    /// For each folder with songs inside, generate a group config file with entries for those songs derived from their metadata.
    /// Common metadata (e.g. if all songs have the same album) get pushed up to the global entry
    pub fn import(
        &mut self,
        folders: &[String],
        formats: Option<&[String]>,
        inherit_native_metadata: bool,
        mode: ImportMode,
    ) -> anyhow::Result<()> {
        let formats: Option<HashSet<&OsStr>> =
            formats.map(|formats| HashSet::from_iter(formats.iter().map(OsStr::new)));

        for f in folders {
            let path = F::PathBuf::parse_path_from_str(f);
            let s = scan_dir(self.fs, path.as_ref())?;

            if let Some(path) = s.group_file {
                // TODO recommend the user call an 'update' function instead
                self.warner.warn(Warning::GroupTomlAlreadyExists { path });
                continue;
            }

            let mut relevant_songs = vec![];
            for song in s.all_music_files {
                // TODO refactor this, this is awful
                let should_import = if let Some(formats) = &formats {
                    if let Some(ext) = self.fs.path_ext(song.as_ref()) {
                        formats.contains(ext)
                    } else {
                        false
                    }
                } else {
                    false
                };

                if !should_import {
                    continue;
                }

                let filename = self.fs.path_trailing(song.as_ref()).ok_or_else(|| {
                    anyhow!(
                        "Song file '{:?}' has no trailing path and thus no name",
                        song
                    )
                })?;
                let filename = filename
                    .to_str()
                    .ok_or_else(|| anyhow!("Song file path '{:?}' was not valid Unicode", song))?;

                let meta = if inherit_native_metadata {
                    self.fs.parse_native_metadata(song.as_ref())?
                } else {
                    NativeMetadata::default()
                };
                relevant_songs.push((filename.to_string(), meta))
            }

            fn userify_vec_if_not_global<T>(g: &Option<Vec<T>>, u: Vec<T>) -> Option<Vec<T>> {
                if g.is_some() {
                    None
                } else if u.is_empty() {
                    None
                } else {
                    Some(u)
                }
            }
            fn userify_opt_if_not_global<T>(g: &Option<T>, u: Option<T>) -> Option<T> {
                if g.is_some() { None } else { u }
            }

            let origin = user_defined::Origin::default();

            let group_file = match mode {
                ImportMode::Album => {
                    // Sort by disc_idx, track_idx, and otherwise by the pathname
                    // TODO this clone is sad :((
                    relevant_songs
                        .sort_by_key(|(name, meta)| (meta.disc, meta.track, name.clone()));

                    // Get globals
                    let global = user_defined::AlbumGlobalMeta {
                        artists: where_all_equal(&relevant_songs, |(_, m)| &m.artists),
                        genres: where_all_equal(&relevant_songs, |(_, m)| &m.genres),
                        album: where_all_equal(&relevant_songs, |(_, m)| &m.album).flatten(),
                        album_artists: where_all_equal(&relevant_songs, |(_, m)| &m.album_artists),
                        num_discs: where_all_equal(&relevant_songs, |(_, m)| &m.num_discs)
                            .flatten(),
                        disc: where_all_equal(&relevant_songs, |(_, m)| &m.disc).flatten(),
                        num_tracks: where_all_equal(&relevant_songs, |(_, m)| &m.num_tracks)
                            .flatten(),
                    };

                    if global.album.is_none() {
                        self.warner.warn(Warning::AlbumMayBeACompilation {
                            path: path.clone(),
                            different_albums: relevant_songs
                                .iter()
                                .filter_map(|(_, s)| s.album.clone())
                                .collect(),
                        });
                    }

                    // Pull out metadata, defaulting the track names to the filenames but otherwise keeping things normal
                    let files = relevant_songs
                        .into_iter()
                        .map(|(name, meta)| {
                            let meta = user_defined::AlbumFileMeta {
                                name: meta.name.unwrap_or_else(|| name.clone()),
                                artists: userify_vec_if_not_global(&global.artists, meta.artists),
                                genres: userify_vec_if_not_global(&global.genres, meta.genres),
                                album: userify_opt_if_not_global(&global.album, meta.album),
                                album_artists: userify_vec_if_not_global(
                                    &global.album_artists,
                                    meta.album_artists,
                                ),
                                num_discs: userify_opt_if_not_global(
                                    &global.num_discs,
                                    meta.num_discs,
                                ),
                                disc: userify_opt_if_not_global(&global.disc, meta.disc),
                                num_tracks: userify_opt_if_not_global(
                                    &global.num_tracks,
                                    meta.track,
                                ),
                                track: meta.track,
                            };
                            (name, meta)
                        })
                        .collect::<IndexMap<String, _>>();

                    // TODO album art handling

                    user_defined::GroupFile::Album {
                        origin,
                        album_art: None,
                        global,
                        files,
                    }
                }
                ImportMode::Compilation => {
                    // Don't sort

                    // Get the compilation name
                    let name = self.fs.path_trailing(path.as_ref()).ok_or_else(|| {
                        anyhow!(
                            "Compilation directory '{:?}' has no trailing path and thus no name",
                            path
                        )
                    })?;
                    let name = name.to_str().ok_or_else(|| {
                        anyhow!("Compilation directory '{:?}' was not valid Unicode", name)
                    })?;
                    let title = name.to_string();

                    // Get globals
                    let global = user_defined::CompilationGlobalMeta {
                        artists: where_all_equal(&relevant_songs, |(_, m)| &m.artists),
                        genres: where_all_equal(&relevant_songs, |(_, m)| &m.genres),
                        album: where_all_equal(&relevant_songs, |(_, m)| &m.album).flatten(),
                        album_artists: where_all_equal(&relevant_songs, |(_, m)| &m.album_artists),
                        num_discs: where_all_equal(&relevant_songs, |(_, m)| &m.num_discs)
                            .flatten(),
                        disc: where_all_equal(&relevant_songs, |(_, m)| &m.disc).flatten(),
                        num_tracks: where_all_equal(&relevant_songs, |(_, m)| &m.num_tracks)
                            .flatten(),
                    };

                    if let Some(album) = &global.album {
                        self.warner.warn(Warning::CompilationMayBeAnAlbum {
                            path: path.clone(),
                            common_album: album.clone(),
                        });
                    }

                    // Pull out metadata, defaulting the track names to the filenames but otherwise keeping things normal
                    let files = relevant_songs
                        .into_iter()
                        .map(|(name, meta)| {
                            let meta = user_defined::CompilationFileMeta {
                                name: meta.name.unwrap_or_else(|| name.clone()),
                                artists: userify_vec_if_not_global(&global.artists, meta.artists),
                                genres: userify_vec_if_not_global(&global.genres, meta.genres),
                                album: userify_opt_if_not_global(&global.album, meta.album),
                                album_artists: userify_vec_if_not_global(
                                    &global.album_artists,
                                    meta.album_artists,
                                ),
                                num_discs: userify_opt_if_not_global(
                                    &global.num_discs,
                                    meta.num_discs,
                                ),
                                disc: userify_opt_if_not_global(&global.disc, meta.disc),
                                num_tracks: userify_opt_if_not_global(
                                    &global.num_tracks,
                                    meta.track,
                                ),
                                track: meta.track,
                                sort_by_idx: None,
                            };
                            (name, meta)
                        })
                        .collect::<IndexMap<String, _>>();

                    // TODO album art handling

                    user_defined::GroupFile::Compilation {
                        origin,
                        title,
                        global,
                        files,
                    }
                }
            };
            let doc = toml_edit::ser::to_document(&group_file)?;
            self.fs.write_toml_file(path, doc)?
        }

        Ok(())
    }
}
