use crate::data_model::{
    CddbDiscId,
    MbDiscId,
    MbId,
    // metadata::{self, song},
};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::Path};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ConfigFileInputs {
    pub search_paths: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ConfigFile {
    pub library: ConfigFileInputs,
    pub exports: Option<IndexMap<String, ExportParams>>,
}
impl ConfigFile {
    pub const TOML_FILE_NAME: &'static str = "library.tm2.toml";

    pub fn from_str(s: &str) -> anyhow::Result<(toml_edit::DocumentMut, ConfigFile)> {
        let document = s.parse::<toml_edit::DocumentMut>()?;
        let file = toml_edit::de::from_document(document.clone())?;
        Ok((document, file))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ExportParams {
    /// Target output directory
    pub output_path: String,
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
    // TODO
    // pub target_charset: ExportCharset::NTFS,
    // pub album_art: AlbumArtMode,
}

// #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
// pub struct ConfigArtistNameOverride {
//     pub artist_id: MbId,
//     pub artist_name: String,
// }

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

// /// A filter for the files to actually scan and use ---
// /// in case of icky input directories with different copies of the same music
// #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
// pub struct ScanFilter {
//     /// e.g. \['mp3', 'flac'\]
//     pub ext_filters: Vec<String>,
// }

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "type")]
pub enum GroupFile {
    Compilation {
        origin: Origin,
        // scan_filter: Option<ScanFilter>,
        title: String, // TODO use a tag system or something
        global: CompilationGlobalMeta,
        files: IndexMap<String, CompilationFileMeta>,
    },
    Album {
        origin: Origin,
        // scan_filter: Option<ScanFilter>,
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

    // pub fn scan_filter(&self) -> Option<&ScanFilter> {
    //     match self {
    //         GroupFile::Compilation { scan_filter, .. } => scan_filter.as_ref(),
    //         GroupFile::Album { scan_filter, .. } => scan_filter.as_ref(),
    //     }
    // }
}

/// TODO probably shouldn't be Default but it's useful for e.g. testing
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct CompilationFileMeta {
    /// Likely to be set
    pub name: String,
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
    // pub name: Option<String>,
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
    pub name: String,
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
    // pub name: Option<String>,
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
