use crate::{data_model::native_metadata::*, tests::fs::TestFs};
use string_literals::s;

macro_rules! test_dir {
    [ $( ($name:literal, $entry:expr), )* ] => {
        TestFs::Dir(vec![
            $(($name.to_owned(), $entry),)*
        ])
    };
}
macro_rules! assert_matches {
    ( $actual:expr, $expected:pat, $($msg:tt)* ) => {{
        let actual = $actual;
        if !matches!(actual, $expected) {
            panic!("{actual:?} didn't meet condition: {}", format!($($msg)*));
        }
    }};
    ( $actual:expr, $expected:pat_param if $e:expr, $($msg:tt)* ) => {{
        let actual = $actual;
        if let $expected = actual && ($e) {

        } else {
            panic!("{actual:?} didn't meet condition: {}", format!($($msg)*));
        }
    }};
}
macro_rules! test_path {
    ($($msg:literal),*) => {
        string_literals::string_vec![$($msg,)*]
    };
}

mod init {
    use super::*;
    use crate::{
        cli::{CliContext, Library},
        tests::cli::basic_unpadded_tracks,
        warning::Warning,
    };

    #[test]
    fn test_basic_init_no_library_path() {
        let mut fs = basic_test_hierarchy(
            test_dir!(
                ("basic", basic_unpadded_tracks()), //
            ),
            false,
        );
        let mut warner = vec![];

        let library = || -> anyhow::Result<Option<Library<_>>> {
            let mut ctx = CliContext::new(None, &mut fs, &mut warner);
            ctx.init(vec![s!("songs")], false)?;
            Ok(ctx.loaded_library)
        }();

        assert_matches!(
            fs.traverse(test_path!("library.tm2.toml")),
            Ok(TestFs::TextFile(t)) if t ==
r#"[library]
search_paths = ["songs"]
"#,
            "Library config should be correct"
        );
        assert_matches!(library, Ok(None), "shouldn't have loaded library");
        assert_eq!(
            warner,
            vec![Warning::OrphanedSongs {
                folder: test_path!("songs", "basic"),
                files: vec![
                    test_path!("songs", "basic", "1-The Biggest Fish.wav"),
                    test_path!("songs", "basic", "2-The Next Biggest Fish.wav"),
                    test_path!("songs", "basic", "11-Fish to the Twenty-First Order.wav"),
                ],
            }],
            "should have produced orphaned songs warning"
        );
    }

    #[test]
    fn test_basic_init_custom_library_path() {
        let mut fs = test_dir!(
            (
                "toplevel",
                basic_test_hierarchy(
                    test_dir!(
                        ("basic", basic_unpadded_tracks()), //
                    ),
                    false,
                )
            ), //
        );
        let mut warner = vec![];

        let library = || -> anyhow::Result<Option<Library<_>>> {
            let mut ctx = CliContext::new(
                Some(s!("toplevel/custom_library.toml")),
                &mut fs,
                &mut warner,
            );
            ctx.init(vec![s!("songs")], false)?;
            Ok(ctx.loaded_library)
        }();

        assert_matches!(
            fs.traverse(test_path!("toplevel", "custom_library.toml")),
            Ok(TestFs::TextFile(t)) if t ==
r#"[library]
search_paths = ["songs"]
"#,
            "Library config should be correct"
        );
        assert_matches!(library, Ok(None), "shouldn't have loaded library");
        assert_eq!(
            warner,
            vec![Warning::OrphanedSongs {
                folder: test_path!("toplevel", "songs", "basic"),
                files: vec![
                    test_path!("toplevel", "songs", "basic", "1-The Biggest Fish.wav"),
                    test_path!("toplevel", "songs", "basic", "2-The Next Biggest Fish.wav"),
                    test_path!(
                        "toplevel",
                        "songs",
                        "basic",
                        "11-Fish to the Twenty-First Order.wav"
                    ),
                ],
            }],
            "should have produced orphaned songs warning"
        );
    }
}

fn basic_test_hierarchy(songs: TestFs, with_library: bool) -> TestFs {
    if with_library {
        test_dir!(
            (
                "library.tm2.toml",
                TestFs::TextFile(s!(r#"
[library]
search_paths = ["songs"]
    "#))
            ),
            ("songs", songs),
        )
    } else {
        test_dir!(("songs", songs),)
    }
}

/// ODD FUTURE CD rip to FLAC cddb id 19028203
fn cdrip_oddfuture() -> TestFs {
    test_dir!(
        (
            "track01.flac",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("ODD FUTURE")),
                    album: Some(s!("ODD FUTURE")),
                    album_artists: vec![],
                    artists: vec![s!("UVERworld")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(1),
                    genres: vec![]
                },
                None
            )
        ),
        (
            "track02.flac",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("PLOT")),
                    album: Some(s!("ODD FUTURE")),
                    album_artists: vec![],
                    artists: vec![s!("UVERworld")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(1),
                    genres: vec![]
                },
                None
            )
        ),
        (
            "track03.flac",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("CORE STREAM")),
                    album: Some(s!("ODD FUTURE")),
                    album_artists: vec![],
                    artists: vec![s!("UVERworld")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(1),
                    genres: vec![]
                },
                None
            )
        ),
    )
}

/// SOUVENIR CD rip to FLAC cddb id 21065404
fn cdrip_souvenir() -> TestFs {
    test_dir!(
        (
            "track01.flac",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("SOUVENIR")),
                    album: Some(s!("SOUVENIR")),
                    album_artists: vec![],
                    artists: vec![s!("BUMP OF CHICKEN")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(1),
                    genres: vec![]
                },
                None
            )
        ),
        (
            "track02.flac",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("クロノスタシス")),
                    album: Some(s!("SOUVENIR")),
                    album_artists: vec![],
                    artists: vec![s!("BUMP OF CHICKEN")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(2),
                    genres: vec![]
                },
                None
            )
        ),
        (
            "track03.flac",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("窓の中から")),
                    album: Some(s!("SOUVENIR")),
                    album_artists: vec![],
                    artists: vec![s!("BUMP OF CHICKEN")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(3),
                    genres: vec![]
                },
                None
            )
        ),
        (
            "track04.flac",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("Track 4")),
                    album: Some(s!("SOUVENIR")),
                    album_artists: vec![],
                    artists: vec![s!("BUMP OF CHICKEN")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(4),
                    genres: vec![]
                },
                None
            )
        ),
    )
}

/// Partial DELTARUNE bandcamp download
fn deltarune_partial() -> TestFs {
    test_dir!(
        ("cover.png", TestFs::OtherFile),
        (
            "Laura Shigihara - DELTARUNE Chapter 1 OST - 39 Don't Forget.mp3",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("Don't Forget")),
                    album: Some(s!("DELTARUNE Chapter 1 OST")),
                    album_artists: vec![s!("Toby Fox")],
                    artists: vec![s!("Laura Shigihara")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(39),
                    genres: vec![]
                },
                None
            )
        ),
        (
            "Toby Fox - DELTARUNE Chapter 1 OST - 01 ANOTHER HIM.mp3",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("ANOTHER HIM")),
                    album: Some(s!("DELTARUNE Chapter 1 OST")),
                    album_artists: vec![s!("Toby Fox")],
                    artists: vec![s!("Toby Fox")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(1),
                    genres: vec![]
                },
                None
            )
        ),
        (
            "Toby Fox - DELTARUNE Chapter 1 OST - 02 Beginning.mp3",
            TestFs::MusicFile(
                NativeMetadata {
                    fmt: NativeMetadataFormat::FLAC,
                    name: Some(s!("Beginning")),
                    album: Some(s!("DELTARUNE Chapter 1 OST")),
                    album_artists: vec![s!("Toby Fox")],
                    artists: vec![s!("Toby Fox")],
                    num_discs: None,
                    disc: None,
                    num_tracks: None,
                    track: Some(3),
                    genres: vec![]
                },
                None
            )
        ),
    )
}

fn basic_zero_padded_tracks() -> TestFs {
    test_dir!(
        (
            "01-The Biggest Fish.wav",
            TestFs::MusicFile(NativeMetadata::default(), None)
        ),
        (
            "02-The Next Biggest Fish.wav",
            TestFs::MusicFile(NativeMetadata::default(), None)
        ),
        (
            "11-Fish to the Twenty-First Order.wav",
            TestFs::MusicFile(NativeMetadata::default(), None)
        ),
    )
}

fn basic_unpadded_tracks() -> TestFs {
    test_dir!(
        (
            "1-The Biggest Fish.wav",
            TestFs::MusicFile(NativeMetadata::default(), None)
        ),
        (
            "2-The Next Biggest Fish.wav",
            TestFs::MusicFile(NativeMetadata::default(), None)
        ),
        (
            "11-Fish to the Twenty-First Order.wav",
            TestFs::MusicFile(NativeMetadata::default(), None)
        ),
    )
}
