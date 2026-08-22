use crate::data_model::{native_metadata, parsed, user_defined};
use crate::fs::Fs;
use std::collections::HashSet;
use std::ffi::{OsStr, OsString};

pub fn scan_library<F: Fs>(
    fs: &F,
    root_path: F::PathBuf,
) -> anyhow::Result<Vec<parsed::GroupFile<F>>> {
    let mut scan_stack = vec![root_path];
    let group_file_name = OsStr::new(user_defined::GroupFile::TOML_FILE_NAME);
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
    Ok(groups
        .into_iter()
        .map(|(path, group, dirs, files)| parsed::GroupFile::from_user(fs, path.as_ref(), group))
        .collect::<Vec<_>>())
}
