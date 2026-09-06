use std::{collections::HashSet, ffi::OsStr};

use crate::{
    data_model::{
        native_metadata::NativeMetadata,
        parsed,
        user_defined::{self, CompilationMode::AsM3u8, ConfigFile, ConfigFileInputs, ExportConfig},
    },
    fs::{Fs, FsPathBuf},
    scanner::{Group, scan_dir, scan_library},
    toml::TomlItemExt,
    util::TitleSortKey,
    warning::{Warning, WarningSender},
};
use anyhow::{anyhow, bail};
use indexmap::IndexMap;

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

#[derive(Debug, Clone)]
pub struct Library<F: Fs> {
    pub config_file: user_defined::ConfigFile,
    pub group_files: Vec<Group<F>>,
}

pub struct CliContext<'a, F: Fs, W: WarningSender<F::PathBuf>> {
    config_path: F::PathBuf,
    library_dir: F::PathBuf,
    fs: &'a mut F,
    warner: &'a mut W,

    pub loaded_library: Option<Library<F>>,
}
impl<'a, F: Fs, W: WarningSender<F::PathBuf>> CliContext<'a, F, W> {
    pub fn new(library: Option<String>, fs: &'a mut F, warner: &'a mut W) -> Self {
        let config_path = match library {
            Some(p) => {
                let p = F::PathBuf::parse_path_from_user_str(&p);
                if fs.is_dir(&p) || fs.path_ext(p.as_ref()).is_none() {
                    p.joined(ConfigFile::TOML_FILE_NAME)
                } else {
                    p
                }
            }
            None => F::PathBuf::parse_path_from_user_str(ConfigFile::TOML_FILE_NAME),
        };
        let library_dir = fs
            .path_parent_dir(config_path.as_ref())
            .expect("library_path is a path-to-file, must always have a parent directory");

        Self {
            config_path,
            library_dir,
            fs,
            warner,

            loaded_library: None,
        }
    }

    /// Reload the library config file and rescan from that
    pub fn reload_library(&mut self) -> anyhow::Result<()> {
        let (_library_doc, library_file) = self.fs.parse_config_file(self.config_path.as_ref())?;
        self.loaded_library = Some(Library {
            config_file: library_file,
            group_files: vec![],
        });
        self.rescan_library()
    }

    /// Rescan paths for the existing library file
    pub fn rescan_library(&mut self) -> anyhow::Result<()> {
        match self.loaded_library.as_mut() {
            Some(l) => {
                let mut groups = vec![];
                for s in l.config_file.library.search_paths.iter() {
                    groups.extend(scan_library(
                        self.fs,
                        self.warner,
                        self.library_dir.clone().joined(s.as_ref()),
                    )?);
                }
                groups.sort_by_cached_key(|g| g.toml_path.clone());
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
        if self.fs.is_file(&self.config_path) {
            anyhow::bail!("File already exists")
        }

        let config = ConfigFile {
            library: ConfigFileInputs {
                search_paths: search_paths.clone(),
            },
            exports: if generate_basic_exports {
                Some(indexmap::indexmap! {
                    "mp3".to_string() => ExportConfig {
                        output_path:"output".to_string(),
                        target_format:vec!["mp3".to_string()],
                        reencode_params:None,
                        target_bitrate:Some(128),
                        max_bitrate:Some(320),
                        output_structure: user_defined::FolderStructure::Albums,
                        target_charset: Some(user_defined::ExportCharset::Ntfs),
                        compilation_mode: Some(user_defined::CompilationMode::AsM3u8)
                    }
                })
            } else {
                None
            },
        };
        let mut config_doc = toml_edit::ser::to_document(&config)?;
        config_doc.get_mut("library").unwrap().make_table_regular();
        // TODO apply custom formatting
        self.fs.write_toml_file(&self.config_path, config_doc)?;

        // Scan library to alert the user where unhandled songs are
        for s in search_paths {
            scan_library(self.fs, self.warner, self.library_dir.clone().joined(&s))?;
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
            let path = F::PathBuf::parse_path_from_user_str(f);
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
                    true
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
            fn pull_empty_to_none<T>(v: Option<Vec<T>>) -> Option<Vec<T>> {
                match v {
                    Some(v) if v.is_empty() => None,
                    _ => v,
                }
            }

            let origin = user_defined::Origin::default();

            let group_file = match mode {
                ImportMode::Album => {
                    // Sort by disc_idx, track_idx, and otherwise by a parsed form of the title
                    relevant_songs.sort_by_cached_key(|(title, meta)| {
                        (meta.disc, meta.track, TitleSortKey::parse_from(title))
                    });

                    // Get globals

                    // If everyone has exactly the same album, that's to be expected.
                    // If not everyone has the same album, raise a warning
                    let common_album = where_all_equal(&relevant_songs, |(_, m)| &m.album);
                    if common_album.is_none() {
                        self.warner.warn(Warning::AlbumMayBeACompilation {
                            path: path.clone(),
                            different_albums: relevant_songs
                                .iter()
                                .filter_map(|(_, s)| s.album.clone())
                                .collect(),
                        });
                    }

                    let global = user_defined::AlbumGlobalMeta {
                        // For lists, if they are all the same and they are all empty list then treat them as None instead of Some(vec![])
                        artists: pull_empty_to_none(where_all_equal(&relevant_songs, |(_, m)| {
                            &m.artists
                        })),
                        genres: pull_empty_to_none(where_all_equal(&relevant_songs, |(_, m)| {
                            &m.genres
                        })),
                        album: common_album.flatten(),
                        album_artists: pull_empty_to_none(where_all_equal(
                            &relevant_songs,
                            |(_, m)| &m.album_artists,
                        )),
                        num_discs: where_all_equal(&relevant_songs, |(_, m)| &m.num_discs)
                            .flatten(),
                        disc: where_all_equal(&relevant_songs, |(_, m)| &m.disc).flatten(),
                        num_tracks: where_all_equal(&relevant_songs, |(_, m)| &m.num_tracks)
                            .flatten(),
                    };

                    // Pull out metadata, defaulting the track names to the filenames but otherwise keeping things normal
                    let mut files = relevant_songs
                        .into_iter()
                        .map(|(filename, meta)| {
                            let meta = user_defined::AlbumFileMeta {
                                title: meta.title.unwrap_or_else(|| {
                                    let title = filename
                                        .rsplit_once('.')
                                        .map(|(name, _ext)| name)
                                        .unwrap_or(&filename);
                                    title.to_string()
                                }),
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
                                    meta.num_tracks,
                                ),
                                track: meta.track,
                            };
                            (filename, meta)
                        })
                        .collect::<IndexMap<String, _>>();

                    // Check: if the track and disc numbers will be auto-detected correctly, don't include them in the text.
                    let mut prev_disc = None;
                    let mut prev_track = None;
                    let mut first_item = true;
                    for (_title, meta) in files.iter_mut() {
                        let curr_disc = meta.disc;
                        let curr_track = meta.track;

                        // If the disc is the same as before, elide it.
                        // This works for both-None and both-Some.
                        if curr_disc == prev_disc {
                            meta.disc = None;

                            // If the track is an increment from a non-None previous track, elide it.
                            // Never elide the track information if the disc changed.

                            if let Some(prev_track) = prev_track
                                && let Some(curr_track) = curr_track
                                && curr_track == prev_track + 1
                            {
                                meta.track = None;
                            } else if first_item && curr_track == Some(1) {
                                // Also, if this is the first item and the track == 1, elide it - this will be assumed.
                                meta.track = None;
                            }
                        }

                        prev_disc = curr_disc;
                        prev_track = curr_track;
                        first_item = false;
                    }

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
                    let title = self.fs.path_trailing(path.as_ref()).ok_or_else(|| {
                        anyhow!(
                            "Compilation directory '{:?}' has no trailing path and thus no title",
                            path
                        )
                    })?;
                    let title = title.to_str().ok_or_else(|| {
                        anyhow!("Compilation directory '{:?}' was not valid Unicode", title)
                    })?;
                    let title = title.to_string();

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
                        .map(|(title, meta)| {
                            let meta = user_defined::CompilationFileMeta {
                                title: meta.title.unwrap_or_else(|| title.clone()),
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
                                    meta.num_tracks,
                                ),
                                track: meta.track,
                                sort_by_idx: None,
                            };
                            (title, meta)
                        })
                        .collect::<IndexMap<String, _>>();

                    // TODO art handling?

                    user_defined::GroupFile::Compilation {
                        origin,
                        title,
                        global,
                        files,
                    }
                }
            };
            let mut doc = toml_edit::ser::to_document(&group_file)?;
            // Correct formatting - toml_edit LOVES inline tables
            doc.get_mut("origin").unwrap().make_table_regular();
            doc.get_mut("global").unwrap().make_table_regular();
            // files table
            let files = doc.get_mut("files").unwrap();
            files.make_table_regular();
            files.as_table_mut().unwrap().set_implicit(true);
            for f in files.as_table_mut().unwrap().iter_mut() {
                f.1.make_table_regular();
            }

            self.fs
                .write_toml_file(path.joined(user_defined::GroupFile::TOML_FILE_NAME), doc)?
        }

        // TODO rescan

        Ok(())
    }

    pub fn export(&self, config: &str) -> anyhow::Result<ExportContext<F>> {
        if self.loaded_library.is_none() {
            // TODO warning
            bail!("No loaded library")
        }
        let library = self.loaded_library.as_ref().unwrap();
        if library.config_file.exports.is_none() {
            bail!("Loaded library has no export parameters - edit TOML")
        }
        let config = library
            .config_file
            .exports
            .as_ref()
            .unwrap()
            .get(config)
            .ok_or_else(|| {
                anyhow!(
                    "Export config '{}' doesn't exist in the library config TOML",
                    config
                )
            })?;

        // Gather info on songs
        // TODO GroupFile.files needs to be agnostic... the only diff between album and compilation types is Optional<u64> track.
        // Alternatively, could filter both into NativeMetadata...

        // let output_path = self.library_dir.clone().joined();
        let mut exports = ExportContext::<F>::new(config.clone());

        // TODO need some sort of unification process to figure out exactly how many tracks of an album exist (or more accurately what the maximum value of disc and track is/could be)

        for g in library.group_files.iter() {
            match &g.parsed {
                parsed::GroupFile::Album {
                    origin,
                    album_art,
                    files,
                } => {
                    // Gather exported songs
                    for f in files.iter() {
                        exports.add_song(f.0.as_ref(), f.1.clone().into(), None);
                    }
                }
                parsed::GroupFile::Compilation {
                    origin,
                    title: compilation_title,
                    files,
                } => {
                    // Gather exported songs
                    match config.compilation_mode.unwrap_or_default() {
                        user_defined::CompilationMode::AsAlbum => {
                            let album = compilation_title.to_string();
                            let album_artists = vec!["Compilation".to_string()];

                            // Check the length fits into u64. This is literally never going to not happen.
                            let _len_u64: u64 = files.len().try_into().map_err(|_e| anyhow!("Compilation '{compilation_title}' has more songs than fit into a u64. This will never happen."))?;

                            for (track, f) in files.iter().enumerate() {
                                exports.add_song(f.0.as_ref(), NativeMetadata {
                                        fmt: crate::data_model::native_metadata::NativeMetadataFormat::None,
                                        title: Some(f.1.title.clone()),
                                        artists: f.1.artists.clone(),
                                        genres: f.1.genres.clone(),
                                        album: Some(album.clone()),
                                        album_artists: album_artists.clone(),
                                        num_discs: None,
                                        disc: None,
                                        num_tracks: Some(files.len() as u64),
                                        track: Some(track as u64),
                                    }, Some(&compilation_title));
                            }
                        }
                        // Export songs as normal
                        user_defined::CompilationMode::AsM3u8
                        | user_defined::CompilationMode::Disabled => {
                            for f in files.iter() {
                                exports.add_song(
                                    f.0.as_ref(),
                                    f.1.clone().into(),
                                    Some(&compilation_title),
                                );
                            }
                        }
                    };
                }
            }
        }

        Ok(exports)
    }
}

pub struct ExportContext<F: Fs> {
    config: ExportConfig,

    /// lib-relative output paths to create
    pub folders_to_make: HashSet<F::PathBuf>,
    /// lib-relative input_path, metadata, lib-relative output_path
    pub song_exports: Vec<(F::PathBuf, NativeMetadata, F::PathBuf)>,
    /// title -> (m3u8_path, lib-relative song_paths)
    pub m3u8_exports: IndexMap<String, (F::PathBuf, Vec<F::PathBuf>)>,
}
impl<F: Fs> ExportContext<F> {
    fn new(config: ExportConfig) -> Self {
        let output_path = F::PathBuf::parse_path_from_user_str(&config.output_path);
        let mut folders_to_make = HashSet::new();
        folders_to_make.insert(output_path);

        Self {
            config,
            folders_to_make,
            song_exports: vec![],
            m3u8_exports: IndexMap::new(),
        }
    }

    fn add_song(
        &mut self,
        input_file: &F::Path,
        mut metadata: NativeMetadata,
        in_compilation: Option<&str>,
    ) {
        // TODO code for figuring out the target format
        let ext = "mp3";
        metadata.fmt = crate::data_model::native_metadata::NativeMetadataFormat::ID3;

        // TODO integrate track numbering
        let filename = format!("{}.{}", metadata.title.as_ref().unwrap(), ext);
        //     match (metadata.disc, metadata.track) {
        //     (Some(disc), Some(track)) =>
        // }

        let output_dir: &[&str] = match self.config.output_structure {
            user_defined::FolderStructure::Albums => match &metadata.album {
                Some(album) => &[album],
                None => &[],
            },
            user_defined::FolderStructure::Song => &[],
            user_defined::FolderStructure::AlbumArtistAlbums => {
                match (metadata.album_artists.as_slice(), &metadata.album) {
                    ([artist, ..], Some(album)) => &[artist, album],
                    ([], Some(album)) => &[album],
                    _ => &[],
                }
            }
            user_defined::FolderStructure::ArtistAlbums => {
                match (metadata.artists.as_slice(), &metadata.album) {
                    ([artist, ..], Some(album)) => &[artist, album],
                    ([], Some(album)) => &[album],
                    _ => &[],
                }
            }
        };

        // TODO handle charsets and deduplication for directory
        let output_dir = F::PathBuf::build(output_dir.iter());
        self.folders_to_make.insert(output_dir.clone());

        // TODO handle charsets and deduplication for filename

        let output_file = output_dir.joined(&filename);

        if let Some(compilation_title) = in_compilation
            && self.config.compilation_mode.unwrap_or_default() == AsM3u8
        {
            match self.m3u8_exports.get_mut(compilation_title) {
                Some((_existing_path, songs)) => songs.push(output_file.clone()),
                None => {
                    self.m3u8_exports.insert(
                        compilation_title.to_string(),
                        (
                            F::PathBuf::build([format!("{}.m3u8", compilation_title)].iter()),
                            vec![output_file.clone()],
                        ),
                    );
                }
            }
        }

        self.song_exports
            .push((input_file.to_owned(), metadata, output_file));
    }
}
