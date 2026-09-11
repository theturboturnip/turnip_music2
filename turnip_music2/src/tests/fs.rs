use crate::{
    data_model::{
        Chromaprint,
        native_metadata::{NativeMetadata, NativeMetadataFormat, NativeMusicExt},
        user_defined::{self, ConfigFile, ConfigFileInputs, GroupFile, Origin},
    },
    export::{FfmpegArgs, ToOsString},
    fs::{Fs, FsPathBuf},
};
use std::ffi::{OsStr, OsString};

use anyhow::bail;
use string_literals::{s, string_vec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TestFs {
    MusicFile(NativeMetadata, Option<Chromaprint>),
    FfmpegOutputFile(OsString, FfmpegArgs),
    TextFile(String),
    OtherFile,
    Dir(Vec<(String, TestFs)>),
    SomethingElse,
}

impl ToOsString for Vec<String> {
    fn to_os_string(self) -> std::ffi::OsString {
        // TODO handle escaping, backslashes, etc.
        self.join("/").to_os_string()
    }
}
/// TODO absolute path support
/// TODO ../ support
impl FsPathBuf<[String]> for Vec<String> {
    fn parse_path_from_user_str(s: &str) -> Self {
        // TODO handle escaping, backslashes, etc.
        s.split("/").map(|s| s.to_owned()).collect()
    }

    fn plus(mut self, p: &[String]) -> Self {
        self.extend(p.iter().map(|s| s.clone()));
        self
    }

    fn build<S: AsRef<str>, I: Iterator<Item = S>>(components: I) -> Self {
        components.map(|s| s.as_ref().to_string()).collect()
    }

    fn map<F: FnMut(&str) -> String>(&self, mut f: F) -> Self {
        self.iter().map(|s| f(s)).collect()
    }
}

impl TestFs {
    pub fn traverse<'s, P: AsRef<<Self as Fs>::Path>>(
        &'s self,
        path: P,
    ) -> anyhow::Result<&'s TestFs> {
        let constant_path_stack = path.as_ref();
        {
            let mut i = 0;
            let mut curr_fs = self;
            'walk: while i <= constant_path_stack.len() {
                let curr_path = &constant_path_stack[..i];
                let rem_path = &constant_path_stack[i..];
                match (&curr_fs, &rem_path) {
                    (entry, &[]) => return Ok(entry),
                    // local paths (".." not supported)
                    (TestFs::Dir(entries), &[next_comp, ..]) if next_comp == "." => {
                        i += 1;
                        continue;
                    }
                    // TODO '..' support here should now work
                    (TestFs::Dir(entries), &[next_comp, ..]) => {
                        // Recurse on the next element
                        for (subpath, entry) in entries.iter() {
                            if subpath == next_comp {
                                i += 1;
                                curr_fs = entry;
                                continue 'walk;
                            }
                        }
                        bail!(
                            "Path lookup {constant_path_stack:?}: At directory {curr_path:?} {i}, tried to find {next_comp:?} but it didn't exist"
                        )
                    }
                    (_, &[next_comp, ..]) => {
                        bail!(
                            "Path lookup {constant_path_stack:?}: At {curr_path:?} {i} that isn't a directory, tried to traverse into file {next_comp:?}"
                        )
                    }
                }
            }
            bail!("Mystery error");
        }
    }

    fn take_dir_entries(&mut self) -> &mut Vec<(String, Self)> {
        if let TestFs::Dir(entries) = self {
            return entries;
        };
        panic!();
    }
    pub fn mkdir_p<'s, P: AsRef<<Self as Fs>::Path>>(&mut self, path: P) -> anyhow::Result<()> {
        let constant_path_stack = path.as_ref();
        {
            let mut i = 0;
            let mut curr_fs = self;
            while i <= constant_path_stack.len() {
                let curr_path = &constant_path_stack[..i];
                let rem_path = &constant_path_stack[i..];
                // Do a match to see if we hit a directory
                let next_comp: &String = match (&mut curr_fs, &rem_path) {
                    // We reached the end, didn't have to create anything, this entry must be a directory
                    (TestFs::Dir(_), &[]) => return Ok(()),
                    // local paths (".." not supported)
                    (TestFs::Dir(_), &[next_comp, ..]) if next_comp == "." => {
                        continue;
                    }
                    // TODO '..' support here should now work
                    (TestFs::Dir(_), &[next_comp, ..]) => next_comp,
                    (_, _) => {
                        bail!(
                            "Mkdir {constant_path_stack:?}: Arrived at {curr_path:?} {i} but it exists and is not a directory."
                        )
                    }
                };
                // We have hit a directory and need to either traverse into it or make a new subdirectory first.
                // Either way, we will move forward
                i += 1;
                curr_fs = {
                    let entries = curr_fs.take_dir_entries();
                    // Search the entries...
                    if entries
                        .iter()
                        .find(|(subpath, _entry)| subpath == next_comp)
                        .is_some()
                    {
                        // ... and then search them *again* so we can go into the directory that already exists.
                        // We could do this all in one, and potentially all in the above match, if we had
                        // Polonius <https://blog.rust-lang.org/2026/08/04/enabling-polonius-alpha-on-nightly/>
                        let matching_entry = entries
                            .iter_mut()
                            .find(|(subpath, _entry)| subpath == next_comp);
                        &mut matching_entry.unwrap().1
                    } else {
                        let new_entry = entries.push_mut((next_comp.clone(), TestFs::Dir(vec![])));
                        // Create a new directory
                        &mut new_entry.1
                    }
                }
            }
            bail!("Mystery error");
        }
    }

    pub fn overwrite<'s, P: AsRef<<Self as Fs>::Path>>(
        &mut self,
        path: P,
        file: TestFs,
    ) -> anyhow::Result<Option<TestFs>> {
        let constant_path_stack = path.as_ref();
        {
            let mut i = 0;
            let mut curr_fs = self;
            'walk: while i <= constant_path_stack.len() {
                let curr_path = &constant_path_stack[..i];
                let rem_path = &constant_path_stack[i..];
                // Do a match to see if we need to recurse into a directory
                let next_comp: &String = match (&mut curr_fs, &rem_path) {
                    (_entry, &[]) => {
                        bail!("Can't overwrite, ran out of path in {constant_path_stack:?}")
                    }
                    (TestFs::Dir(entries), &[name]) => {
                        if name == "." {
                            bail!("Overwrite {constant_path_stack:?}: Cannot overwrite '.'");
                        }
                        for (subpath, entry) in entries.iter_mut() {
                            if subpath == name {
                                let old = std::mem::replace(entry, file);
                                return Ok(Some(old));
                            }
                        }
                        entries.push((name.clone(), file));
                        return Ok(None);
                    }
                    (_entry, &[_name]) => {
                        bail!("Overwrite {constant_path_stack:?}: {curr_path:?} is not a directory")
                    }
                    // local paths (".." not supported)
                    (TestFs::Dir(entries), &[next_comp, ..]) if next_comp == "." => {
                        i += 1;
                        continue;
                    }
                    // TODO '..' support here should now work
                    (TestFs::Dir(entries), &[next_comp, ..]) => next_comp,
                    (_, &[next_comp, ..]) => {
                        bail!(
                            "Overwrite {constant_path_stack:?}: At {curr_path:?} {i} that isn't a directory, tried to traverse into file {next_comp:?}"
                        )
                    }
                };
                // Recurse into the next directory
                // We could probably do this inside the (TestFs::Dir(entries), &[next_comp, ..]) match
                // IF we had polonius.
                let entries = curr_fs.take_dir_entries();
                for (subpath, entry) in entries.iter_mut() {
                    if subpath == next_comp {
                        i += 1;
                        curr_fs = entry;
                        continue 'walk;
                    }
                }
                bail!(
                    "Overwrite {constant_path_stack:?}: At directory {curr_path:?} {i}, tried to find {next_comp:?} but it didn't exist"
                )
            }
            bail!("Mystery error");
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

    fn path_stringify<'p>(&self, path: &'p Self::Path) -> String {
        // TODO need some handling of escaping and stuff
        path.join("/")
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
            .map(|(_last, prelast)| prelast.to_vec())
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
    ) -> anyhow::Result<(Option<NativeMusicExt>, NativeMetadata)> {
        let file_ext: Option<NativeMusicExt> = self
            .path_ext(path.as_ref())
            .map(|e| e.to_str())
            .flatten()
            .map(|e| e.parse().ok())
            .flatten();
        match self.traverse(path.as_ref())? {
            TestFs::MusicFile(native, _) => {
                assert_eq!(
                    native.fmt,
                    file_ext
                        .map(|e| e.into())
                        .unwrap_or(NativeMetadataFormat::None),
                    "Mismatching file metadata for {:?} based on extension",
                    path.as_ref()
                );
                Ok((file_ext, native.clone()))
            }
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
        let overwritten = self.overwrite(path.as_ref(), TestFs::TextFile(string))?;
        if let Some(overwritten) = overwritten
            && !matches!(overwritten, TestFs::TextFile(..))
        {
            panic!(
                "write_toml_file overwrote a {:?} at {:?} - likely unintended",
                overwritten,
                path.as_ref()
            );
        }
        Ok(())
    }

    fn write_text_file<P: AsRef<Self::Path>>(
        &mut self,
        path: P,
        contents: String,
    ) -> anyhow::Result<()> {
        let string = contents;
        let overwritten = self.overwrite(path.as_ref(), TestFs::TextFile(string))?;
        if let Some(overwritten) = overwritten
            && !matches!(overwritten, TestFs::TextFile(..))
        {
            panic!(
                "write_text_file overwrote a {:?} at {:?} - likely unintended",
                overwritten,
                path.as_ref()
            );
        }
        Ok(())
    }

    fn create_dir_all<P: AsRef<Self::Path>>(&mut self, path: P) -> anyhow::Result<()> {
        self.mkdir_p(path)
    }

    fn execute_ffmpeg(&mut self, ffmpeg: &OsStr, args: FfmpegArgs) -> anyhow::Result<()> {
        let output_path = args
            .0
            .iter()
            .last()
            .map(|path| Self::PathBuf::parse_path_from_user_str(path.to_str().unwrap()))
            .expect("ffmpeg invocation {:?} should not be empty");
        let overwritten = self.overwrite(
            &output_path,
            TestFs::FfmpegOutputFile(ffmpeg.to_owned(), args),
        )?;
        // Allow ffmpeg overwrites to collide, we have warnings for that
        if let Some(overwritten) = overwritten
            && !matches!(overwritten, TestFs::FfmpegOutputFile(..))
        {
            panic!(
                "execute_ffmpeg overwrote {:?} at path {:?}",
                overwritten, &output_path,
            );
        }
        Ok(())
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
type="album"
# exclude album_art_rel_path

[origin]
# put origin here eventually

[global]
album="Example Album"
album_artists=["Mr Example", "Ms Example"]

[files."song1.mp3"]
title="song1"
"#
                        .to_string()
                    )
                ),
                (
                    "song1.mp3",
                    TestFs::MusicFile(
                        NativeMetadata {
                            fmt: NativeMetadataFormat::Id3,
                            title: Some(s!("song1-mp3meta")),
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
    assert!(
        fs.read_dir(PathBuf::parse_path_from_user_str("hello"))
            .is_err()
    );
    let dir1 = fs.read_dir(PathBuf::parse_path_from_user_str("dir1"));
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
    assert!(fs.is_file(PathBuf::parse_path_from_user_str("base_file")));
    assert!(fs.is_file(PathBuf::parse_path_from_user_str("config.tm2.toml")));
    assert!(fs.is_file(PathBuf::parse_path_from_user_str(
        "example_album/music.tm2.toml"
    )));
    assert!(fs.is_file(PathBuf::parse_path_from_user_str("example_album/song1.mp3")));

    assert!(!fs.is_file(PathBuf::parse_path_from_user_str("example_album")));
    assert!(!fs.is_file(PathBuf::parse_path_from_user_str("dir1")));
    assert!(!fs.is_file(PathBuf::parse_path_from_user_str("dummy I made up")));
}

#[test]
fn test_is_dir() {
    type PathBuf = <TestFs as Fs>::PathBuf;
    let fs = test_hierarchy();
    assert!(fs.is_dir(PathBuf::parse_path_from_user_str("example_album")));
    assert!(fs.is_dir(PathBuf::parse_path_from_user_str("dir1")));

    assert!(!fs.is_dir(PathBuf::parse_path_from_user_str("base_file")));
    assert!(!fs.is_dir(PathBuf::parse_path_from_user_str("config.tm2.toml")));
    assert!(!fs.is_dir(PathBuf::parse_path_from_user_str(
        "example_album/music.tm2.toml"
    )));
    assert!(!fs.is_dir(PathBuf::parse_path_from_user_str("example_album/song1.mp3")));
    assert!(!fs.is_dir(PathBuf::parse_path_from_user_str("dummy I made up")));
}

#[test]
fn test_config_file() {
    type PathBuf = <TestFs as Fs>::PathBuf;
    let fs = test_hierarchy();
    let file =
        debugify_error(fs.parse_config_file(PathBuf::parse_path_from_user_str("config.tm2.toml")));
    assert_eq!(
        file.map(|(_doc, c)| c),
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
    let file = debugify_error(fs.parse_group_file(PathBuf::parse_path_from_user_str(
        "example_album/music.tm2.toml",
    )));
    assert_eq!(
        file.map(|(_doc, g)| g),
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
                    title: s!("song1"),
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
        fs.parse_native_metadata(PathBuf::parse_path_from_user_str("example_album/song1.mp3")),
    );
    assert_eq!(
        file,
        Ok((
            Some(NativeMusicExt::Mp3),
            NativeMetadata {
                fmt: NativeMetadataFormat::Id3,
                title: Some("song1-mp3meta".to_owned()),
                album: None,
                album_artists: vec![],
                artists: vec![],
                num_discs: None,
                disc: None,
                num_tracks: None,
                track: None,
                genres: vec![],
            }
        ))
    );
}
