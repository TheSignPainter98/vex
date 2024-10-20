use std::{
    fs::{self, File},
    io::Read,
};

use crate::{
    error::{Error, IOAction},
    result::Result,
    source_path::PrettyPath,
};
use camino::{Utf8Path, Utf8PathBuf};
use log::{info, log_enabled};

pub trait ScriptSource {
    fn path(&self) -> &Utf8Path;
    fn content(&self) -> Result<String>;
}

#[derive(Debug)]
pub struct FileSource {
    load_path: Utf8PathBuf,
    path: Utf8PathBuf,
}

impl FileSource {
    pub fn new(load_path: Utf8PathBuf, path: Utf8PathBuf) -> Self {
        Self { load_path, path }
    }
}

impl ScriptSource for FileSource {
    fn path(&self) -> &Utf8Path {
        &self.load_path
    }

    fn content(&self) -> Result<String> {
        let io_error = |cause| Error::IO {
            path: PrettyPath::new(&self.path),
            action: IOAction::Read,
            cause,
        };

        let metadata = fs::symlink_metadata(&self.path).map_err(io_error)?;
        if !metadata.is_file() && log_enabled!(log::Level::Info) {
            info!("ignoring {}: not a regular file", self.path)
        }

        let mut file = File::open(&self.path).map_err(io_error)?;
        let mut content = String::with_capacity(metadata.len().try_into().expect("file too large"));
        file.read_to_string(&mut content).map_err(io_error)?;
        Ok(content)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    pub struct TestSource<S: AsRef<str>> {
        pub path: S,
        pub content: S,
    }

    impl<S: AsRef<str>> ScriptSource for TestSource<S> {
        fn path(&self) -> &Utf8Path {
            self.path.as_ref().into()
        }

        fn content(&self) -> Result<String> {
            Ok(self.content.as_ref().to_owned())
        }
    }
}
