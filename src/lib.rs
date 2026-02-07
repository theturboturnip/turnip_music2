use std::path::Path;

use async_trait::async_trait;

use crate::data_model::{AlbumInputGroup, metadata};

mod data_model;
pub mod scanner;

// see docs for each crate

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
