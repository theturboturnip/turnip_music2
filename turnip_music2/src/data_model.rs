//! turnip_music2 operates on input files to build output libraries.
//! Input files come in a few flavors:
//! - Config files [user_defined::ConfigFile], that aren't associated with specific music files but affect global behaviour.
//!   These are passed in as top-level command line arguments and are usually named `library.tm2.toml`.
//!   Examples of controls are global renamings for artists.
//! - Group Metadata [user_defined::CompilationInputGroup] [user_defined::AlbumInputGroup] , stored in `music.tm2.toml` files in folders containing source music files.
//!   These control the metadata for those source music files, including information on where they came from,
//!   which affects how those files are then transcoded and output.
//!
//!   For example, it holds the `Origin` data on where the group came from (e.g. if it was ripped from a disc, which disc?);
//!   and any media-specific overrides for that metadata.
//!   A separate file (TODO: TOML or SQLite?) also holds:
//!    - a cache of the derived metadata source, found automatically from the Origin;
//!    - a cache of the actual metadata extracted from that source for each song;
//! - Source Music files, stored inside folders (recursive search) containing Group Metadata files.
//!
//! Loading a library consists of
//! - Gathering all the Groups you can find
//! - Within those Groups, scanning for relevant Songs
//! - Searching for any missing metadata
//! - Resolving the metadata for each Song
//!     - Start with the metadata encoded within the source song
//!     - If there is cached metadata from Musicbrainz, override with that
//!         - If the Song is inside an Album Group, the metadata for the Song is derived from that of the Album's MusicBrainz release
//!           *and* the media index/track index of the Song.
//!             - the "source" disc and track indices of each Song are derived from the source file metadata if present, and otherwise
//!           are respectively kept constant and incremented from the previous Song in an alphanumeric sorting by file name within the Group,
//!           starting at (1,1).
//!             - TODO the Album Group should then have the ability to offset the track number or fix the disc number
//!             - The song metadata is then looked up from the given media and the given track.
//!             - If the track number is too large for the given media index, increment the media index and decrement the track number by the length of that media.
//!             - This allows long sequential incrementing track numbers to be automatically split across disks.
//!         - If the Song is inside a Compilation Group, the metadata for the song is derived from the origin MusicBrainz ID if one is present.
//!     - If there is override metadata in the Group Metadata file, override with that
//! - Creating a 1:1 mapping of Songs -> output Songs
//!     - if within an Album Group, `<First Artist of Album>/<Album Name>/<Song Name>`
//!     - if within a Compilation Group, `<First Artist of Song>/<Song Name>`
//!     - all path components are deduplicated if necessary with uppercase alpha "ABCDE..." postfixes.
//!     - if any path component contains special characters the output process stops (UTF-8 allowed, but not filesystem-breakers such as NTFS `/\:*"?<>|`)
//! - Use FFMPEG to render out output files
//!     - If same extension, don't bother - avoid recompressing MP3->MP3? TODO add config option for that
//!     - If same input file hash as previous (job cache?) and output file exists
//!         - TODO if output file has different hash than expected, also rerender?
//!         - if input and output file hashes change that indicates loss of integrity, if input file is the same assume that's fine?
//!     - Delete output files that aren't supposed to be there.
//! - Create .m3u8 files for the compilations
//!     - Can just delete old ones and remake, no point in doing sensitivity there?
//!     - Compilations retain the same track ordering as alphanumeric input file sorting, so ordered compilations can be created if desired but otherwise do not matter.

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

// use chromaprint::ChromaprintAlgorithm;
use serde::{Deserialize, Serialize};

use crate::{
    fs::{Fs, FsPathBuf},
    resolver::OutputGroupKey,
};

/// MusicBrainz ID <https://musicbrainz.org/doc/MusicBrainz_Identifier>,
/// which can be for one of many different kinds of [entities](https://musicbrainz.org/doc/MusicBrainz_Entity)
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MbId(String);
/// https://musicbrainz.org/doc/Disc_ID
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MbDiscId(String);
/// https://en.wikipedia.org/wiki/CDDB#Example_calculation_of_a_CDDB1_(FreeDB)_disc_ID
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub struct CddbDiscId(String);

/// Song audio fingerprint via chromaprint, which allows lookup via MusicBrainz
pub struct Chromaprint(/* ChromaprintAlgorithm, */ Vec<u8>);

/// Data types defining the user-controlled TOML files
pub mod user_defined {
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
    pub struct ConfigFile {
        pub search_paths: Option<Vec<String>>,
        // pub artist_name_overrides: Option<Vec<ConfigArtistNameOverride>>,
    }
    impl ConfigFile {
        pub fn from_str(s: &str) -> anyhow::Result<ConfigFile> {
            let document = s.parse::<toml_edit::DocumentMut>()?;
            let file = toml_edit::de::from_document(document)?;
            Ok(file)
        }
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
        pub const TOML_FILE_NAME: &'static str = "music.toml";

        pub fn from_str(s: &str) -> anyhow::Result<GroupFile> {
            let document = s.parse::<toml_edit::DocumentMut>()?;
            let file = toml_edit::de::from_document(document)?;
            Ok(file)
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
}

/// Data types defining the internal model
pub mod parsed {
    use crate::{
        data_model::user_defined,
        fs::{Fs, FsPathBuf},
    };

    pub enum GroupFile<F: Fs> {
        Compilation {
            origin: user_defined::Origin,
            // scan_filter: Option<ScanFilter>,
            title: String, // TODO use a tag system or something

            /// Pairs of (full path, metadata)
            files: Vec<(F::PathBuf, CompilationFileMeta)>,
        },
        /// Partial album
        Album {
            origin: user_defined::Origin,

            /// Full path to the album art to use (TODO should be per-item?)
            album_art: Option<F::PathBuf>,

            /// Pairs of (full path, metadata)
            files: Vec<(F::PathBuf, AlbumFileMeta)>,
        },
    }

    impl<F: Fs> GroupFile<F> {
        pub fn from_user(fs: &F, root_path: &F::Path, g: user_defined::GroupFile) -> Self {
            match g {
                user_defined::GroupFile::Compilation {
                    origin,
                    title,
                    global,
                    files,
                } => {
                    let mut files = files
                        .into_iter()
                        .map(|(rel_path, file_meta)| {
                            let full_path = root_path
                                .to_owned()
                                .joined(F::PathBuf::parse_path_from_str(&rel_path));

                            let (meta, idx) = CompilationFileMeta::from_user(file_meta, &global);

                            (full_path, meta, idx)
                        })
                        .collect::<Vec<_>>();
                    // Sort.
                    // Send Some(key) to the start, and then order by key within Some(key).
                    // This allows ordering compilations per-track (ascending keys) or simply grouping similar tracks.
                    // All non-sorted keys are sent to the end.
                    files.sort_by(|(_, _, s1), (_, _, s2)| match (s1, s2) {
                        (None, None) => std::cmp::Ordering::Equal,
                        (None, Some(_)) => std::cmp::Ordering::Less,
                        (Some(_), None) => std::cmp::Ordering::Greater,
                        (Some(s1), Some(s2)) => s1.cmp(&s2),
                    });

                    GroupFile::Compilation {
                        origin,
                        title,
                        files: files
                            .into_iter()
                            .map(|(p, m, _)| (p, m))
                            .collect::<Vec<_>>(),
                    }
                }
                user_defined::GroupFile::Album {
                    origin,
                    album_art,
                    global,
                    files,
                } => {
                    let mut disc = None;
                    let mut track = 1;
                    let mut files = files
                        .into_iter()
                        .map(|(rel_path, file_meta)| {
                            let full_path = root_path
                                .to_owned()
                                .joined(F::PathBuf::parse_path_from_str(&rel_path));

                            let meta =
                                AlbumFileMeta::from_user(file_meta, &global, &mut disc, &mut track);

                            (full_path, meta)
                        })
                        .collect::<Vec<_>>();
                    // Sort.
                    // Send album: None to the start, order by (album, track) ascending otherwise
                    files.sort_by(|(_, m1), (_, m2)| {
                        match (m1.disc, m2.disc, m1.track, m2.track) {
                            (None, None, t1, t2) => t1.cmp(&t2),
                            (None, Some(_), _, _) => std::cmp::Ordering::Less,
                            (Some(_), None, _, _) => std::cmp::Ordering::Greater,
                            (Some(a1), Some(a2), t1, t2) => (a1, t1).cmp(&(a2, t2)),
                        }
                    });

                    // pull the data out of the mapping, ordered by the final ordering of rel_song_paths
                    GroupFile::Album {
                        origin,
                        album_art: album_art.as_ref().map(|rel_path| {
                            root_path
                                .to_owned()
                                .joined(F::PathBuf::parse_path_from_str(&rel_path))
                        }),
                        files,
                    }
                }
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct AlbumFileMeta {
        // TODO replace with generic ID
        // pub mbid: Option<MbId>,
        /// Likely to be set
        pub name: String,
        pub artists: Vec<String>,
        pub genres: Vec<String>,

        /// Generally album-specific
        pub album: Option<String>,
        pub album_artists: Vec<String>,
        pub num_discs: Option<u64>,
        pub disc: Option<u64>,
        pub num_tracks: Option<u64>,
        pub track: u64,
    }
    impl AlbumFileMeta {
        /// Derive metadata from the per-file and global user-defined pieces, plus auto-incrementing disc and track counting state.
        /// This implements the following logic:
        /// - if file b has metadata defined directly after file a, it will inherit file a's disc if it doesn't have one defined.
        /// - if file b has metadata defined directly after file a, it will inherit file a's track number plus one if it doesn't have one defined.
        /// The initial disc and track values are None and 1 respectively, as seen a [metadata::GroupFile::from_user]
        pub fn from_user(
            f: super::user_defined::AlbumFileMeta,
            g: &super::user_defined::AlbumGlobalMeta,

            curr_disc: &mut Option<u64>,
            curr_track: &mut u64,
        ) -> Self {
            let mut meta_disc = f.disc.or_else(|| g.disc.clone());
            if meta_disc.is_some() {
                *curr_disc = meta_disc;
            } else {
                meta_disc = *curr_disc;
            }

            let meta_track = match f.track {
                Some(t) => {
                    *curr_track = t;
                    t
                }
                None => {
                    let this_t = *curr_track;
                    *curr_track += 1;
                    this_t
                }
            };

            Self {
                // Always individually set
                name: f.name,

                // Set by interaction with auto-incrementer
                track: meta_track,

                artists: f
                    .artists
                    .or_else(|| g.artists.clone())
                    .unwrap_or_else(Vec::new),
                genres: f
                    .genres
                    .or_else(|| g.genres.clone())
                    .unwrap_or_else(Vec::new),
                album: f.album.or_else(|| g.album.clone()),
                album_artists: f
                    .album_artists
                    .or_else(|| g.album_artists.clone())
                    .unwrap_or_else(Vec::new),
                num_discs: f.num_discs.or_else(|| g.num_discs.clone()),
                disc: meta_disc,
                num_tracks: f.num_tracks.or_else(|| g.num_tracks.clone()),
            }
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct CompilationFileMeta {
        /// Likely to be set
        pub name: String,
        pub artists: Vec<String>,
        pub genres: Vec<String>,

        /// Generally album-related, but might still be set
        pub album: Option<String>,
        pub album_artists: Vec<String>,
        pub num_discs: Option<u64>,
        pub disc: Option<u64>,
        pub num_tracks: Option<u64>,
        pub track: Option<u64>,
    }
    impl CompilationFileMeta {
        /// Returns (file meta, idx)
        pub fn from_user(
            f: super::user_defined::CompilationFileMeta,
            g: &super::user_defined::CompilationGlobalMeta,
        ) -> (Self, Option<u64>) {
            (
                Self {
                    // Always individually set
                    name: f.name,
                    track: f.track,

                    artists: f
                        .artists
                        .or_else(|| g.artists.clone())
                        .unwrap_or_else(Vec::new),
                    genres: f
                        .genres
                        .or_else(|| g.genres.clone())
                        .unwrap_or_else(Vec::new),
                    album: f.album.or_else(|| g.album.clone()),
                    album_artists: f
                        .album_artists
                        .or_else(|| g.album_artists.clone())
                        .unwrap_or_else(Vec::new),
                    num_discs: f.num_discs.or_else(|| g.num_discs.clone()),
                    disc: f.disc.or_else(|| g.disc.clone()),
                    num_tracks: f.num_tracks.or_else(|| g.num_tracks.clone()),
                },
                f.sort_by_idx,
            )
        }
    }
}

/// Data types for metadata, both cached and overridden by users.
/*pub mod metadata {
    use super::*;

    pub struct CachedArtist {
        id: MbId,
        name: String,
    }

    pub mod song {
        use super::CachedArtist;
        use crate::data_model::{Chromaprint, MbId};
        use serde::{Deserialize, Serialize};

        /// Derived by the tool from the Origin and other metadata and cached as an association with each group.
        pub struct CompilationDerivedMetadataSource {
            pub chromaprint: Option<Chromaprint>,
            pub mb_recording_id: Option<MbId>,
        }

        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
        pub struct CompilationOverride {
            pub title: Option<String>,
            pub artists: Option<Vec<String>>,
            pub position: Option<u64>,
        }

        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
        pub struct AlbumOverride {
            pub title: Option<String>,
            pub artists: Option<Vec<String>>,
            pub disc: Option<u64>,
            pub track: Option<u64>,
        }

        pub struct Cached {
            pub song_title: String,
            pub song_artists: Vec<CachedArtist>,
        }

        pub struct Output {
            pub song_title: String,
            pub song_artists: Vec<String>,
        }
    }
    pub mod album {
        use super::CachedArtist;
        use crate::data_model::{Chromaprint, MbId};
        use serde::{Deserialize, Serialize};

        /// Derived by the tool from the Origin and other metadata and cached as an association with each group.
        pub struct DerivedMetadataSource {
            pub mb_release_group_and_release_ids: Option<(MbId, MbId)>,
            pub derived_songs: Vec<SongDerivedMetadataSource>,
        }

        pub struct SongDerivedMetadataSource {
            pub chromaprint: Option<Chromaprint>,
            pub media_track_idxs: Option<(i64, i64)>,
            // pub track_idx: i64,
        }

        #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
        pub struct Override {
            pub album_title: Option<String>,
            pub album_artists: Option<Vec<String>>,
        }

        pub struct Cached {
            pub title: String,
            pub artists: Vec<CachedArtist>,
        }
    }
}*/
// struct FileId {
//     /// '/' coded path relative to the library config TOML file being read, NOT to the group TOML file.
//     pub path: String,
//     /// Base64 encoded SHA256 digest of the file, used for integrity checks
//     pub hash: String,
// }
pub mod native_metadata;
