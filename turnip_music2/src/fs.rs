use std::{ffi::OsStr, fmt::Debug, hash::Hash};

use crate::data_model::{
    native_metadata::{NativeMetadata, NativeMetadataFormat},
    user_defined::{ConfigFile, GroupFile},
};

pub trait FsPathBuf: Clone + Hash + Debug + PartialEq + Eq + PartialOrd + Ord {
    fn parse_path_from_str(s: &str) -> Self;
}

/// Minimal trait encoding only the necessary components of a filesystem scanner.
pub trait Fs {
    type Path: ?Sized + ToOwned<Owned = Self::PathBuf> + AsRef<Self::Path>; // = std::path::Path;
    type PathBuf: AsRef<Self::Path> + FsPathBuf;

    fn read_dir<'s, P: AsRef<Self::Path>>(
        &'s self,
        path: P,
    ) -> anyhow::Result<impl Iterator<Item = anyhow::Result<Self::PathBuf>>>;
    fn path_trailing<'p>(&self, path: &'p Self::Path) -> Option<&'p OsStr>;
    fn path_ext<'p>(&self, path: &'p Self::Path) -> Option<&'p OsStr>;
    fn is_file<P: AsRef<Self::Path>>(&self, path: P) -> bool;
    fn is_dir<P: AsRef<Self::Path>>(&self, path: P) -> bool;
    fn strip_prefix<'a, P: AsRef<Self::Path>>(
        &self,
        path_buf: &'a Self::PathBuf,
        prefix: P,
    ) -> anyhow::Result<&'a Self::Path>;

    fn parse_native_metadata<P: AsRef<Self::Path>>(
        &self,
        path: P,
    ) -> anyhow::Result<NativeMetadata>;
    fn parse_config_file<P: AsRef<Self::Path>>(&self, path: P) -> anyhow::Result<ConfigFile>;
    fn parse_group_file<P: AsRef<Self::Path>>(&self, path: P) -> anyhow::Result<GroupFile>;
}

pub struct StdFs;
impl FsPathBuf for std::path::PathBuf {
    fn parse_path_from_str(s: &str) -> Self {
        // This parses multiple path components from the string, instead of creating a single
        std::path::PathBuf::from(s)
    }
}
impl Fs for StdFs {
    type Path = std::path::Path;
    type PathBuf = std::path::PathBuf;

    fn read_dir<'s, P: AsRef<Self::Path>>(
        &'s self,
        path: P,
    ) -> anyhow::Result<impl Iterator<Item = anyhow::Result<Self::PathBuf>>> {
        let i = std::fs::read_dir(path)?.map(|entry| match entry {
            Ok(e) => Ok(e.path()),
            Err(e) => Err(anyhow::anyhow!(e)),
        });
        Ok(i)
    }

    fn is_file<P: AsRef<Self::Path>>(&self, path: P) -> bool {
        path.as_ref().is_file()
    }

    fn is_dir<P: AsRef<Self::Path>>(&self, path: P) -> bool {
        path.as_ref().is_dir()
    }

    fn path_trailing<'p>(&self, path: &'p Self::Path) -> Option<&'p OsStr> {
        path.file_name()
    }

    fn path_ext<'p>(&self, path: &'p Self::Path) -> Option<&'p OsStr> {
        path.extension()
    }

    fn strip_prefix<'a, P: AsRef<Self::Path>>(
        &self,
        path_buf: &'a Self::PathBuf,
        prefix: P,
    ) -> anyhow::Result<&'a Self::Path> {
        Ok(path_buf.strip_prefix(prefix)?)
    }

    fn parse_native_metadata<P: AsRef<Self::Path>>(
        &self,
        path: P,
    ) -> anyhow::Result<NativeMetadata> {
        Ok(NativeMetadataFormat::parse_from_file(path.as_ref())?)
    }
    fn parse_config_file<P: AsRef<Self::Path>>(&self, path: P) -> anyhow::Result<ConfigFile> {
        let data = std::fs::read_to_string(path)?;
        ConfigFile::from_str(&data)
    }
    fn parse_group_file<P: AsRef<Self::Path>>(&self, path: P) -> anyhow::Result<GroupFile> {
        let data = std::fs::read_to_string(path)?;
        GroupFile::from_str(&data)
    }
}

#[cfg(test)]
pub mod test {
    use crate::{
        data_model::{
            native_metadata::NativeMetadataFormat,
            user_defined::{AlbumInputSongOverride, ConfigFile, Origin, ScanFilter},
        },
        fs::{Fs, FsPathBuf},
    };
    use std::{ffi::OsStr, path::PathBuf};

    use anyhow::bail;
    use string_literals::{s, string_vec};

    use crate::data_model::{
        Chromaprint, native_metadata::NativeMetadata, user_defined::GroupFile,
    };

    pub enum TestFs {
        MusicFile(NativeMetadata, Option<Chromaprint>),
        TextFile(String),
        OtherFile,
        Dir(Vec<(String, TestFs)>),
        SomethingElse,
    }

    impl super::FsPathBuf for Vec<String> {
        fn parse_path_from_str(s: &str) -> Self {
            s.split("/").map(|s| s.to_owned()).collect()
        }
    }

    impl TestFs {
        fn traverse<'s, P: AsRef<<Self as super::Fs>::Path>>(
            &'s self,
            path: P,
        ) -> anyhow::Result<&'s TestFs> {
            // TODO absolute path support
            let path = path.as_ref();
            match (self, &path) {
                (entry, &[]) => Ok(entry),
                // local paths (".." not supported)
                (TestFs::Dir(entries), &[next_comp, ..]) if next_comp == "." => {
                    self.traverse(&path[1..])
                }
                (TestFs::Dir(entries), &[next_comp, ..]) => {
                    // Recurse on the next element
                    for (subpath, entry) in entries.iter() {
                        if subpath == next_comp {
                            return entry.traverse(&path[1..]);
                        }
                    }
                    bail!("No such entry {next_comp} in directory")
                }
                (_, &[next_comp, ..]) => {
                    bail!("tried to traverse into {next_comp}, which is not a directory")
                }
            }
        }
    }

    impl Fs for TestFs {
        type Path = [String];
        type PathBuf = Vec<String>;

        fn read_dir<'s, P: AsRef<Self::Path>>(
            &'s self,
            path: P,
        ) -> anyhow::Result<impl Iterator<Item = anyhow::Result<Self::PathBuf>>> {
            let path = path.as_ref().to_owned();
            match self.traverse(&path)? {
                TestFs::Dir(entries) => {
                    // Return an iterator over *this* directory
                    let i = entries.iter().map(move |(subpath, _entry)| {
                        let mut v = path.iter().map(|s| s.clone()).collect::<Self::PathBuf>();
                        v.push(subpath.clone());
                        Ok(v)
                    });
                    Ok(i)
                }
                _ => {
                    bail!("read_dir called on a file or a SomethingElse")
                }
            }
        }

        fn path_trailing<'p>(&self, path: &'p Self::Path) -> Option<&'p OsStr> {
            path.as_ref().last().map(|comp| OsStr::new(comp))
        }
        fn path_ext<'p>(&self, path: &'p Self::Path) -> Option<&'p OsStr> {
            path.as_ref()
                .last()
                .map(|comp| comp.rsplit('.').next().map(|ext| OsStr::new(ext)))
                .flatten()
        }

        fn is_file<P: AsRef<Self::Path>>(&self, path: P) -> bool {
            match self.traverse(path) {
                Ok(TestFs::SomethingElse) => false,
                Ok(TestFs::Dir(..)) => false,
                Ok(_) => true,
                Err(_) => false,
            }
        }

        fn is_dir<P: AsRef<Self::Path>>(&self, path: P) -> bool {
            match self.traverse(path) {
                Ok(TestFs::Dir(..)) => true,
                _ => false,
            }
        }

        fn strip_prefix<'a, P: AsRef<Self::Path>>(
            &self,
            path_buf: &'a Self::PathBuf,
            prefix: P,
        ) -> anyhow::Result<&'a Self::Path> {
            if let Some(path) = path_buf.strip_prefix(prefix.as_ref()) {
                Ok(path)
            } else {
                bail!("prefix not actually a prefix of path_buf")
            }
        }

        fn parse_native_metadata<P: AsRef<Self::Path>>(
            &self,
            path: P,
        ) -> anyhow::Result<NativeMetadata> {
            match self.traverse(path)? {
                TestFs::MusicFile(native, _) => Ok((*native).clone()),
                _ => bail!("not a music file, no metadata found"),
            }
        }

        fn parse_config_file<P: AsRef<Self::Path>>(&self, path: P) -> anyhow::Result<ConfigFile> {
            match self.traverse(path)? {
                TestFs::TextFile(contents) => ConfigFile::from_str(&contents),
                _ => bail!("not a music file, no metadata found"),
            }
        }

        fn parse_group_file<P: AsRef<Self::Path>>(&self, path: P) -> anyhow::Result<GroupFile> {
            match self.traverse(path)? {
                TestFs::TextFile(contents) => GroupFile::from_str(&contents),
                _ => bail!("not a music file, no metadata found"),
            }
        }
    }

    macro_rules! test_dir {
        [ $( ($name:literal, $entry:expr), )* ] => {
            TestFs::Dir(vec![
                $(($name.to_owned(), $entry),)*
            ])
        };
    }

    fn test_hierarchy() -> TestFs {
        test_dir!(
            (
                "dir1",
                test_dir!(
                    ("file1", TestFs::OtherFile),
                    ("file2", TestFs::OtherFile),
                    ("file3", TestFs::OtherFile),
                )
            ),
            ("base_file", TestFs::OtherFile),
            (
                "config.tm2.toml",
                TestFs::TextFile(
                    r#"
search_paths=["example_album"]
"#
                    .to_string()
                )
            ),
            (
                "example_album",
                test_dir!(
                    (
                        "music.tm2.toml",
                        TestFs::TextFile(
                            r#"
type="Album"
scan_filter={ ext_filters=["mp3"] }
# exclude album_art_rel_path

[origin]
# put origin here eventually

[override_metadata]
album_title="Example Album"
album_artists=["Mr Example", "Ms Example"]

[[override_songs]]
file_rel_path="song1.mp3"
override_disc_idx=1
"#
                            .to_string()
                        )
                    ),
                    (
                        "song1.mp3",
                        TestFs::MusicFile(
                            NativeMetadata {
                                fmt: NativeMetadataFormat::ID3,
                                name: Some("song1".to_owned()),
                                album: None,
                                album_artists: vec![],
                                artist: vec![],
                                num_discs: None,
                                disc_idx: None,
                                num_tracks: None,
                                track_idx: None,
                                genres: vec![],
                            },
                            None
                        )
                    ),
                )
            ),
        )
    }

    fn debugify_error<T, E: std::fmt::Debug>(r: Result<T, E>) -> Result<T, String> {
        r.map_err(|e| format!("{e:?}"))
    }

    #[test]
    fn test_traverse() {
        type PathBuf = <TestFs as Fs>::PathBuf;
        let fs = test_hierarchy();
        assert!(fs.read_dir(PathBuf::parse_path_from_str("hello")).is_err());
        let dir1 = fs.read_dir(PathBuf::parse_path_from_str("dir1"));
        assert!(dir1.is_ok());
        let dir1_contents = dir1.unwrap().map(debugify_error).collect::<Vec<_>>();
        assert_eq!(
            dir1_contents,
            vec![
                Ok(string_vec!["dir1", "file1"]),
                Ok(string_vec!["dir1", "file2"]),
                Ok(string_vec!["dir1", "file3"]),
            ]
        );
    }

    #[test]
    fn test_is_file() {
        type PathBuf = <TestFs as Fs>::PathBuf;
        let fs = test_hierarchy();
        assert!(fs.is_file(PathBuf::parse_path_from_str("base_file")));
        assert!(fs.is_file(PathBuf::parse_path_from_str("config.tm2.toml")));
        assert!(fs.is_file(PathBuf::parse_path_from_str("example_album/music.tm2.toml")));
        assert!(fs.is_file(PathBuf::parse_path_from_str("example_album/song1.mp3")));

        assert!(!fs.is_file(PathBuf::parse_path_from_str("example_album")));
        assert!(!fs.is_file(PathBuf::parse_path_from_str("dir1")));
        assert!(!fs.is_file(PathBuf::parse_path_from_str("dummy I made up")));
    }

    #[test]
    fn test_is_dir() {
        type PathBuf = <TestFs as Fs>::PathBuf;
        let fs = test_hierarchy();
        assert!(fs.is_dir(PathBuf::parse_path_from_str("example_album")));
        assert!(fs.is_dir(PathBuf::parse_path_from_str("dir1")));

        assert!(!fs.is_dir(PathBuf::parse_path_from_str("base_file")));
        assert!(!fs.is_dir(PathBuf::parse_path_from_str("config.tm2.toml")));
        assert!(!fs.is_dir(PathBuf::parse_path_from_str("example_album/music.tm2.toml")));
        assert!(!fs.is_dir(PathBuf::parse_path_from_str("example_album/song1.mp3")));
        assert!(!fs.is_dir(PathBuf::parse_path_from_str("dummy I made up")));
    }

    #[test]
    fn test_config_file() {
        type PathBuf = <TestFs as Fs>::PathBuf;
        let fs = test_hierarchy();
        let file =
            debugify_error(fs.parse_config_file(PathBuf::parse_path_from_str("config.tm2.toml")));
        assert_eq!(
            file,
            Ok(ConfigFile {
                search_paths: Some(string_vec!["example_album"]),
                artist_name_overrides: None,
            })
        );
    }

    #[test]
    fn test_group_file() {
        type PathBuf = <TestFs as Fs>::PathBuf;
        let fs = test_hierarchy();
        let file = debugify_error(
            fs.parse_group_file(PathBuf::parse_path_from_str("example_album/music.tm2.toml")),
        );
        assert_eq!(
            file,
            Ok(GroupFile::Album {
                origin: Origin::default(),
                scan_filter: Some(ScanFilter {
                    ext_filters: string_vec!["mp3"]
                }),
                album_art_rel_path: None,
                override_metadata: Some(crate::data_model::metadata::album::Override {
                    album_title: Some(s!("Example Album")),
                    album_artists: Some(string_vec!["Mr Example", "Ms Example"]),
                    fixed_disc_idx: None,
                    offset_track_idx: None,
                }),
                override_songs: Some(vec![AlbumInputSongOverride {
                    file_rel_path: s!("song1.mp3"),
                    override_metadata: None,
                    override_disc_idx: Some(1),
                    override_track_idx: None,
                }]),
            })
        );
    }

    #[test]
    fn test_song_metadata() {
        type PathBuf = <TestFs as Fs>::PathBuf;
        let fs = test_hierarchy();
        let file = debugify_error(
            fs.parse_native_metadata(PathBuf::parse_path_from_str("example_album/song1.mp3")),
        );
        assert_eq!(
            file,
            Ok(NativeMetadata {
                fmt: NativeMetadataFormat::ID3,
                name: Some("song1".to_owned()),
                album: None,
                album_artists: vec![],
                artist: vec![],
                num_discs: None,
                disc_idx: None,
                num_tracks: None,
                track_idx: None,
                genres: vec![],
            })
        );
    }
}
