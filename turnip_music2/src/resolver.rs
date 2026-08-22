use crate::data_model::MbId;

// TODO take scanner output, group it under these keys once
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OutputGroupKey {
    AlbumByMusicBrainz(MbId),
    AlbumByName(String),
    CompilationByName(String),
}
