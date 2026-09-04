use crate::data_model::{native_metadata, parsed, user_defined};
use crate::fs::Fs;
use crate::warning::{Warning, WarningSender};
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};

pub struct Group<F: Fs> {
    path: F::PathBuf,
    doc: toml_edit::DocumentMut,
    parsed: parsed::GroupFile<F>,
}

pub struct ScannedDir<F: Fs> {
    pub group_file: Option<F::PathBuf>,
    pub all_music_files: Vec<F::PathBuf>,
    pub dirs: Vec<F::PathBuf>,
}

pub fn scan_dir<F: Fs>(fs: &F, dir: &F::Path) -> anyhow::Result<ScannedDir<F>> {
    let group_file_name = OsStr::new(user_defined::GroupFile::TOML_FILE_NAME);

    // let mut files = vec![];
    let mut dirs = vec![];
    let mut group = None;
    let mut music_files_found = vec![];

    for path in fs.read_dir(&dir)? {
        let path = path?;

        if fs.is_dir(&path) {
            dirs.push(path);
        } else if fs.is_file(&path) {
            if fs.path_trailing(path.as_ref()) == Some(group_file_name) {
                group = Some(path);
                continue;
            }
            if let Some(ext) = fs
                .path_ext(path.as_ref())
                .map(|os_str| os_str.to_str())
                .flatten()
                && native_metadata::NATIVE_MUSIC_EXTS.contains(&ext)
            {
                music_files_found.push(path.clone());
            }
            // files.push(path);
        }
    }

    Ok(ScannedDir {
        group_file: group,
        all_music_files: music_files_found,
        dirs,
    })
}

pub fn scan_library<F: Fs, W: WarningSender<F::PathBuf>>(
    fs: &F,
    warner: &mut W,
    root_path: F::PathBuf,
) -> anyhow::Result<Vec<Group<F>>> {
    let mut scan_stack = vec![root_path];
    let mut groups = vec![];

    while let Some(dir) = scan_stack.pop() {
        let s = scan_dir(fs, dir.as_ref())?;

        if let Some(path) = s.group_file {
            // TODO this shouldn't go into anyhow, it should use the warner
            let (group_doc, group_struct) = fs.parse_group_file(&path)?;
            let group = groups.push((path, group_doc, group_struct));
            // We do NOT recursive scan, because we assume the group file will cover all children
        } else {
            // Recursive scan
            scan_stack.extend(s.dirs);
            if !s.all_music_files.is_empty() {
                warner.warn(Warning::OrphanedSongs {
                    folder: dir.clone(),
                    files: s.all_music_files,
                });
            }
        }
    }

    // TODO par_iter here?
    Ok(groups
        .into_iter()
        .map(|(path, group_doc, group_struct)| Group {
            doc: group_doc,
            parsed: parsed::GroupFile::from_user(fs, path.as_ref(), group_struct),
            path,
        })
        .collect::<Vec<_>>())
}
