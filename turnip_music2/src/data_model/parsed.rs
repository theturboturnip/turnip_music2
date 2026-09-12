use crate::{
    data_model::{
        native_metadata::{NativeMetadata, NativeMusicExt},
        user_defined,
    },
    fs::{Fs, FsPathBuf},
};
use anyhow::anyhow;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GroupFile<F: Fs> {
    Compilation {
        origin: user_defined::Origin,
        // scan_filter: Option<ScanFilter>,
        title: String, // TODO use a tag system or something

        /// Pairs of (full path, metadata)
        files: Vec<(F::PathBuf, NativeMusicExt, CompilationFileMeta)>,
    },
    /// Partial album
    Album {
        origin: user_defined::Origin,

        /// Full path to the album art to use (TODO should be per-item?)
        album_art: Option<F::PathBuf>,

        /// Pairs of (full path, metadata)
        files: Vec<(F::PathBuf, NativeMusicExt, AlbumFileMeta)>,
    },
}

impl<F: Fs> GroupFile<F> {
    pub fn from_user(
        fs: &F,
        root_path: &F::Path,
        g: user_defined::GroupFile,
    ) -> anyhow::Result<Self> {
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
                        let full_path = root_path.to_owned().joined(&rel_path);

                        let (meta, idx) = CompilationFileMeta::from_user(file_meta, &global);

                        // TODO this snippet shouldn't be repeated everywhere...
                        let ext = fs
                            .path_ext(full_path.as_ref())
                            .map(|e| e.to_str())
                            .flatten()
                            .map(|e| e.parse().ok())
                            .flatten()
                            .ok_or_else(|| {
                                anyhow!(
                                    "Song file path '{:?}' is not a recognized music file",
                                    full_path
                                )
                            })?;

                        Ok((full_path, ext, meta, idx))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                // Sort.
                // Send Some(key) to the start, and then order by key within Some(key).
                // This allows ordering compilations per-track (ascending keys) or simply grouping similar tracks.
                // All non-sorted keys are sent to the end.
                files.sort_by(|(_, _, _, s1), (_, _, _, s2)| match (s1, s2) {
                    (None, None) => std::cmp::Ordering::Equal,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (Some(s1), Some(s2)) => s1.cmp(&s2),
                });

                Ok(GroupFile::Compilation {
                    origin,
                    title,
                    files: files
                        .into_iter()
                        .map(|(p, ext, m, _)| (p, ext, m))
                        .collect::<Vec<_>>(),
                })
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
                        let full_path = root_path.to_owned().joined(&rel_path);

                        let meta =
                            AlbumFileMeta::from_user(file_meta, &global, &mut disc, &mut track);

                        // TODO this snippet shouldn't be repeated everywhere...
                        let ext: NativeMusicExt = fs
                            .path_ext(full_path.as_ref())
                            .map(|e| e.to_str())
                            .flatten()
                            .map(|e| e.parse().ok())
                            .flatten()
                            .ok_or_else(|| {
                                anyhow!(
                                    "Song file path '{:?}' is not a recognized music file",
                                    full_path
                                )
                            })?;

                        Ok((full_path, ext, meta))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                // Sort.
                // Send album: None to the start, order by (album, track) ascending otherwise
                files.sort_by(|(_, _, m1), (_, _, m2)| {
                    match (m1.disc, m2.disc, m1.track, m2.track) {
                        (None, None, t1, t2) => t1.cmp(&t2),
                        (None, Some(_), _, _) => std::cmp::Ordering::Less,
                        (Some(_), None, _, _) => std::cmp::Ordering::Greater,
                        (Some(a1), Some(a2), t1, t2) => (a1, t1).cmp(&(a2, t2)),
                    }
                });

                // pull the data out of the mapping, ordered by the final ordering of rel_song_paths
                Ok(GroupFile::Album {
                    origin,
                    album_art: album_art
                        .as_ref()
                        .map(|rel_path| root_path.to_owned().joined(&rel_path)),
                    files,
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumFileMeta {
    // TODO replace with generic ID
    // pub mbid: Option<MbId>,
    /// Likely to be set
    pub title: String,
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
            // TODO should this reset track number?
        } else {
            meta_disc = *curr_disc;
        }

        // TODO all of this behaviour should affect or carry through num_tracks, num_discs

        let meta_track = match f.track {
            Some(t) => {
                *curr_track = t;
                t
            }
            None => {
                let this_t = *curr_track + 1;
                *curr_track = this_t;
                this_t
            }
        };

        Self {
            // Always individually set
            title: f.title,

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
impl From<AlbumFileMeta> for NativeMetadata {
    fn from(value: AlbumFileMeta) -> Self {
        Self {
            fmt: super::native_metadata::NativeMetadataFormat::None,
            title: Some(value.title),
            album: value.album,
            album_artists: value.album_artists,
            artists: value.artists,
            num_discs: value.num_discs,
            disc: value.disc,
            num_tracks: value.num_tracks,
            track: Some(value.track),
            genres: value.genres,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilationFileMeta {
    /// Likely to be set
    pub title: String,
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
                title: f.title,
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
impl From<CompilationFileMeta> for NativeMetadata {
    fn from(value: CompilationFileMeta) -> Self {
        Self {
            fmt: super::native_metadata::NativeMetadataFormat::None,
            title: Some(value.title),
            album: value.album,
            album_artists: value.album_artists,
            artists: value.artists,
            num_discs: value.num_discs,
            disc: value.disc,
            num_tracks: value.num_tracks,
            track: value.track,
            genres: value.genres,
        }
    }
}
