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
//!   and the metadata used when exporting those files.
//!
//!   Compilations are unsorted by default but can opt in to sorting.
//!   Albums are sorted by (disc, track) numbers.
//! - Source Music files, stored inside folders (recursive search) containing Group Metadata files.
//!
//! Loading a library consists of
//! - Gathering all the Groups you can find
//! - Within those Groups, scanning for relevant Songs
//! - Resolving the metadata for each Song
//!     - Using the values in the group config file
//!         - If the Song is inside an Album Group:
//!             - the "source" disc and track indices of each Song are derived from the source file metadata if present, and otherwise
//!           are respectively kept constant and incremented from the previous Song in an alphanumeric sorting by file name within the Group,
//!           starting at (1,1).
//! - Creating a 1:1 mapping of Songs -> output Songs
//!     - if within an Album Group, `<First Artist of Album>/<Album Title>/<Song Title>`
//!     - if within a Compilation Group, `<First Artist of Song>/<Song Title>`
//!     - all path components are deduplicated if necessary with uppercase alpha "ABCDE..." postfixes.
//!     - if any path component contains special characters the output process stops (UTF-8 allowed, but not filesystem-breakers such as NTFS `/\:*"?<>|`)
//!     - TODO: the output restrictions and FFMPEG configs should be encoded as separate TOML files or in the library config TOML
//! - Use FFMPEG to render out output files
//!     - If same extension, don't bother - avoid recompressing MP3->MP3? TODO add config option for that
//!     - If same input file hash as previous (job cache?) and output file exists
//!         - TODO if output file has different hash than expected, also rerender?
//!         - if input and output file hashes change that indicates loss of integrity, if input file is the same assume that's fine?
//!     - Delete output files that aren't supposed to be there.
//! - Create .m3u8 files for the compilations
//!     - Can just delete old ones and remake, no point in doing sensitivity there?

// use chromaprint::ChromaprintAlgorithm;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Song audio fingerprint via chromaprint, which allows lookup via MusicBrainz
pub struct Chromaprint(/* ChromaprintAlgorithm, */ Vec<u8>);

/// Data types defining the user-controlled TOML files
pub mod user_defined;

/// Data types defining the internal model
pub mod parsed;

/// Data types for native file metadata
pub mod native_metadata;
