//! Cross-process serialization for the managed model store (RFC-050 §5).

use crate::generation::ModelStoreProfileId;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub const MODEL_STORE_LOCK_FILE: &str = ".model-store.lock";
const LOCK_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Canonical binding between one managed profile and its model-store root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedModelStore {
    models_dir: PathBuf,
    profile_id: ModelStoreProfileId,
}

impl ManagedModelStore {
    pub fn default_embedding(models_dir: impl Into<PathBuf>) -> Self {
        Self {
            models_dir: models_dir.into(),
            profile_id: ModelStoreProfileId::default_embedding(),
        }
    }

    pub fn models_dir(&self) -> &Path {
        &self.models_dir
    }

    pub fn profile_id(&self) -> &ModelStoreProfileId {
        &self.profile_id
    }

    pub fn acquire_shared(
        &self,
        timeout: Duration,
    ) -> Result<ModelStoreMutationGuard<SharedAccess>, ModelStoreLockError> {
        ModelStoreMutationGuard::acquire_shared(&self.models_dir, self.profile_id.clone(), timeout)
    }

    pub fn acquire_exclusive(
        &self,
        timeout: Duration,
    ) -> Result<ModelStoreMutationGuard<ExclusiveAccess>, ModelStoreLockError> {
        ModelStoreMutationGuard::acquire_exclusive(
            &self.models_dir,
            self.profile_id.clone(),
            timeout,
        )
    }
}

#[derive(Debug)]
pub struct SharedAccess;

#[derive(Debug)]
pub struct ExclusiveAccess;

#[derive(Debug)]
pub struct ModelStoreMutationGuard<Mode> {
    file: File,
    lock_path: PathBuf,
    profile_id: ModelStoreProfileId,
    _mode: PhantomData<Mode>,
}

impl ModelStoreMutationGuard<SharedAccess> {
    pub fn acquire_shared(
        models_dir: &Path,
        profile_id: ModelStoreProfileId,
        timeout: Duration,
    ) -> Result<Self, ModelStoreLockError> {
        acquire(models_dir, profile_id, timeout, LockMode::Shared)
    }
}

impl ModelStoreMutationGuard<ExclusiveAccess> {
    pub fn acquire_exclusive(
        models_dir: &Path,
        profile_id: ModelStoreProfileId,
        timeout: Duration,
    ) -> Result<Self, ModelStoreLockError> {
        acquire(models_dir, profile_id, timeout, LockMode::Exclusive)
    }
}

impl<Mode> ModelStoreMutationGuard<Mode> {
    pub fn profile_id(&self) -> &ModelStoreProfileId {
        &self.profile_id
    }

    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }
}

impl<Mode> Drop for ModelStoreMutationGuard<Mode> {
    fn drop(&mut self) {
        #[cfg(any(unix, windows))]
        {
            let _ = fs4::FileExt::unlock(&self.file);
        }
    }
}

#[derive(Debug)]
pub enum ModelStoreLockError {
    Io(std::io::Error),
    Timeout,
    UnsupportedTarget,
}

impl fmt::Display for ModelStoreLockError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "model-store lock I/O failed: {error}"),
            Self::Timeout => formatter.write_str("model-store lock acquisition timed out"),
            Self::UnsupportedTarget => {
                formatter.write_str("model-store locking is unsupported on this target")
            }
        }
    }
}

impl std::error::Error for ModelStoreLockError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Timeout | Self::UnsupportedTarget => None,
        }
    }
}

impl From<std::io::Error> for ModelStoreLockError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

#[derive(Debug, Clone, Copy)]
enum LockMode {
    Shared,
    Exclusive,
}

fn acquire<Mode>(
    models_dir: &Path,
    profile_id: ModelStoreProfileId,
    timeout: Duration,
    mode: LockMode,
) -> Result<ModelStoreMutationGuard<Mode>, ModelStoreLockError> {
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (models_dir, profile_id, timeout, mode);
        return Err(ModelStoreLockError::UnsupportedTarget);
    }

    #[cfg(any(unix, windows))]
    {
        let lock_path = models_dir.join(MODEL_STORE_LOCK_FILE);
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        let deadline = Instant::now()
            .checked_add(timeout)
            .ok_or(ModelStoreLockError::Timeout)?;
        loop {
            let result = match mode {
                LockMode::Shared => fs4::FileExt::try_lock_shared(&file),
                LockMode::Exclusive => fs4::FileExt::try_lock(&file),
            };
            match result {
                Ok(()) => {
                    return Ok(ModelStoreMutationGuard {
                        file,
                        lock_path,
                        profile_id,
                        _mode: PhantomData,
                    });
                }
                Err(fs4::TryLockError::Error(error)) => {
                    return Err(ModelStoreLockError::Io(error));
                }
                Err(fs4::TryLockError::WouldBlock) => {
                    let now = Instant::now();
                    if now >= deadline {
                        return Err(ModelStoreLockError::Timeout);
                    }
                    std::thread::sleep(
                        LOCK_POLL_INTERVAL.min(deadline.saturating_duration_since(now)),
                    );
                }
            }
        }
    }
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;
    use std::process::{Child, Command, Stdio};

    const CHILD_ACTION: &str = "ORBOK_TEST_LOCK_CHILD_ACTION";
    const CHILD_MODELS_DIR: &str = "ORBOK_TEST_LOCK_MODELS_DIR";
    const CHILD_MARKER: &str = "ORBOK_TEST_LOCK_MARKER";

    #[test]
    fn cross_process_lock_child() {
        let Ok(action) = std::env::var(CHILD_ACTION) else {
            return;
        };
        let models_dir = PathBuf::from(std::env::var(CHILD_MODELS_DIR).unwrap());
        let marker = PathBuf::from(std::env::var(CHILD_MARKER).unwrap());
        let profile = ModelStoreProfileId::default_embedding();
        match action.as_str() {
            "shared-once" => {
                let _guard = ModelStoreMutationGuard::acquire_shared(
                    &models_dir,
                    profile,
                    Duration::from_secs(2),
                )
                .unwrap();
                std::fs::write(marker, b"locked").unwrap();
            }
            "expect-shared-timeout" => {
                let result = ModelStoreMutationGuard::acquire_shared(
                    &models_dir,
                    profile,
                    Duration::from_millis(100),
                );
                assert!(matches!(result, Err(ModelStoreLockError::Timeout)));
            }
            "expect-exclusive-timeout" => {
                let result = ModelStoreMutationGuard::acquire_exclusive(
                    &models_dir,
                    profile,
                    Duration::from_millis(100),
                );
                assert!(matches!(result, Err(ModelStoreLockError::Timeout)));
            }
            "exclusive-hold" => {
                let _guard = ModelStoreMutationGuard::acquire_exclusive(
                    &models_dir,
                    profile,
                    Duration::from_secs(2),
                )
                .unwrap();
                std::fs::write(marker, b"locked").unwrap();
                loop {
                    std::thread::sleep(Duration::from_millis(50));
                }
            }
            other => panic!("unknown lock child action: {other}"),
        }
    }

    #[test]
    fn separate_process_lock_mode_matrix_is_enforced() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("child-ready");
        let profile = ModelStoreProfileId::default_embedding();
        let shared = ModelStoreMutationGuard::acquire_shared(
            temp.path(),
            profile.clone(),
            Duration::from_secs(1),
        )
        .unwrap();
        let status = spawn_child("shared-once", temp.path(), &marker)
            .wait()
            .unwrap();
        assert!(status.success());
        assert!(marker.exists());
        let status = spawn_child("expect-exclusive-timeout", temp.path(), &marker)
            .wait()
            .unwrap();
        assert!(status.success());
        drop(shared);

        let exclusive = ModelStoreMutationGuard::acquire_exclusive(
            temp.path(),
            profile,
            Duration::from_secs(1),
        )
        .unwrap();
        let status = spawn_child("expect-shared-timeout", temp.path(), &marker)
            .wait()
            .unwrap();
        assert!(status.success());
        let status = spawn_child("expect-exclusive-timeout", temp.path(), &marker)
            .wait()
            .unwrap();
        assert!(status.success());
        assert!(exclusive.lock_path().exists());
    }

    #[test]
    fn crashed_process_releases_lock_without_deleting_lock_file() {
        let temp = tempfile::tempdir().unwrap();
        let marker = temp.path().join("child-ready");
        let mut child = spawn_child("exclusive-hold", temp.path(), &marker);
        wait_for_marker(&mut child, &marker);
        child.kill().unwrap();
        child.wait().unwrap();

        let guard = ModelStoreMutationGuard::acquire_exclusive(
            temp.path(),
            ModelStoreProfileId::default_embedding(),
            Duration::from_secs(1),
        )
        .unwrap();
        assert!(guard.lock_path().exists());
        drop(guard);
        assert!(temp.path().join(MODEL_STORE_LOCK_FILE).exists());
    }

    fn spawn_child(action: &str, models_dir: &Path, marker: &Path) -> Child {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "model_store::tests::cross_process_lock_child",
                "--nocapture",
            ])
            .env(CHILD_ACTION, action)
            .env(CHILD_MODELS_DIR, models_dir)
            .env(CHILD_MARKER, marker)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap()
    }

    fn wait_for_marker(child: &mut Child, marker: &Path) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !marker.exists() {
            assert!(Instant::now() < deadline, "child did not acquire lock");
            assert!(child.try_wait().unwrap().is_none(), "child exited early");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}
