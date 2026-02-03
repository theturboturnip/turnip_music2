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
    collections::HashMap,
    path::{Path, PathBuf},
};

use chromaprint::ChromaprintAlgorithm;
use serde::{Deserialize, Serialize};

use crate::data_model::user_defined::{CompilationInputSongOverride, Origin, ScanFilter};

/// MusicBrainz ID <https://musicbrainz.org/doc/MusicBrainz_Identifier>,
/// which can be for one of many different kinds of [entities](https://musicbrainz.org/doc/MusicBrainz_Entity)
#[derive(Serialize, Deserialize, Debug)]
pub struct MbId(String);
/// https://musicbrainz.org/doc/Disc_ID
#[derive(Serialize, Deserialize, Debug)]
pub struct MbDiscId(String);
/// https://en.wikipedia.org/wiki/CDDB#Example_calculation_of_a_CDDB1_(FreeDB)_disc_ID
#[derive(Serialize, Deserialize, Debug)]
pub struct CddbDiscId(String);

/// Song audio fingerprint via chromaprint, which allows lookup via MusicBrainz
pub struct Chromaprint(ChromaprintAlgorithm, Vec<u8>);

/// Data types defining the user-controlled TOML files
pub mod user_defined {
    use crate::data_model::{CddbDiscId, MbDiscId, MbId, metadata};
    use serde::{Deserialize, Serialize};
    use std::path::Path;

    #[derive(Serialize, Deserialize, Debug)]
    pub struct ConfigFile {
        pub search_paths: Vec<String>,
        pub artist_name_overrides: Vec<ConfigArtistNameOverride>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct ConfigArtistNameOverride {
        pub artist_id: MbId,
        pub artist_name: String,
    }

    /// A set of concrete sources for metadata, controlled by the user, that are never discarded.
    #[derive(Serialize, Deserialize, Debug)]
    pub struct Origin {
        pub url: Option<String>,
        pub mb_release_group_id: Option<MbId>,
        pub mb_release_id: Option<MbId>,
        pub mb_discid: Option<MbDiscId>,
        pub cddb_discid: Option<CddbDiscId>,
    }

    /// A filter for the files to actually scan and use ---
    /// in case of icky input directories with different copies of the same music
    #[derive(Serialize, Deserialize, Debug)]
    pub struct ScanFilter {
        /// e.g. \['mp3', 'flac'\]
        pub ext_filters: Vec<String>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(tag = "type")]
    pub enum GroupFile {
        Compilation {
            origin: Origin,
            scan_filter: Option<ScanFilter>,
            title: String,
            songs: Vec<CompilationInputSongOverride>,
        },
        Album {
            origin: Origin,
            scan_filter: Option<ScanFilter>,
            album_art_rel_path: Option<String>,
            override_metadata: Option<metadata::album::Override>,
            songs: Vec<AlbumInputSongOverride>,
        },
    }
    impl GroupFile {
        pub fn from_file(p: &Path) -> anyhow::Result<GroupFile> {
            let document = std::fs::read_to_string(p)?.parse::<toml_edit::DocumentMut>()?;
            let file = toml_edit::de::from_document(document)?;
            Ok(file)
        }

        pub fn scan_filter(&self) -> Option<&ScanFilter> {
            match self {
                GroupFile::Compilation { scan_filter, .. } => scan_filter.as_ref(),
                GroupFile::Album { scan_filter, .. } => scan_filter.as_ref(),
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct CompilationInputSongOverride {
        pub file_rel_path: String,
        pub origin_mbid: Option<MbId>,
        pub override_metadata: Option<metadata::song::Override>,
        pub override_position: Option<usize>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct AlbumInputSongOverride {
        pub file_rel_path: String,
        pub override_metadata: Option<metadata::song::Override>,
        pub override_disc_idx: Option<u64>,
        pub override_track_idx: Option<u64>,
    }
}

/// Data types for metadata, both cached and overridden by users.
pub mod metadata {
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

        #[derive(Serialize, Deserialize, Debug)]
        pub struct Override {
            pub song_title: Option<String>,
            pub song_artists: Option<Vec<String>>,
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

        #[derive(Serialize, Deserialize, Debug)]
        pub struct Override {
            pub album_title: Option<String>,
            pub album_artists: Option<Vec<String>>,
        }

        pub struct Cached {
            pub title: String,
            pub artists: Vec<CachedArtist>,
        }
    }
}

// struct FileId {
//     /// '/' coded path relative to the library config TOML file being read, NOT to the group TOML file.
//     pub path: String,
//     /// Base64 encoded SHA256 digest of the file, used for integrity checks
//     pub hash: String,
// }
type FileId = PathBuf;

pub struct CompilationInputGroup {
    origin: user_defined::Origin,
    scan_filter: Option<user_defined::ScanFilter>,
    title: String,
    song_files: Vec<CompilationInputSong>,
}
impl CompilationInputGroup {
    pub fn new(
        path: &Path,

        origin: Origin,
        scan_filter: Option<ScanFilter>,
        title: String,
        songs: Vec<CompilationInputSongOverride>,

        non_rel_song_paths: Vec<PathBuf>,
    ) -> Self {
        // sort music_files by path alphanumeric descending, this is the first step of the ordering.
        let mut rel_song_paths = non_rel_song_paths
            .into_iter()
            .map(|p| {
                p.strip_prefix(path)
                    .expect("non_rel_song_paths had a path that wasn't prefixed with the parent")
                    .to_owned()
            })
            .collect::<Vec<_>>();
        rel_song_paths.sort();

        // Build a set of song information for all songs scanned
        let mut mapping = HashMap::new();
        for p in rel_song_paths.iter() {
            mapping.insert(
                p.clone(),
                CompilationInputSong {
                    file: p.clone(),
                    origin_mbid: None,
                    override_metadata: None,
                    derived_metadata_src: None,
                    cached_metadata: None,
                },
            );
        }

        if mapping.len() != rel_song_paths.len() {
            panic!("rel_song_paths had duplicates");
        }

        // For each override:
        for s in songs {
            let mut path = PathBuf::new();
            path.push(s.file_rel_path);

            // - apply the reordering if present. we want to apply the reorderings in file order so it makes sense to the user.
            // TODO does this make sense or is it just confusing? it will be stable but if the user asks for "z is 5, y is 4, x is 3" they will/will not get the exact indices they want
            match s.override_position {
                Some(override_pos) => {
                    let existing_pos = rel_song_paths.iter().position(|p| p.as_os_str() == path.as_os_str()).expect("CompilationInputGroup file contained an override for a file that isn't in the compilation");
                    // if we need to, reorder by shifting things up and down.
                    if existing_pos < override_pos {
                        (&mut rel_song_paths[existing_pos..=override_pos]).rotate_left(1);
                    } else if existing_pos > override_pos {
                        (&mut rel_song_paths[override_pos..=existing_pos]).rotate_right(1);
                    }
                }
                None => {}
            };

            // - update the mapping with the override information
            let s_mapping = mapping.get_mut(&path);
            match s_mapping {
                None => panic!(
                    "CompilationInputGroup referred to song {:?} not present",
                    path
                ),
                Some(s_mapping) => {
                    // Merge in the data from the mapping
                    // TODO how to handle partial metadata? Maybe disable merging?
                    if s.origin_mbid.is_some() {
                        s_mapping.origin_mbid = s.origin_mbid;
                    }
                    if s.override_metadata.is_some() {
                        s_mapping.override_metadata = s.override_metadata;
                    }
                }
            }
        }

        // pull the data out of the mapping, ordered by the final ordering of rel_song_paths
        CompilationInputGroup {
            origin,
            scan_filter,
            title,
            song_files: rel_song_paths
                .into_iter()
                .map(|p| {
                    mapping
                        .remove(&p)
                        .expect("Removing from a list that was populated with mapping")
                })
                .collect(),
        }
    }
}

pub struct CompilationInputSong {
    file: FileId,
    origin_mbid: Option<MbId>,
    override_metadata: Option<metadata::song::Override>,

    derived_metadata_src: Option<metadata::song::CompilationDerivedMetadataSource>,
    cached_metadata: Option<metadata::song::Cached>,
}

pub struct AlbumInputGroup {
    origin: user_defined::Origin,
    override_metadata: Option<metadata::album::Override>,
    scan_filter: Option<user_defined::ScanFilter>,
    album_art: FileId,

    song_files: Vec<AlbumInputSong>,

    derived_metadata: Option<metadata::album::DerivedMetadataSource>,
    cached_metadata: Option<(metadata::album::Cached, Vec<metadata::song::Cached>)>,
}
pub struct AlbumInputSong {
    file: FileId,
    override_metadata: Option<metadata::song::Override>,
    disc_idx: u64,
    track_idx: u64,
}
