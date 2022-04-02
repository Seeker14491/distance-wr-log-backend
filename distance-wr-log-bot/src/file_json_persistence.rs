use crate::{ChangelistEntry, LevelInfo};
use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fs::File;
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use tempfile::NamedTempFile;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct FileJsonPersistence {
    query_results_path: PathBuf,
    changelist_path: PathBuf,
}

impl FileJsonPersistence {
    pub fn new(
        query_results_path: impl Into<PathBuf>,
        changelist_path: impl Into<PathBuf>,
    ) -> Self {
        FileJsonPersistence {
            query_results_path: query_results_path.into(),
            changelist_path: changelist_path.into(),
        }
    }

    pub fn load_query_results(&self) -> Result<Vec<LevelInfo>, LoadError> {
        load_file(&self.query_results_path)
    }

    pub fn save_query_results(&self, query_results: &[LevelInfo]) -> Result<()> {
        save_file(query_results, &self.query_results_path)
    }

    pub fn load_changelist(&self) -> Result<Vec<ChangelistEntry>, LoadError> {
        load_file(&self.changelist_path)
    }

    pub fn save_changelist(&self, changelist: &[ChangelistEntry]) -> Result<()> {
        save_file(changelist, &self.changelist_path)
    }
}

fn load_file<T>(path: &Path) -> Result<T, LoadError>
where
    T: DeserializeOwned,
{
    match File::open(path) {
        Ok(mut handle) => {
            serde_json::from_reader(&mut handle).map_err(|e| LoadError::Other(e.into()))
        }
        Err(e) => {
            if let io::ErrorKind::NotFound = e.kind() {
                Err(LoadError::DoesNotExist)
            } else {
                Err(LoadError::Other(e.into()))
            }
        }
    }
}

fn save_file<T: Serialize + DeserializeOwned>(data: &[T], path: &Path) -> Result<()> {
    let serialized = serde_json::to_vec(&data)?;

    // Make sure the JSON we just generated is valid
    let _: Vec<T> =
        serde_json::from_slice(&serialized).context("the JSON we just generated is not valid")?;

    // Atomically update the file using a temporary file
    let mut tmp = NamedTempFile::new_in(path.parent().unwrap())?;
    tmp.write_all(&serialized)?;
    #[allow(unused_variables)]
    let file = tmp.persist(path)?;

    // Set appropriate file permissions on unix
    {
        #[allow(unused_mut)]
        let mut perms = file.metadata()?.permissions();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;

            perms.set_mode(0o644);
        }

        file.set_permissions(perms)?;
    }

    Ok(())
}

#[derive(Error, Debug)]
pub enum LoadError {
    #[error("The requested item does not exist.")]
    DoesNotExist,

    #[error("{0}")]
    Other(#[from] anyhow::Error),
}
