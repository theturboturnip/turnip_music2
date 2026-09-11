use std::{path::Path, str::FromStr};

use id3::TagLike;
use mp4ameta::ChplTimescale;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeMusicExt {
    Mp3,
    Wav,
    Aiff,
    M4a,
    Flac,
    Ogg,
    // TODO m4b support one day? requires general splitting-big-file support.
}
impl NativeMusicExt {
    pub fn to_str(self) -> &'static str {
        self.into()
    }
}
// TODO completeness tests on FromStr and .into() str
impl FromStr for NativeMusicExt {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mp3" => Ok(Self::Mp3),
            "wav" => Ok(Self::Wav),
            "aiff" => Ok(Self::Aiff),
            "m4a" => Ok(Self::M4a),
            "flac" => Ok(Self::Flac),
            "ogg" => Ok(Self::Ogg),
            _ => Err(()),
        }
    }
}
impl From<NativeMusicExt> for &'static str {
    fn from(value: NativeMusicExt) -> Self {
        match value {
            NativeMusicExt::Mp3 => "mp3",
            NativeMusicExt::Ogg => "ogg",
            NativeMusicExt::Flac => "flag",
            NativeMusicExt::Wav => "wav",
            NativeMusicExt::Aiff => "aiff",
            NativeMusicExt::M4a => "m4a",
        }
    }
}
impl From<NativeMusicExt> for NativeMetadataFormat {
    fn from(value: NativeMusicExt) -> Self {
        match value {
            NativeMusicExt::Mp3 | NativeMusicExt::Wav | NativeMusicExt::Aiff => {
                NativeMetadataFormat::Id3
            }
            // TODO: the Flac format I'm using is Ogg Vorbis... surely Ogg is also capable
            NativeMusicExt::Ogg => NativeMetadataFormat::None,
            NativeMusicExt::Flac => NativeMetadataFormat::Flac,
            NativeMusicExt::M4a => NativeMetadataFormat::M4a,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum NativeMetadataFormat {
    #[default]
    None,
    /// - <https://id3.org/id3v2.3.0>
    /// - <https://web.archive.org/web/20260330001711/https://id3.org/id3v2.3.0>
    Id3,
    /// [mp4ameta]
    M4a,
    /// FLAC uses Vorbis comments
    /// - <https://datatracker.ietf.org/doc/rfc9639/>
    /// - <https://xiph.org/vorbis/doc/v-comment.html>
    ///
    /// Reddit recommends these standard tags:
    /// - <https://taglib.org/api/p_propertymapping.html>
    Flac,
}

pub const NATIVE_MUSIC_EXTS: [&'static str; 6] = ["mp3", "ogg", "flac", "wav", "aiff", "m4a"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NativeMetadata {
    pub fmt: NativeMetadataFormat,
    pub title: Option<String>,
    pub album: Option<String>,
    pub album_artists: Vec<String>,
    pub artists: Vec<String>,
    pub num_discs: Option<u64>,
    pub disc: Option<u64>,
    pub num_tracks: Option<u64>,
    pub track: Option<u64>,
    pub genres: Vec<String>,
}

impl Default for NativeMetadata {
    fn default() -> Self {
        Self {
            fmt: NativeMetadataFormat::None,
            title: Default::default(),
            album: Default::default(),
            album_artists: Default::default(),
            artists: Default::default(),
            num_discs: Default::default(),
            disc: Default::default(),
            num_tracks: Default::default(),
            track: Default::default(),
            genres: Default::default(),
        }
    }
}

impl NativeMetadataFormat {
    pub fn parse_from_file(
        path: &Path,
    ) -> anyhow::Result<(Option<NativeMusicExt>, NativeMetadata)> {
        // TODO more robust detection could use e.g. Symphonia
        let ext: Option<NativeMusicExt> = path
            .extension()
            .map(|e| e.to_str())
            .flatten()
            .map(|e| e.parse().ok())
            .flatten();
        let fmt = ext.map(|e| e.into()).unwrap_or(NativeMetadataFormat::None);

        let meta = match fmt {
            NativeMetadataFormat::None => NativeMetadata::default(),
            NativeMetadataFormat::Id3 => {
                let tag = id3::Tag::read_from_path(&path)?;
                NativeMetadata {
                    fmt,
                    title: tag.title().map(str::to_string),
                    album: tag.album().map(str::to_string),
                    album_artists: match tag.album_artist() {
                        Some(s) => vec![s.to_string()],
                        None => vec![],
                    },
                    artists: tag
                        .artists()
                        .map(|v| v.into_iter().map(|s| s.to_string()).collect())
                        .unwrap_or_default(),
                    num_discs: tag.total_discs().map(Into::into),
                    disc: tag.disc().map(Into::into),
                    num_tracks: tag.total_tracks().map(Into::into),
                    track: tag.track().map(Into::into),
                    genres: tag
                        .genres_parsed()
                        .into_iter()
                        .map(|s| s.to_string())
                        .collect(),
                }
            }
            NativeMetadataFormat::M4a => {
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
                )?;
                NativeMetadata {
                    fmt,
                    title: tag.take_title(),
                    // TODO take_title_sort_order
                    album: tag.take_album(),
                    // TODO take_album_sort_order
                    album_artists: tag.take_album_artists().collect::<Vec<_>>(),
                    // TODO take album_artists_sort_orders
                    artists: tag.take_artists().collect::<Vec<_>>(),
                    // TODO take artists_sort_orders
                    num_discs: tag.disc().1.map(Into::into),
                    disc: tag.disc().0.map(Into::into),
                    num_tracks: tag.track().1.map(Into::into),
                    track: tag.track().0.map(Into::into),
                    genres: tag.genres().map(str::to_string).collect(),
                }
            }
            NativeMetadataFormat::Flac => {
                let tag = metaflac::Tag::read_from_path(&path)?;

                // <https://datatracker.ietf.org/doc/rfc9639/>
                // <https://xiph.org/vorbis/doc/v-comment.html>
                // <https://taglib.org/api/p_propertymapping.html>
                // TODO include musicbrainz tags?
                // e.g.
                // Title            Dance!
                // Artist           ATLUS
                // Album            PERSONA4 DANCING ALL NIGHT Original Soundtrack Disc3
                // TrackNumber      1/17
                let title = tag
                    .get_vorbis("title")
                    .map(|iter| iter.last().map(str::to_owned))
                    .flatten();
                // TODO include Version? or keep that separate
                let album = tag
                    .get_vorbis("album")
                    .map(|iter| iter.last().map(str::to_owned))
                    .flatten();
                let album_artists = tag
                    .get_vorbis("albumartist")
                    .into_iter()
                    .flat_map(|iter| iter.map(str::to_owned))
                    .collect::<Vec<_>>();
                let artists = tag
                    .get_vorbis("artist")
                    .into_iter()
                    .flat_map(|iter| iter.map(str::to_owned))
                    .collect::<Vec<_>>();
                let genres = tag
                    .get_vorbis("genre")
                    .into_iter()
                    .flat_map(|iter| iter.map(str::to_owned))
                    .collect::<Vec<_>>();

                let track_disc_num_regex =
                    regex::Regex::new(r"(\d+)(/(\d+))?").expect("regex must never fail");

                let disc_number_str = tag
                    .get_vorbis("discnumber")
                    .map(|iter| iter.last()) // NOT to_owned, don't need that
                    .flatten()
                    .unwrap_or_default();
                let (disc, num_discs) = {
                    match track_disc_num_regex.captures(disc_number_str) {
                        Some(cs) => {
                            let idx = cs
                                .get(1)
                                .expect("can't match regex without first group")
                                .as_str()
                                .parse::<u64>()?;
                            let num = match cs.get(3) {
                                Some(m) => Some(m.as_str().parse::<u64>()?),
                                None => None,
                            };

                            (Some(idx), num)
                        }
                        None => (None, None),
                    }
                };

                let track_number_str = tag
                    .get_vorbis("tracknumber")
                    .map(|iter| iter.last()) // NOT to_owned, don't need that
                    .flatten()
                    .unwrap_or_default();
                let (track, num_tracks) = {
                    match track_disc_num_regex.captures(track_number_str) {
                        Some(cs) => {
                            let idx = cs
                                .get(1)
                                .expect("can't match regex without first group")
                                .as_str()
                                .parse::<u64>()?;
                            let num = match cs.get(3) {
                                Some(m) => Some(m.as_str().parse::<u64>()?),
                                None => None,
                            };

                            (Some(idx), num)
                        }
                        None => (None, None),
                    }
                };

                NativeMetadata {
                    fmt,
                    title,
                    album,
                    album_artists,
                    artists,
                    num_discs,
                    disc,
                    num_tracks,
                    track,
                    genres,
                }
            }
        };
        Ok((ext, meta))
    }
}
