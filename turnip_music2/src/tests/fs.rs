use crate::{
    data_model::{
        Chromaprint,
        native_metadata::{NativeMetadata, NativeMetadataFormat},
        user_defined::{self, AlbumFileMeta, ConfigFile, ConfigFileInputs, GroupFile, Origin},
    },
    fs::{Fs, FsPathBuf},
};
use std::{ffi::OsStr, path::PathBuf};

use anyhow::bail;
use indexmap::IndexMap;
use string_literals::{s, string_vec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestFs {
    MusicFile(NativeMetadata, Option<Chromaprint>),
    TextFile(String),
    OtherFile,
    Dir(Vec<(String, TestFs)>),
    SomethingElse,
}

/// TODO absolute path support
/// TODO ../ support
impl FsPathBuf<[String]> for Vec<String> {
    fn parse_path_from_str(s: &str) -> Self {
        s.split("/").map(|s| s.to_owned()).collect()
    }

    fn joined(mut self, p: &str) -> Self {
        self.extend(Self::parse_path_from_str(p));
        self
    }
}

impl TestFs {
    // TODO these functions shouldn't be recursive, it discards information
    pub fn traverse<'s, P: AsRef<<Self as Fs>::Path>>(
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

    pub fn overwrite<'s, P: AsRef<<Self as Fs>::Path>>(
        &mut self,
        path: P,
        file: TestFs,
    ) -> anyhow::Result<()> {
        // TODO absolute path support
        let path = path.as_ref();
        // Reborrow https://users.rust-lang.org/t/matching-on-mut-self-without-moving-it/101411
        match (&mut *self, &path) {
            (entry, &[]) => bail!("can't overwrite, empty path"),
            (TestFs::Dir(entries), &[name]) => {
                let mut inserted = false;
                for (subpath, entry) in entries.iter_mut() {
                    if subpath == name {
                        *entry = file;
                        return Ok(());
                    }
                }
                entries.push((name.clone(), file));
                Ok(())
            }
            (entry, &[name]) => bail!("can't overwrite, containing level was not a directory"),
            // local paths (".." not supported)
            (TestFs::Dir(entries), &[next_comp, ..]) if next_comp == "." => {
                return self.overwrite(&path[1..], file);
            }
            (TestFs::Dir(entries), &[next_comp, ..]) => {
                // Recurse on the next element
                for (subpath, entry) in entries.iter_mut() {
                    if subpath == next_comp {
                        return entry.overwrite(&path[1..], file);
                    }
                }
                bail!("can't overwrite, no such entry {next_comp} in directory")
            }
            (_, &[next_comp, ..]) => {
                bail!("tried to overwrite into {next_comp}, which is not a directory")
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
    fn path_parent_dir<'p>(&self, path: &'p Self::Path) -> Option<Self::PathBuf> {
        path.as_ref()
            .split_last()
            .map(|(last, prelast)| prelast.to_vec())
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
            TestFs::MusicFile(native, _) => Ok(native.clone()),
            _ => bail!("not a music file, no metadata found"),
        }
    }

    fn parse_config_file<P: AsRef<Self::Path>>(
        &self,
        path: P,
    ) -> anyhow::Result<(toml_edit::DocumentMut, ConfigFile)> {
        match self.traverse(path)? {
            TestFs::TextFile(contents) => ConfigFile::from_str(&contents),
            _ => bail!("not a music file, no metadata found"),
        }
    }

    fn parse_group_file<P: AsRef<Self::Path>>(
        &self,
        path: P,
    ) -> anyhow::Result<(toml_edit::DocumentMut, GroupFile)> {
        match self.traverse(path)? {
            TestFs::TextFile(contents) => GroupFile::from_str(&contents),
            _ => bail!("not a music file, no metadata found"),
        }
    }

    fn write_toml_file<P: AsRef<Self::Path>>(
        &mut self,
        path: P,
        doc: toml_edit::DocumentMut,
    ) -> anyhow::Result<()> {
        let string = doc.to_string();
        self.overwrite(path, TestFs::TextFile(string))
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
[library]
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
# exclude album_art_rel_path

[origin]
# put origin here eventually

[global]
album="Example Album"
album_artists=["Mr Example", "Ms Example"]

[files."song1.mp3"]
name="song1"
"#
                        .to_string()
                    )
                ),
                (
                    "song1.mp3",
                    TestFs::MusicFile(
                        NativeMetadata {
                            fmt: NativeMetadataFormat::ID3,
                            name: Some(s!("song1-mp3meta")),
                            album: None,
                            album_artists: vec![],
                            artists: vec![],
                            num_discs: None,
                            disc: None,
                            num_tracks: None,
                            track: None,
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
        file.map(|(doc, c)| c),
        Ok(ConfigFile {
            library: ConfigFileInputs {
                search_paths: string_vec!["example_album"],
            },
            exports: None,
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
        file.map(|(doc, g)| g),
        Ok(GroupFile::Album {
            origin: Origin::default(),
            album_art: None,
            global: user_defined::AlbumGlobalMeta {
                album: Some(s!("Example Album")),
                album_artists: Some(string_vec!["Mr Example", "Ms Example"]),
                ..Default::default()
            },
            files: indexmap::indexmap! {
                s!("song1.mp3") => user_defined::AlbumFileMeta {
                    name: s!("song1"),
                    ..Default::default()
                }
            },
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
            name: Some("song1-mp3meta".to_owned()),
            album: None,
            album_artists: vec![],
            artists: vec![],
            num_discs: None,
            disc: None,
            num_tracks: None,
            track: None,
            genres: vec![],
        })
    );
}
