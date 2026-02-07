use std::path::Path;

use id3::TagLike;
use mp4ameta::ChplTimescale;

pub enum NativeMetadataFormat {
    None,
    ID3,
    M4A,
    FLAC,
}

pub const NATIVE_MUSIC_EXTS: [&'static str; 6] = [
    "mp3", "ogg", "flac", "wav", "aiff",
    "m4a",
    // TODO m4b support one day? requires general splitting-big-file support.
];

pub struct NativeMetadata {
    pub fmt: NativeMetadataFormat,
    pub name: Option<String>,
    pub album: Option<String>,
    pub album_artists: Vec<String>,
    pub artist: Vec<String>,
    pub num_discs: Option<u64>,
    pub disc_idx: Option<u64>,
    pub num_tracks: Option<u64>,
    pub track_idx: Option<u64>,
}

impl Default for NativeMetadata {
    fn default() -> Self {
        Self {
            fmt: NativeMetadataFormat::None,
            name: Default::default(),
            album: Default::default(),
            album_artists: Default::default(),
            artist: Default::default(),
            num_discs: Default::default(),
            disc_idx: Default::default(),
            num_tracks: Default::default(),
            track_idx: Default::default(),
        }
    }
}

impl NativeMetadataFormat {
    pub fn parse_from_file(path: &Path) -> Result<NativeMetadata, String> {
        // TODO more robust detection could use e.g. Symphonia

        let fmt = {
            let ext = path.extension();
            match ext {
                Some(s)
                    if s.eq_ignore_ascii_case("mp3")
                        || s.eq_ignore_ascii_case("wav")
                        || s.eq_ignore_ascii_case("aiff") =>
                {
                    NativeMetadataFormat::ID3
                }
                Some(s) if s.eq_ignore_ascii_case("flac") => NativeMetadataFormat::FLAC,
                Some(s) if s.eq_ignore_ascii_case("m4a") => NativeMetadataFormat::M4A,
                _ => NativeMetadataFormat::None,
            }
        };

        match fmt {
            NativeMetadataFormat::None => Ok(NativeMetadata::default()),
            NativeMetadataFormat::ID3 => {
                let tag = id3::Tag::read_from_path(&path).map_err(|err| err.to_string())?;
                Ok(NativeMetadata {
                    fmt,
                    name: tag.title().map(str::to_owned),
                    album: tag.album().map(str::to_owned),
                    album_artists: match tag.album_artist() {
                        Some(s) => vec![s.to_owned()],
                        None => vec![],
                    },
                    artist: tag
                        .artists()
                        .map(|v| v.into_iter().map(|s| s.to_owned()).collect())
                        .unwrap_or_default(),
                    num_discs: tag.total_discs().map(Into::into),
                    disc_idx: tag.disc().map(Into::into),
                    num_tracks: tag.total_tracks().map(Into::into),
                    track_idx: tag.track().map(Into::into),
                })
            }
            NativeMetadataFormat::M4A => {
                let mut tag = mp4ameta::Tag::read_with_path(
                    &path,
                    &mp4ameta::ReadConfig {
                        read_meta_items: true,
                        read_image_data: false,
                        read_chapter_list: false,
                        read_chapter_track: false,
                        read_audio_info: true,
                        chpl_timescale: ChplTimescale::DEFAULT,
                    },
                )
                .map_err(|err| err.to_string())?;
                Ok(NativeMetadata {
                    fmt,
                    name: tag.take_title(),
                    // TODO take_title_sort_order
                    album: tag.take_album(),
                    // TODO take_album_sort_order
                    album_artists: tag.take_album_artists().collect::<Vec<_>>(),
                    // TODO take album_artists_sort_orders
                    artist: tag.take_artists().collect::<Vec<_>>(),
                    // TODO take artists_sort_orders
                    num_discs: tag.disc().1.map(Into::into),
                    disc_idx: tag.disc().0.map(Into::into),
                    num_tracks: tag.track().1.map(Into::into),
                    track_idx: tag.track().0.map(Into::into),
                })
            }
            NativeMetadataFormat::FLAC => {
                let tag = metaflac::Tag::read_from_path(&path).map_err(|err| err.to_string())?;

                // https://xiph.org/vorbis/doc/v-comment.html
                // TODO include musicbrainz tags?
                // e.g.
                // Title            Dance!
                // Artist           ATLUS
                // Album            PERSONA4 DANCING ALL NIGHT Original Soundtrack Disc3
                // TrackNumber      1/17
                let name = tag
                    .get_vorbis("title")
                    .map(|iter| iter.last().map(str::to_owned))
                    .flatten();
                // TODO include Version? or keep that separate
                let album = tag
                    .get_vorbis("album")
                    .map(|iter| iter.last().map(str::to_owned))
                    .flatten();
                let artist = tag
                    .get_vorbis("artist")
                    .map(|iter| iter.last().map(str::to_owned))
                    .flatten();

                let track_number_str = tag
                    .get_vorbis("artist")
                    .map(|iter| iter.last()) // NOT to_owned, don't need that
                    .flatten()
                    .unwrap_or_default();
                let track_num_regex =
                    regex::Regex::new(r"(\d+)(/(\d+))?").expect("regex must never fail");
                let (track_idx, num_tracks) = {
                    match track_num_regex.captures(track_number_str) {
                        Some(cs) => {
                            let track_idx = cs
                                .get(1)
                                .expect("can't match regex without first group")
                                .as_str()
                                .parse::<u64>()
                                .map_err(|err| err.to_string())?;
                            let track_num = match cs.get(2) {
                                Some(m) => {
                                    Some(m.as_str().parse::<u64>().map_err(|err| err.to_string())?)
                                }
                                None => None,
                            };

                            (Some(track_idx), track_num)
                        }
                        None => (None, None),
                    }
                };

                Ok(NativeMetadata {
                    fmt,
                    name,
                    album,
                    album_artists: vec![],
                    artist: artist.into_iter().collect(),
                    num_discs: None,
                    disc_idx: None,
                    num_tracks: track_idx,
                    track_idx: num_tracks,
                })
            }
        }
    }
}
