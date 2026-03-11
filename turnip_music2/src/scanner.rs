use crate::data_model::native_metadata::NATIVE_MUSIC_EXTS;
use crate::data_model::{AlbumInputGroup, CompilationInputGroup, user_defined};
use crate::fs::Fs;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
const GROUP_FILE_NAME: &'static str = "music.tm2.toml";

pub enum Group<F: Fs> {
    PartialAlbum(AlbumInputGroup<F>, F::PathBuf),
    PartialCompilation(CompilationInputGroup<F>, F::PathBuf),
}

pub fn scan_library<F: Fs>(fs: &F, root_path: F::PathBuf) -> anyhow::Result<Vec<Group<F>>> {
    let mut scan_stack = vec![root_path];
    let group_file_name = OsStr::new(GROUP_FILE_NAME);
    let mut groups = vec![];

    while let Some(dir) = scan_stack.pop() {
        let mut files = vec![];
        let mut dirs = vec![];
        let mut group = None;

        for path in fs.read_dir(dir)? {
            let path = path?;

            if fs.is_dir(&path) {
                dirs.push(path);
            } else if fs.is_file(&path) {
                if fs.path_trailing(path.as_ref()) == Some(group_file_name) {
                    group = Some((fs.parse_group_file(&path)?, path));
                } else {
                    files.push(path);
                }
            }
        }

        if let Some((group, path)) = group {
            groups.push((path, group, dirs, files));
        } else {
            scan_stack.extend(dirs);
        }
    }

    // TODO par_iter here?
    groups
        .into_iter()
        .map(|(path, group, dirs, files)| scan_group(fs, path, group, dirs, files))
        .collect::<anyhow::Result<Vec<_>>>()
}

fn scan_group<F: Fs>(
    fs: &F,
    root_path: F::PathBuf,
    group: user_defined::GroupFile,
    root_dirs: Vec<F::PathBuf>,
    root_files: Vec<F::PathBuf>,
) -> anyhow::Result<Group<F>> {
    let mut scan_stack = root_dirs;
    // TODO have to include path-relative-to-root_dirs
    let mut music_files: Vec<F::PathBuf> = vec![];
    let scan_exts: HashSet<OsString> = group.scan_filter().map_or_else(
        || NATIVE_MUSIC_EXTS.iter().map(|s| s.into()).collect(),
        |scan_filter| scan_filter.ext_filters.iter().map(|s| s.into()).collect(),
    );

    for path in root_files {
        if let Some(ext) = fs.path_ext(path.as_ref()) {
            if scan_exts.contains(ext) {
                music_files.push(path);
            }
        }
    }

    while let Some(dir) = scan_stack.pop() {
        for path in fs.read_dir(dir)? {
            let path = path?;

            if fs.is_dir(&path) {
                scan_stack.push(path);
            } else if fs.is_file(&path) {
                if let Some(ext) = fs.path_ext(path.as_ref()) {
                    if scan_exts.contains(ext) {
                        music_files.push(path);
                    }
                }
            }
        }
    }

    match group {
        user_defined::GroupFile::Compilation {
            origin,
            scan_filter,
            title,
            songs,
        } => Ok(Group::PartialCompilation(
            CompilationInputGroup::new(
                fs,
                root_path.as_ref(),
                origin,
                scan_filter,
                title,
                songs,
                music_files,
            ),
            root_path,
        )),
        user_defined::GroupFile::Album {
            origin,
            scan_filter,
            album_art_rel_path,
            override_metadata,
            songs,
        } => Ok(Group::PartialAlbum(
            AlbumInputGroup::new(
                fs,
                root_path.as_ref(),
                origin,
                override_metadata,
                scan_filter,
                album_art_rel_path,
                songs,
                music_files,
            ),
            root_path,
        )),
    }
}
