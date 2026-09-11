use std::{
    collections::{HashMap, HashSet},
    ffi::{OsStr, OsString},
};

use anyhow::anyhow;
use indexmap::IndexMap;

use crate::{
    data_model::{
        native_metadata::{NativeMetadata, NativeMetadataFormat, NativeMusicExt},
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
            parsed::GroupFile::Album { files, .. } => {
                // Gather exported songs
                for (path, ext, meta) in files.iter() {
                    to_export.push((path.as_ref(), *ext, meta.clone().into(), None));
                }
            }
            parsed::GroupFile::Compilation {
                title: compilation_title,
                files,
                ..
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

/// ffmpeg command line args, effectively.
/// OsString for passing into [subprocess] eventually
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfmpegArgs(pub Vec<OsString>);

#[derive(Debug, Clone)]
pub struct ExportSong<F: Fs> {
    /// lib-relative
    input_path: F::PathBuf,
    input_ext: NativeMusicExt,

    /// output-dir-relative
    output_path: F::PathBuf,
    output_ext: NativeMusicExt,
    output_meta: NativeMetadata,
}

#[derive(Debug, Clone)]
pub struct ExportContext<F: Fs> {
    pub config: user_defined::ExportConfig,

    /// output-dir-relative output paths to create
    pub folders_to_make: HashSet<F::PathBuf>,
    /// lib-relative input_path, inputlib-relative output_path, outputoutput metadata
    pub song_exports: Vec<ExportSong<F>>,
    /// title -> (output-dir-relative m3u8_path, output-dir-relative song_paths)
    pub m3u8_exports: IndexMap<String, (F::PathBuf, Vec<F::PathBuf>)>,
    /// mapping of (charset-normalized path) -> (input paths)
    all_outputs: HashMap<F::PathBuf, Vec<F::PathBuf>>,
}
impl<F: Fs> ExportContext<F> {
    fn new(config: user_defined::ExportConfig) -> Self {
        Self {
            config,
            // Don't need to initialize - could put in [""], but that would be redundant
            folders_to_make: HashSet::new(),
            song_exports: vec![],
            m3u8_exports: IndexMap::new(),
            all_outputs: HashMap::new(),
        }
    }

    fn check_duplicate_file<W: WarningSender<F::PathBuf>>(
        &mut self,
        path: F::PathBuf,
        warner: &mut W,
        is_folder: bool,
    ) {
        let (normalized_path, input_path) = if self
            .config
            .target_charset
            .unwrap_or_default()
            .case_insensitive()
        {
            (path.map(|s| s.to_ascii_uppercase()), path)
        } else {
            (path.clone(), path)
        };
        // TODO test duplicate output warnings
        // For now,
        if let Some(prior_input_paths) = self.all_outputs.get_mut(&normalized_path) {
            let should_warn = if prior_input_paths.contains(&input_path) {
                // Do not warn anyone if this is a folder overlap.
                // These are to be expected, unless this is a new overlap because of charset normalization
                !is_folder
            } else {
                // This is a new overlap because of charset normalization
                prior_input_paths.push(input_path.clone());
                true
            };

            if should_warn {
                // In all other cases
                warner.warn(Warning::DuplicateOutputFile {
                    path: input_path,
                    normalized_path,
                });
            }
        } else {
            self.all_outputs.insert(normalized_path, vec![input_path]);
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
        // TODO this shouldn't just be duplicates... for e.g. album paths they will be duplicated by definition
        self.check_duplicate_file(output_dir.clone(), warner, true);
        self.folders_to_make.insert(output_dir.clone());

        // Handle deduplication for filename
        // Charsets have already been handled before constructing filename
        let output_file = output_dir.joined(&filename);
        self.check_duplicate_file(output_file.clone(), warner, false);

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

        self.song_exports.push(ExportSong {
            input_path: input_file.to_owned(),
            input_ext,
            output_path: output_file,
            output_ext,
            output_meta: metadata,
        });
    }

    /// - input_prefix should be an offset to the library directory.
    /// - output_prefix should be an offset to the output directory.
    pub fn export_song_to_ffmpeg(
        &self,
        input_prefix: &F::PathBuf,
        output_prefix: &F::PathBuf,
        export: &ExportSong<F>,
    ) -> FfmpegArgs {
        let mut args = vec![
            // Pull in the input
            "-i".to_os_string(),
            input_prefix
                .clone()
                .plus(&export.input_path.as_ref())
                .to_os_string(),
            // Clear all metadata
            "-map_metadata".to_os_string(),
            "-1".to_os_string(),
        ];

        if export.input_ext == export.output_ext {
            // No point reencoding
            args.push("-codec:a".to_os_string());
            args.push("copy".to_os_string());
        } else if let Some(params) = self.config.reencode_params.as_ref() {
            // Use the provided args
            args.extend(params.iter().map(|p| p.clone()));
        } else if let Some(bitrate) = self.config.target_bitrate {
            // Just put in bitrate, assume it'll figure out what encoder to use
            args.push("-b:a".to_os_string());
            args.push(format!("{bitrate}k").to_os_string());
        } else {
            // TODO what should I do here?
        }

        // Put in metadata
        let mut push_metadata = |key: &str, value: &str| -> () {
            args.push("-metadata".to_os_string());
            if !value.contains('"') {
                args.push(format!("{key}=\"{value}\"").to_os_string());
            } else {
                let value = value.replace("\"", "\\\"");
                args.push(format!("{key}=\"{value}\"").to_os_string());
            }
        };
        // TODO configurable mode for concatenating artists together or putting multiple tags?
        let output_meta_fmt: NativeMetadataFormat = export.output_ext.into();
        // Each of these blocks configures the generic ffmpeg metadata keys
        // <https://ffmpeg.org/doxygen/7.0/group__metadata__api.html>
        // <https://wiki.multimedia.cx/index.php/FFmpeg_Metadata>
        // based on the respective specifications
        match output_meta_fmt {
            NativeMetadataFormat::None => {
                // Do nothing
            }
            NativeMetadataFormat::Id3 => {
                // <https://id3.org/id3v2.3.0>
                // <https://web.archive.org/web/20260330001711/https://id3.org/id3v2.3.0>

                // title is always present
                push_metadata("title", export.output_meta.title.as_ref().unwrap());

                if let Some(album) = export.output_meta.album.as_ref() {
                    push_metadata("album", album);
                }

                // ID3 doesn't technically have a separate album_artists tag.
                // ffmpeg uses the separate TPE2 tag, so it's supported here.
                // HOWEVER: this only supports ONE album_artist.
                // The spec doesn't mention 'separated with a "/" character'.
                if !export.output_meta.album_artists.is_empty() {
                    push_metadata("album_artist", &export.output_meta.album_artists[0])
                    // TODO warning for multiple album_artists, only one will be honored
                }

                if !export.output_meta.artists.is_empty() {
                    // artists are separated by the '/' character
                    // TODO if any artist has the / character, warn the user
                    push_metadata("artist", &export.output_meta.artists.join("/"))
                }

                // This is ID3v2.3 exclusive, TPOS key
                // > The 'Part of a set' frame is a numeric string that describes which part of a set the audio came from. This frame is used if the source described in the "TALB" frame is divided into several mediums, e.g. a double CD. The value may be extended with a "/" character and a numeric string containing the total number of parts in the set. E.g. "1/2".
                match (export.output_meta.disc, export.output_meta.num_discs) {
                    (Some(disc), Some(n)) => push_metadata("disc", &format!("{disc}/{n}")),
                    (Some(disc), _) => push_metadata("disc", &format!("{disc}")),
                    // This needs to start with 0 - 'may be extended' implies something at the front
                    (_, Some(n)) => push_metadata("disc", &format!("0/{n}")),
                    _ => {}
                }

                // TRCK key
                // > The 'Track number/Position in set' frame is a numeric string containing the order number of the audio-file on its original recording. This may be extended with a "/" character and a numeric string containing the total numer of tracks/elements on the original recording. E.g. "4/9".
                match (export.output_meta.track, export.output_meta.num_tracks) {
                    (Some(track), Some(n)) => push_metadata("track", &format!("{track}/{n}")),
                    (Some(track), _) => push_metadata("track", &format!("{track}")),
                    // This needs to start with 0 - 'may be extended' implies something at the front
                    (_, Some(n)) => push_metadata("track", &format!("0/{n}")),
                    _ => {}
                }

                // TODO numerify genre?
                // > The 'Content type', which previously was stored as a one byte numeric value only, is now a numeric string. You may use one or several of the types as ID3v1.1 did or, since the category list would be impossible to maintain with accurate and up to date categories, define your own.
                if !export.output_meta.genres.is_empty() {
                    push_metadata("genre", &export.output_meta.genres[0]);
                    // TODO warning for multiple genres
                }

                // TODO sort keys for album/artist/title, that's ID3v2.4+ only
            }
            NativeMetadataFormat::M4a => {
                // title is always present
                push_metadata("title", export.output_meta.title.as_ref().unwrap());

                if let Some(album) = export.output_meta.album.as_ref() {
                    push_metadata("album", album);
                }

                // m4a supports multiple instances of the album_artist tag.
                // m4a ffmpeg supports exactly one album_artist.
                if !export.output_meta.album_artists.is_empty() {
                    push_metadata("album_artist", &export.output_meta.album_artists[0])
                    // TODO warning for multiple album_artists, only one will be honored
                }

                // m4a supports multiple instances of the artist tag.
                // m4a ffmpeg supports exactly one artist.
                if !export.output_meta.artists.is_empty() {
                    push_metadata("author", &export.output_meta.artists[0])
                    // TODO warning for multiple artists, only one will be honored
                }

                // These should work
                // https://github.com/FFmpeg/FFmpeg/blob/35b7df64a0146fc0e2effb88151f912dcd80756b/libavformat/movenc.c#L4794
                match (export.output_meta.disc, export.output_meta.num_discs) {
                    (Some(disc), Some(n)) => push_metadata("disc", &format!("{disc}/{n}")),
                    (Some(disc), _) => push_metadata("disc", &format!("{disc}")),
                    // ffmpeg always expects this to start with a number
                    (_, Some(n)) => push_metadata("disc", &format!("0/{n}")),
                    _ => {}
                }
                match (export.output_meta.track, export.output_meta.num_tracks) {
                    (Some(track), Some(n)) => push_metadata("track", &format!("{track}/{n}")),
                    (Some(track), _) => push_metadata("track", &format!("{track}")),
                    // ffmpeg always expects this to start with a number
                    (_, Some(n)) => push_metadata("track", &format!("0/{n}")),
                    _ => {}
                }

                // TODO numerify genre?
                // > The 'Content type', which previously was stored as a one byte numeric value only, is now a numeric string. You may use one or several of the types as ID3v1.1 did or, since the category list would be impossible to maintain with accurate and up to date categories, define your own.
                if !export.output_meta.genres.is_empty() {
                    push_metadata("genre", &export.output_meta.genres[0]);
                    // TODO warning for multiple genres, only one will be honored
                }
            }
            NativeMetadataFormat::Flac => {
                // TODO: test that ffmpeg Flac even supports metadata in the first place
                // TODO: be sad that ffmpeg doesn't support multiple values per metadata key

                // title is always present
                push_metadata("title", export.output_meta.title.as_ref().unwrap());

                if let Some(album) = export.output_meta.album.as_ref() {
                    push_metadata("album", album);
                }

                // m4a supports multiple instances of the album_artist tag.
                // m4a ffmpeg supports exactly one album_artist.
                if !export.output_meta.album_artists.is_empty() {
                    push_metadata("albumartist", &export.output_meta.album_artists[0])
                    // TODO warning for multiple album_artists, only one will be honored
                }

                // m4a supports multiple instances of the artist tag.
                // m4a ffmpeg supports exactly one artist.
                if !export.output_meta.artists.is_empty() {
                    push_metadata("artist", &export.output_meta.artists[0])
                    // TODO warning for multiple artists, only one will be honored
                }

                match (export.output_meta.disc, export.output_meta.num_discs) {
                    (Some(disc), Some(n)) => push_metadata("discnumber", &format!("{disc}/{n}")),
                    (Some(disc), _) => push_metadata("discnumber", &format!("{disc}")),
                    // ffmpeg always expects this to start with a number
                    (_, Some(n)) => push_metadata("discnumber", &format!("0/{n}")),
                    _ => {}
                }
                match (export.output_meta.track, export.output_meta.num_tracks) {
                    (Some(track), Some(n)) => push_metadata("tracknumber", &format!("{track}/{n}")),
                    (Some(track), _) => push_metadata("tracknumber", &format!("{track}")),
                    // ffmpeg always expects this to start with a number
                    (_, Some(n)) => push_metadata("tracknumber", &format!("0/{n}")),
                    _ => {}
                }

                // TODO numerify genre?
                if !export.output_meta.genres.is_empty() {
                    push_metadata("genre", &export.output_meta.genres[0]);
                    // TODO warning for multiple genres, only one will be honored
                }
            }
        }

        // Finally, output.
        // Always overwrite
        args.push("-y".to_os_string());
        args.push(
            output_prefix
                .clone()
                .plus(export.output_path.as_ref())
                .to_os_string(),
        );

        FfmpegArgs(args)
    }
}

// TODO test exported output somehow

enum NumberContext {
    NoNumbering,
    Numbered {
        disc_digits: usize,
        track_digits: usize,
    },
}

pub trait ToOsString {
    fn to_os_string(self) -> OsString;
}
impl<'a> ToOsString for &'a str {
    fn to_os_string(self) -> OsString {
        OsStr::new(self).to_owned()
    }
}
impl<'a> ToOsString for String {
    fn to_os_string(self) -> OsString {
        self.as_str().to_os_string()
    }
}
