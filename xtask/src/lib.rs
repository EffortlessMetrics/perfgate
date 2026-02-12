use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

/// Returns the workspace root directory.
pub fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // Since xtask is at repo_root/xtask, parent is repo_root.
    manifest_dir.parent().expect("workspace root").to_path_buf()
}

/// Creates a unique temporary directory for tests.
pub fn unique_temp_dir(prefix: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    dir.push(format!("{}_{}_{}", prefix, std::process::id(), nanos));
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

fn cwd_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub fn lock_cwd() -> std::sync::MutexGuard<'static, ()> {
    match cwd_lock().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub fn with_temp_cwd<F: FnOnce(&PathBuf)>(f: F) {
    let _guard = lock_cwd();
    let original = std::env::current_dir().expect("current dir");
    let temp_dir = unique_temp_dir("perfgate_xtask");
    std::env::set_current_dir(&temp_dir).expect("set cwd");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&temp_dir)));

    std::env::set_current_dir(&original).expect("restore cwd");
    let _ = fs::remove_dir_all(&temp_dir);

    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}

pub fn with_repo_cwd<F: FnOnce()>(f: F) {
    let _guard = lock_cwd();
    let original = std::env::current_dir().expect("current dir");
    let root = repo_root();
    std::env::set_current_dir(&root).expect("set repo cwd");

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));

    std::env::set_current_dir(&original).expect("restore cwd");
    if let Err(panic) = result {
        std::panic::resume_unwind(panic);
    }
}
