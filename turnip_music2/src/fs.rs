use std::{ffi::OsStr, fmt::Debug, hash::Hash};

use crate::data_model::{
    native_metadata::{NativeMetadata, NativeMetadataFormat},
    user_defined::{ConfigFile, GroupFile},
};

pub trait FsPathBuf<Path: ?Sized>:
    Clone + Hash + Debug + PartialEq + Eq + PartialOrd + Ord
{
    fn parse_path_from_str(s: &str) -> Self;
    /// Return the path, having added one or more components as parsed from the argument.
    fn joined(self, p: &str) -> Self;
}

/// Minimal trait encoding only the necessary components of a filesystem scanner.
pub trait Fs {
    type Path: ?Sized + ToOwned<Owned = Self::PathBuf> + AsRef<Self::Path>;
    type PathBuf: AsRef<Self::Path> + FsPathBuf<Self::Path>;

    fn read_dir<'s, P: AsRef<Self::Path>>(
        &'s self,
        path: P,
    ) -> anyhow::Result<impl Iterator<Item = anyhow::Result<Self::PathBuf>>>;
    fn path_trailing<'p>(&self, path: &'p Self::Path) -> Option<&'p OsStr>;
    fn path_ext<'p>(&self, path: &'p Self::Path) -> Option<&'p OsStr>;
    fn path_parent_dir<'p>(&self, path: &'p Self::Path) -> Option<Self::PathBuf>;
    fn is_file<P: AsRef<Self::Path>>(&self, path: P) -> bool;
    fn is_dir<P: AsRef<Self::Path>>(&self, path: P) -> bool;
    fn strip_prefix<'a, P: AsRef<Self::Path>>(
        &self,
        path_buf: &'a Self::PathBuf,
        prefix: P,
    ) -> anyhow::Result<&'a Self::Path>;

    // TODO parsers should use warnings not anyhow
    fn parse_native_metadata<P: AsRef<Self::Path>>(
        &self,
        path: P,
    ) -> anyhow::Result<NativeMetadata>;
    fn parse_config_file<P: AsRef<Self::Path>>(
        &self,
        path: P,
    ) -> anyhow::Result<(toml_edit::DocumentMut, ConfigFile)>;
    fn parse_group_file<P: AsRef<Self::Path>>(
        &self,
        path: P,
    ) -> anyhow::Result<(toml_edit::DocumentMut, GroupFile)>;

    fn write_toml_file<P: AsRef<Self::Path>>(
        &mut self,
        path: P,
        doc: toml_edit::DocumentMut,
    ) -> anyhow::Result<()>;
}

pub struct StdFs;
impl FsPathBuf<std::path::Path> for std::path::PathBuf {
    fn parse_path_from_str(s: &str) -> Self {
        // This parses multiple path components from the string, instead of creating a single
        std::path::PathBuf::from(s)
    }

    fn joined(mut self, p: &str) -> Self {
        std::path::PathBuf::push(&mut self, std::path::PathBuf::from(p));
        self
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

    fn path_parent_dir<'p>(&self, path: &'p Self::Path) -> Option<Self::PathBuf> {
        path.parent().map(|p| p.to_owned())
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
    fn parse_config_file<P: AsRef<Self::Path>>(
        &self,
        path: P,
    ) -> anyhow::Result<(toml_edit::DocumentMut, ConfigFile)> {
        let data = std::fs::read_to_string(path)?;
        ConfigFile::from_str(&data)
    }
    fn parse_group_file<P: AsRef<Self::Path>>(
        &self,
        path: P,
    ) -> anyhow::Result<(toml_edit::DocumentMut, GroupFile)> {
        let data = std::fs::read_to_string(path)?;
        GroupFile::from_str(&data)
    }

    fn write_toml_file<P: AsRef<Self::Path>>(
        &mut self,
        path: P,
        doc: toml_edit::DocumentMut,
    ) -> anyhow::Result<()> {
        std::fs::write(path.as_ref(), doc.to_string().as_bytes())?;
        Ok(())
    }
}
