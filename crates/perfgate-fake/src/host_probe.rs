//! Fake host probe for deterministic testing.

use perfgate_adapters::{HostProbe, HostProbeOptions};
use perfgate_types::HostInfo;
use std::sync::{Arc, Mutex};

/// A host probe that returns pre-configured host information.
///
/// This is useful for testing code that depends on [`HostProbe`] without
/// actually querying system information.
///
/// # Thread Safety
///
/// All configuration methods are `&self` (not `&mut self`), making it safe
/// to share a single instance across multiple threads in tests.
///
/// # Example
///
/// ```
/// use perfgate_fake::FakeHostProbe;
/// use perfgate_adapters::{HostProbe, HostProbeOptions};
/// use perfgate_types::HostInfo;
///
/// let probe = FakeHostProbe::new()
///     .with_os("linux")
///     .with_arch("x86_64")
///     .with_cpu_count(8)
///     .with_memory_bytes(16 * 1024 * 1024 * 1024);
///
/// let options = HostProbeOptions { include_hostname_hash: false };
/// let info = probe.probe(&options);
///
/// assert_eq!(info.os, "linux");
/// assert_eq!(info.arch, "x86_64");
/// assert_eq!(info.cpu_count, Some(8));
/// ```
#[derive(Debug, Clone)]
pub struct FakeHostProbe {
    inner: Arc<Mutex<FakeHostProbeInner>>,
}

#[derive(Debug, Clone)]
struct FakeHostProbeInner {
    os: String,
    arch: String,
    cpu_count: Option<u32>,
    memory_bytes: Option<u64>,
    hostname_hash: Option<String>,
}

impl Default for FakeHostProbe {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeHostProbe {
    /// Create a new `FakeHostProbe` with default values.
    ///
    /// Defaults:
    /// - `os`: "unknown"
    /// - `arch`: "unknown"
    /// - `cpu_count`: `None`
    /// - `memory_bytes`: `None`
    /// - `hostname_hash`: `None` (always, unless explicitly set)
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(FakeHostProbeInner {
                os: "unknown".to_string(),
                arch: "unknown".to_string(),
                cpu_count: None,
                memory_bytes: None,
                hostname_hash: None,
            })),
        }
    }

    /// Create a `FakeHostProbe` that mimics a specific platform.
    ///
    /// # Example
    ///
    /// ```
    /// use perfgate_fake::FakeHostProbe;
    ///
    /// // Create a probe that looks like a Linux system
    /// let probe = FakeHostProbe::platform("linux", "x86_64", 8, 16 * 1024 * 1024 * 1024);
    /// ```
    pub fn platform(os: &str, arch: &str, cpu_count: u32, memory_bytes: u64) -> Self {
        Self::new()
            .with_os(os)
            .with_arch(arch)
            .with_cpu_count(cpu_count)
            .with_memory_bytes(memory_bytes)
    }

    /// Set the OS string.
    pub fn with_os(self, os: &str) -> Self {
        self.inner.lock().expect("lock").os = os.to_string();
        self
    }

    /// Set the architecture string.
    pub fn with_arch(self, arch: &str) -> Self {
        self.inner.lock().expect("lock").arch = arch.to_string();
        self
    }

    /// Set the CPU count.
    pub fn with_cpu_count(self, count: u32) -> Self {
        self.inner.lock().expect("lock").cpu_count = Some(count);
        self
    }

    /// Set the memory in bytes.
    pub fn with_memory_bytes(self, bytes: u64) -> Self {
        self.inner.lock().expect("lock").memory_bytes = Some(bytes);
        self
    }

    /// Set the hostname hash directly.
    ///
    /// Note: This overrides the `include_hostname_hash` option behavior.
    /// If set, this value is always returned.
    pub fn with_hostname_hash(self, hash: &str) -> Self {
        self.inner.lock().expect("lock").hostname_hash = Some(hash.to_string());
        self
    }

    /// Set CPU count to `None`.
    pub fn without_cpu_count(self) -> Self {
        self.inner.lock().expect("lock").cpu_count = None;
        self
    }

    /// Set memory to `None`.
    pub fn without_memory(self) -> Self {
        self.inner.lock().expect("lock").memory_bytes = None;
        self
    }
}

impl HostProbe for FakeHostProbe {
    fn probe(&self, options: &HostProbeOptions) -> HostInfo {
        let inner = self.inner.lock().expect("lock");

        HostInfo {
            os: inner.os.clone(),
            arch: inner.arch.clone(),
            cpu_count: inner.cpu_count,
            memory_bytes: inner.memory_bytes,
            hostname_hash: if options.include_hostname_hash {
                inner.hostname_hash.clone()
            } else {
                None
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_probe_has_defaults() {
        let probe = FakeHostProbe::new();
        let info = probe.probe(&HostProbeOptions::default());

        assert_eq!(info.os, "unknown");
        assert_eq!(info.arch, "unknown");
        assert!(info.cpu_count.is_none());
        assert!(info.memory_bytes.is_none());
        assert!(info.hostname_hash.is_none());
    }

    #[test]
    fn with_methods_configure_values() {
        let probe = FakeHostProbe::new()
            .with_os("linux")
            .with_arch("x86_64")
            .with_cpu_count(8)
            .with_memory_bytes(16 * 1024 * 1024 * 1024);

        let info = probe.probe(&HostProbeOptions::default());

        assert_eq!(info.os, "linux");
        assert_eq!(info.arch, "x86_64");
        assert_eq!(info.cpu_count, Some(8));
        assert_eq!(info.memory_bytes, Some(16 * 1024 * 1024 * 1024));
    }

    #[test]
    fn hostname_hash_respects_option() {
        let probe = FakeHostProbe::new().with_hostname_hash("abc123");

        let info_without = probe.probe(&HostProbeOptions {
            include_hostname_hash: false,
        });
        assert!(info_without.hostname_hash.is_none());

        let info_with = probe.probe(&HostProbeOptions {
            include_hostname_hash: true,
        });
        assert_eq!(info_with.hostname_hash, Some("abc123".to_string()));
    }

    #[test]
    fn platform_convenience_constructor() {
        let probe = FakeHostProbe::platform("macos", "aarch64", 10, 32 * 1024 * 1024 * 1024);
        let info = probe.probe(&HostProbeOptions::default());

        assert_eq!(info.os, "macos");
        assert_eq!(info.arch, "aarch64");
        assert_eq!(info.cpu_count, Some(10));
        assert_eq!(info.memory_bytes, Some(32 * 1024 * 1024 * 1024));
    }

    #[test]
    fn without_methods_clear_values() {
        let probe = FakeHostProbe::new()
            .with_cpu_count(8)
            .with_memory_bytes(1024)
            .without_cpu_count()
            .without_memory();

        let info = probe.probe(&HostProbeOptions::default());

        assert!(info.cpu_count.is_none());
        assert!(info.memory_bytes.is_none());
    }

    #[test]
    fn thread_safe_sharing() {
        use std::sync::Arc;
        use std::thread;

        let probe = Arc::new(FakeHostProbe::new().with_os("linux"));

        let handles: Vec<_> = (0..4)
            .map(|_| {
                let p = probe.clone();
                thread::spawn(move || {
                    let info = p.probe(&HostProbeOptions::default());
                    assert_eq!(info.os, "linux");
                })
            })
            .collect();

        for h in handles {
            h.join().unwrap();
        }
    }
}
