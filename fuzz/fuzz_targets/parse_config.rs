#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Parse as TOML config - only attempt if valid UTF-8
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = toml::from_str::<perfgate_types::ConfigFile>(s);
    }
});
