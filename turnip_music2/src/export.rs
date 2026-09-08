use std::{
    collections::{HashMap, HashSet},
    ffi::OsString,
};

use anyhow::{anyhow, bail};
use indexmap::IndexMap;

use crate::{
    data_model::{
        native_metadata::{NativeMetadata, NativeMusicExt},
        parsed, user_defined,
    },
    fs::{Fs, FsPathBuf},
    scanner,
    warning::{Warning, WarningSender},
};

pub fn build_export_jobs<
    'a,
    F: Fs,
    W: WarningSender<F::PathBuf>,
    I: Iterator<Item = &'a scanner::Group<F>>,
>(
    fs: &'a F,
    warner: &'a mut W,
    config: user_defined::ExportConfig,
    group_files: I,
) -> anyhow::Result<ExportContext<F>> {
    let mut exports = ExportContext::<F>::new(config.clone());

    let mut to_export = vec![];

    for g in group_files {
        match &g.parsed {
            parsed::GroupFile::Album {
                origin,
                album_art,
                files,
            } => {
                // Gather exported songs
                for (path, ext, meta) in files.iter() {
                    to_export.push((path.as_ref(), *ext, meta.clone().into(), None));
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

                        for (track, (path, ext, meta)) in files.iter().enumerate() {
                            to_export.push((path.as_ref(), *ext, NativeMetadata {
                                    fmt: crate::data_model::native_metadata::NativeMetadataFormat::None,
                                    title: Some(meta.title.clone()),
                                    artists: meta.artists.clone(),
                                    genres: meta.genres.clone(),
                                    album: Some(album.clone()),
                                    album_artists: album_artists.clone(),
                                    num_discs: None,
                                    disc: None,
                                    num_tracks: Some(files.len() as u64),
                                    track: Some(track as u64),
                                }, Some(compilation_title.as_str()),));
                        }
                    }
                    // Export songs as normal
                    user_defined::CompilationMode::AsM3u8
                    | user_defined::CompilationMode::Disabled => {
                        for (path, ext, meta) in files.iter() {
                            to_export.push((
                                path.as_ref(),
                                *ext,
                                meta.clone().into(),
                                Some(compilation_title.as_str()),
                                // self.warner,
                            ));
                        }
                    }
                };
            }
        }
    }

    // Count the number of discs and tracks now that we know how they're exported
    fn digits(x: Option<u64>) -> usize {
        match x {
            None => 0,
            Some(x) if x < 10 => 1,
            Some(x) if x < 100 => 2,
            Some(x) if x < 1000 => 3,
            Some(x) if x < 10000 => 4,
            // If you hit this code you are being silly
            _ => 5,
        }
    }
    let (disc_digits, track_digits) = {
        // TODO COWs?
        let mut disc_digits: HashMap<String, usize> = HashMap::new();
        let mut track_digits: HashMap<String, usize> = HashMap::new();
        for (_path, _ext, f, _comp) in to_export.iter() {
            if let Some(album) = f.album.as_ref() {
                let d = std::cmp::max(digits(f.num_discs), digits(f.disc));
                let t = std::cmp::max(digits(f.num_tracks), digits(f.track));
                disc_digits
                    .entry(album.clone())
                    .and_modify(|curr| *curr = std::cmp::max(*curr, d))
                    .or_insert(d);
                track_digits
                    .entry(album.clone())
                    .and_modify(|curr| *curr = std::cmp::max(*curr, t))
                    .or_insert(t);
            }
        }

        (disc_digits, track_digits)
    };

    for (input_file, ext, metadata, in_compilation) in to_export {
        let numbering = match metadata.album.as_ref() {
            Some(a) => NumberContext::Numbered {
                disc_digits: *disc_digits.get(a).unwrap(),
                track_digits: *track_digits.get(a).unwrap(),
            },
            None => NumberContext::NoNumbering,
        };

        exports.add_song(input_file, ext, metadata, numbering, in_compilation, warner);
    }

    Ok(exports)
}

/// ffmpeg command line args, effectively
/// OsString for passing into
pub struct ExportJob(Vec<OsString>);

#[derive(Debug)]
pub struct ExportContext<F: Fs> {
    config: user_defined::ExportConfig,

    /// lib-relative output paths to create
    pub folders_to_make: HashSet<F::PathBuf>,
    /// lib-relative input_path, inputlib-relative output_path, outputoutput metadata
    pub song_exports: Vec<(
        F::PathBuf,
        NativeMusicExt,
        F::PathBuf,
        NativeMusicExt,
        NativeMetadata,
    )>,
    /// title -> (m3u8_path, lib-relative song_paths)
    pub m3u8_exports: IndexMap<String, (F::PathBuf, Vec<F::PathBuf>)>,
    all_outputs: HashSet<F::PathBuf>,
}
impl<F: Fs> ExportContext<F> {
    fn new(config: user_defined::ExportConfig) -> Self {
        let output_path = F::PathBuf::parse_path_from_user_str(&config.output_path);
        let mut folders_to_make = HashSet::new();
        folders_to_make.insert(output_path);

        Self {
            config,
            folders_to_make,
            song_exports: vec![],
            m3u8_exports: IndexMap::new(),
            all_outputs: HashSet::new(),
        }
    }

    fn check_duplicate_file<W: WarningSender<F::PathBuf>>(
        &mut self,
        path: F::PathBuf,
        warner: &mut W,
    ) {
        let path = if self
            .config
            .target_charset
            .unwrap_or_default()
            .case_insensitive()
        {
            path.map(|s| s.to_ascii_uppercase())
        } else {
            path
        };
        if self.all_outputs.contains(&path) {
            warner.warn(Warning::DuplicateOutputFile { path });
        } else {
            self.all_outputs.insert(path);
        }
    }

    fn add_song<W: WarningSender<F::PathBuf>>(
        &mut self,
        input_file: &F::Path,
        input_ext: NativeMusicExt,
        mut metadata: NativeMetadata,
        numbering: NumberContext,
        in_compilation: Option<&str>,
        warner: &mut W,
    ) {
        // Figure out the target format
        // TODO apply bitrate detection, pack into a Transform{} struct (inputoutputreencode_if_equal) which handles the case of e.g. a too-high bitrate mp3->mp3
        let output_ext = if self.config.target_format.contains(&input_ext) {
            input_ext
        } else {
            self.config.target_format[0]
        };
        metadata.fmt = output_ext.into();

        // Assume that track numbering and extensions will never produce invalid chars in any encoding,
        // so we can sanitize the title only without worrying about the rest.
        let sanitized_title = self
            .config
            .target_charset
            .unwrap_or_default()
            .sanitize(metadata.title.as_ref().unwrap());
        // Apply track numbering
        let filename = match numbering {
            NumberContext::Numbered {
                disc_digits,
                track_digits,
            } if disc_digits > 0 && track_digits > 0 => format!(
                "{:0disc_digits$}{:0track_digits$} - {}.{}",
                metadata.disc.unwrap_or_default(),
                metadata.track.unwrap_or_default(),
                sanitized_title,
                output_ext.to_str(),
            ),
            NumberContext::Numbered { track_digits, .. } if track_digits > 0 => format!(
                "{:0track_digits$} - {}.{}",
                metadata.track.unwrap_or_default(),
                sanitized_title,
                output_ext.to_str(),
            ),
            NumberContext::Numbered { disc_digits, .. } if disc_digits > 0 => format!(
                "{:0disc_digits$} - {}.{}",
                metadata.disc.unwrap_or_default(),
                sanitized_title,
                output_ext.to_str(),
            ),
            _ => format!("{}.{}", sanitized_title, output_ext.to_str(),),
        };

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

        // Build output directory, being mindful of charset
        let output_dir = F::PathBuf::build(
            output_dir
                .iter()
                .map(|s| self.config.target_charset.unwrap_or_default().sanitize(s)),
        );
        // Add it to the list of outputted files
        self.check_duplicate_file(output_dir.clone(), warner);
        self.folders_to_make.insert(output_dir.clone());

        // Handle deduplication for filename
        // Charsets have already been handled before constructing filename
        let output_file = output_dir.joined(&filename);
        self.check_duplicate_file(output_file.clone(), warner);

        if let Some(compilation_title) = in_compilation
            && self.config.compilation_mode.unwrap_or_default()
                == user_defined::CompilationMode::AsM3u8
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

        self.song_exports.push((
            input_file.to_owned(),
            input_ext,
            output_file,
            output_ext,
            metadata,
        ));
    }
}

enum NumberContext {
    NoNumbering,
    Numbered {
        disc_digits: usize,
        track_digits: usize,
    },
}
