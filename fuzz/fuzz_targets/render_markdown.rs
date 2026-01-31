#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(compare) = serde_json::from_slice::<perfgate_types::CompareReceipt>(data) {
        let _ = perfgate_app::render_markdown(&compare);
    }
});
