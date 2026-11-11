//! Cross-process resource locks. Lock files are never unlinked: deleting an inode
//! while a peer holds it would let a new opener bypass that peer's lock.
//! OS locks are released on drop, cancellation, panic and process death; no TTL.

use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct Coordination {
    root: PathBuf,
}

#[derive(Debug)]
pub struct ResourceGuard {
    _file: File,
}

impl Default for Coordination {
    fn default() -> Self {
        Self::at(
            dirs::data_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("Nuphus/nuphus-mcp/locks-v2"),
        )
    }
}

impl Coordination {
    /// A separate root is useful for isolated integration tests. All cooperating
    /// production clients MUST use the same root.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn machine(&self, exclusive: bool) -> Result<ResourceGuard, String> {
        self.acquire("machine", exclusive)
    }

    pub fn browser(&self, identity: &str, exclusive: bool) -> Result<ResourceGuard, String> {
        self.acquire(&format!("browser:{identity}"), exclusive)
    }

    pub fn startup(&self, profile: &Path) -> Result<ResourceGuard, String> {
        self.acquire(&format!("startup:{}", profile.display()), true)
    }

    /// Held for the whole task's attachment to a tab, not one tool call.
    pub fn tab(&self, browser: &str, target: &str) -> Result<ResourceGuard, String> {
        self.acquire(&format!("tab:{browser}:{target}"), true)
    }

    fn acquire(&self, resource: &str, exclusive: bool) -> Result<ResourceGuard, String> {
        std::fs::create_dir_all(&self.root).map_err(|e| format!("lock directory: {e}"))?;
        let name = format!("{:x}.lock", Sha256::digest(resource.as_bytes()));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.root.join(name))
            .map_err(|e| format!("open resource lock: {e}"))?;
        let result = if exclusive {
            file.try_lock()
        } else {
            file.try_lock_shared()
        };
        result.map_err(|e| match e {
            std::fs::TryLockError::WouldBlock => format!(
                "Automation is busy: resource '{resource}' is in use by another session. \
                 For a tab, select an unclaimed target_id or open a new tab; otherwise retry later."),
            std::fs::TryLockError::Error(e) => format!("resource lock failed: {e}"),
        })?;
        Ok(ResourceGuard { _file: file })
    }
}
