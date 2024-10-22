use std::{
    fs::{self, File},
    io::Read,
};

use camino::{Utf8Path, Utf8PathBuf};
use log::{info, log_enabled};
use walkdir::WalkDir;

use crate::{
    error::{Error, IOAction},
    result::Result,
    source_path::PrettyPath,
};

pub trait ScriptSource {
    fn path(&self) -> &Utf8Path;
    fn content(&self) -> Result<String>;
}

#[derive(Clone, Debug)]
pub struct FileSource {
    load_path: Utf8PathBuf,
    real_path: Utf8PathBuf,
}

impl FileSource {
    pub fn new(load_path: Utf8PathBuf, real_path: Utf8PathBuf) -> Self {
        Self {
            load_path,
            real_path,
        }
    }
}

impl ScriptSource for FileSource {
    fn path(&self) -> &Utf8Path {
        &self.load_path
    }

    fn content(&self) -> Result<String> {
        let io_error = |cause| Error::IO {
            path: PrettyPath::new(&self.load_path),
            action: IOAction::Read,
            cause,
        };

        let metadata = fs::symlink_metadata(&self.real_path).map_err(io_error)?;
        if !metadata.is_file() && log_enabled!(log::Level::Info) {
            info!("ignoring {}: not a regular file", self.real_path)
        }

        let mut file = File::open(&self.real_path).map_err(io_error)?;
        let mut content = String::with_capacity(metadata.len().try_into().expect("file too large"));
        file.read_to_string(&mut content).map_err(io_error)?;
        Ok(content)
    }
}

pub fn sources_in_dir(dir_path: &Utf8Path) -> Result<Vec<FileSource>> {
    if !dir_path.is_dir() {
        return Err(Error::NoVexesDir(PrettyPath::new(dir_path)));
    }

    let dir_walker = WalkDir::new(dir_path)
        .sort_by_file_name()
        .min_depth(1) // Immediate children.
        .into_iter()
        .filter_entry(|entry| {
            entry.file_type().is_dir() || entry.path().extension().is_some_and(|ext| ext == "star")
        });
    let sources: Vec<_> = dir_walker
        .flatten() // Ignore inaccessible files.
        .filter(|entry| entry.file_type().is_file())
        .flat_map(|entry| Utf8PathBuf::try_from(entry.into_path()))
        .map(|path| {
            let load_path = path.strip_prefix(dir_path).unwrap_or(&path).to_owned();
            FileSource::new(load_path, path)
        })
        .collect();
    Ok(sources)
}

#[cfg(test)]
pub struct TestSource<P, C> {
    pub vex_dir: P,
    pub path: P,
    pub content: C,
}

#[cfg(test)]
impl<P, C> ScriptSource for TestSource<P, C>
where
    P: AsRef<Utf8Path>,
    C: AsRef<str>,
{
    fn path(&self) -> &Utf8Path {
        self.path
            .as_ref()
            .strip_prefix(self.vex_dir.as_ref())
            .unwrap()
    }

    fn content(&self) -> Result<String> {
        Ok(self.content.as_ref().to_owned())
    }
}

#[cfg(test)]
mod test {
    use std::io::Write;

    use regex::Regex;

    use crate::scriptlets::source;

    use super::*;

    #[test]
    fn no_vexes_dir() -> Result<()> {
        let tempdir = tempfile::tempdir().unwrap();
        let tempdir_path = Utf8PathBuf::try_from(tempdir.path().to_owned())?;

        let mut manifest = File::create(tempdir_path.join("vex.toml")).unwrap();
        manifest.write_all("[vex]".as_bytes()).unwrap();

        let re = Regex::new("^cannot find vexes directory at .*").unwrap();
        let err = source::sources_in_dir("i-do-not-exist".into()).unwrap_err();
        assert!(
            re.is_match(&err.to_string()),
            "incorrect error, expected {} but got {err}",
            re.as_str()
        );

        Ok(())
    }
}
