//! Crash-recovery marker file. While sleep prevention is active, this file
//! exists. On helper startup, if the file exists and no client connects
//! within a grace period, we restore normal sleep behavior.
#![allow(dead_code)] // wired in Task 14

use anyhow::Result;
use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;

const MARKER_PATH: &str = "/Library/Application Support/open-lid/sleep-prevention.enabled";

pub struct OwnershipMarker {
    path: PathBuf,
}

impl OwnershipMarker {
    pub fn new() -> Self {
        Self {
            path: PathBuf::from(MARKER_PATH),
        }
    }

    #[cfg(test)]
    pub fn at(p: &Path) -> Self {
        Self { path: p.to_path_buf() }
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    pub fn write(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, b"")?;
        Ok(())
    }

    pub fn remove(&self) -> Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn write_then_exists_then_remove() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("marker.flag");
        let m = OwnershipMarker::at(&p);

        assert!(!m.exists());
        m.write().unwrap();
        assert!(m.exists());
        m.remove().unwrap();
        assert!(!m.exists());
    }

    #[test]
    fn remove_nonexistent_is_ok() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("never-existed");
        OwnershipMarker::at(&p).remove().unwrap();
    }

    #[test]
    fn write_creates_parent_directory() {
        let dir = tempdir().unwrap();
        let p = dir.path().join("nested").join("path").join("marker.flag");
        OwnershipMarker::at(&p).write().unwrap();
        assert!(p.exists());
    }
}
