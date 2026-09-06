//! This module defines the schemas for the TOML files
//! - `library.tm2.toml` [ConfigFile], which controls input and export settings.
//! - `music.tm2.toml` [GroupFile], which defines metadata for individual groups of songs.

use crate::data_model::{
    CddbDiscId,
    MbDiscId,
    MbId,
    // metadata::{self, song},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ConfigFileInputs {
    pub search_paths: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    pub library: ConfigFileInputs,
    pub exports: Option<IndexMap<String, ExportConfig>>,
}
impl ConfigFile {
    pub const TOML_FILE_NAME: &'static str = "library.tm2.toml";

    pub fn from_str(s: &str) -> anyhow::Result<(toml_edit::DocumentMut, ConfigFile)> {
        let document = s.parse::<toml_edit::DocumentMut>()?;
        let file = toml_edit::de::from_document(document.clone())?;
        Ok((document, file))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ExportCharset {
    /// No substitutions, UTF-8 encoding.
    /// Case-sensitive.
    #[default]
    Unrestricted,
    /// UTF-8 base encoding.
    /// Substitutes the banned characters `\/.?*¥`:
    /// - `\/` as `|`
    /// - `¥` as `Y`
    /// - `.*?` as `-`
    ///
    /// Outright bans the ASCII control characters 0x00-0x1F.
    /// Case-insensitive. (Yes, on Linux it can be case-sensitive, but there are enough case-insensitive consumers that we should err on the side of caution.)
    ///
    /// <https://learn.microsoft.com/en-us/windows/win32/intl/character-sets-used-in-file-names>
    Ntfs,
    /// Alias for NTFS.
    Fat,
    /// UTF-8 base encoding.
    /// Substitutes the banned characters `"*/:<>?\|`:
    /// - `"` as `'`
    /// - `\/|*?` as '-'
    /// - `:` as `;`
    /// - `<>` as `[]`
    ///
    /// Outright bans the ASCII control characters 0x00-0x1F.
    /// Case-insensitive by specification.
    ///
    /// <https://learn.microsoft.com/en-us/windows/win32/fileio/exfat-specification#table-35-invalid-filename-characters>
    Exfat,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CompilationMode {
    /// Export all constituent songs normally, and generate .m3u8 files at the root of the output that point to them.
    #[default]
    AsM3u8,
    /// For each compilation, export all constituent songs as if `album="{compilation.title}"`, `album_artists=["Compilation"]`, `disc=None`, `num_discs=None`, `num_tracks={compilation.len()}` and `track` is set based on the sorted compilation order.
    /// This overrides metadata and ignores prior values.
    AsAlbum,
    /// Export all constituent songs, but do not export the compilations in anyway.
    Disabled,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Default)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FolderStructure {
    /// `album/song`
    #[default]
    Albums,
    /// `song`
    Song,
    /// `album_artist[0]/album/song`
    AlbumArtistAlbums,
    /// `artist[0]/album/song`
    ArtistAlbums,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ExportConfig {
    /// Target output directory
    pub output_path: String,
    /// Target output directory structure
    pub output_structure: FolderStructure,
    /// zero or more target output formats.
    /// if empty, songs will never be reencoded.
    /// otherwise, if a song is not in any listed format, it will be reencoded as the first format.
    pub target_format: Vec<String>,
    /// ffmpeg parameters used for reencode.
    /// inserted within the command list as `["ffmpeg", "-i", input] + reencode_params + [output]`.
    /// For MP3, try `["-codec:a", libmp3lame", "-qscale:a", "4"]` as suggested in [the ffmpeg documentation](https://trac.ffmpeg.org/wiki/Encode/MP3).
    ///
    /// Either this or target_bitrate should be set. If neither set, ffmpeg defaults will be used.
    pub reencode_params: Option<Vec<String>>,
    /// target bitrate for reencode.
    /// in kilobits per second.
    ///
    /// Either this or reencode_params should be set. reencode_params should be preferred for greater control.
    /// This is effectively identical to `reencode_params=["-b:a", "{target_bitrate}k"]`.
    pub target_bitrate: Option<u64>,
    /// songs will not be reencoded if they are already in a target format AND if their bitrate does not exceed this.
    /// in kilobits per second
    pub max_bitrate: Option<u64>,
    /// Filepaths will be changed to adhere to these requirements.
    pub target_charset: Option<ExportCharset>,
    /// How to export compilations.
    pub compilation_mode: Option<CompilationMode>,
    // TODO
    // pub album_art: AlbumArtMode,
    // pub tag_mode: TagMode, // This may take precedence over compilation_mode if the concepts are merged.
}

/// A set of concrete sources for metadata, controlled by the user, that are never discarded.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Origin {
    /// An arbitrary URL.
    pub url: Option<String>,
    /// MusicBrainz ID for a 'release group' - a logical grouping of different 'releases' of a group of songs, potentially on different 'mediums'
    pub mb_release_group_id: Option<MbId>,
    /// MusicBrainz ID for a 'release' - a specific issuing of a group of songs, potentially on different 'mediums' such as CD, vinyl, etc
    pub mb_release_id: Option<MbId>,
    /// MusicBrainz DiscID for a specific physical CD within a release - does not work with non-CD mediums like vinyl.
    /// May have duplicates e.g. [`lwHl8fGzJyLXQR33ug60E8jhf4k-`](https://musicbrainz.org/cdtoc/lwHl8fGzJyLXQR33ug60E8jhf4k-)
    pub mb_discid: Option<MbDiscId>,
    /// CDDB disc ID of a specific physical CD. Less specific than a MusicBrainz ID and more likely to have duplicates.
    pub cddb_discid: Option<CddbDiscId>,
}

/// Definition of `music.tm2.toml`.
///
/// Tagged enum, so the toplevel of the TOML file should be `type = "album"` or `type = "compilation"`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GroupFile {
    Compilation {
        origin: Origin,
        title: String, // TODO use a tag system or something
        global: CompilationGlobalMeta,
        files: IndexMap<String, CompilationFileMeta>,
    },
    Album {
        origin: Origin,
        album_art: Option<String>,
        global: AlbumGlobalMeta,
        files: IndexMap<String, AlbumFileMeta>,
    },
}
impl GroupFile {
    pub const TOML_FILE_NAME: &'static str = "music.tm2.toml";

    pub fn from_str(s: &str) -> anyhow::Result<(toml_edit::DocumentMut, GroupFile)> {
        let document = s.parse::<toml_edit::DocumentMut>()?;
        let file = toml_edit::de::from_document(document.clone())?;
        Ok((document, file))
    }
}

/// TODO probably shouldn't be Default but it's useful for e.g. testing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct CompilationFileMeta {
    /// Likely to be set
    pub title: String,
    pub artists: Option<Vec<String>>,
    pub genres: Option<Vec<String>>,

    /// Generally album-related, but might still be set
    pub album: Option<String>,
    pub album_artists: Option<Vec<String>>,
    pub num_discs: Option<u64>,
    pub disc: Option<u64>,
    pub num_tracks: Option<u64>,
    pub track: Option<u64>,

    /// Compilation-specific
    pub sort_by_idx: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct CompilationGlobalMeta {
    // Can't be set globally
    // pub title: Option<String>,
    // pub track: Option<u64>,
    // /// Compilation-specific
    // pub idx: Option<u64>,
    // TODO replace with generic ID
    // pub mbid: Option<MbId>,
    /// Likely to be set
    pub artists: Option<Vec<String>>,
    pub genres: Option<Vec<String>>,

    /// Generally album-related, but might still be set
    pub album: Option<String>,
    pub album_artists: Option<Vec<String>>,
    pub num_discs: Option<u64>,
    pub disc: Option<u64>,
    pub num_tracks: Option<u64>,
}

/// TODO probably shouldn't be Default but it's useful for e.g. testing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct AlbumFileMeta {
    // TODO replace with generic ID
    // pub mbid: Option<MbId>,
    /// Likely to be set
    pub title: String,
    pub artists: Option<Vec<String>>,
    pub genres: Option<Vec<String>>,

    /// Generally album-specific
    pub album: Option<String>,
    pub album_artists: Option<Vec<String>>,
    pub num_discs: Option<u64>,
    pub disc: Option<u64>,
    pub num_tracks: Option<u64>,
    pub track: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct AlbumGlobalMeta {
    // Can't be set globally
    // pub title: Option<String>,
    // pub track: Option<u64>,
    // TODO replace with generic ID
    // pub mbid: Option<MbId>,
    /// Likely to be set
    pub artists: Option<Vec<String>>,
    pub genres: Option<Vec<String>>,

    /// Generally album-specific
    pub album: Option<String>,
    pub album_artists: Option<Vec<String>>,
    pub num_discs: Option<u64>,
    pub disc: Option<u64>,
    pub num_tracks: Option<u64>,
}
