use std::env;
use std::fs;
use std::time::Instant;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: perfgate-selfbench <command>");
        eprintln!("Commands: noop, cpu-fixed, io-fixed, json-read");
        std::process::exit(1);
    }

    match args[1].as_str() {
        "noop" => Ok(()),
        "cpu-fixed" => cpu_fixed(),
        "io-fixed" => io_fixed(),
        "json-read" => json_read(args.get(2).map(|s| s.as_str())),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            std::process::exit(1);
        }
    }
}

fn cpu_fixed() -> anyhow::Result<()> {
    let start = Instant::now();
    let mut sum = 0u64;
    // Perform a fixed amount of CPU work
    for i in 0..10_000_000 {
        sum = sum.wrapping_add(i);
    }
    println!("CPU work complete: {}", sum);
    eprintln!("Duration: {:?}", start.elapsed());
    Ok(())
}

fn io_fixed() -> anyhow::Result<()> {
    let start = Instant::now();
    let tmp_dir = std::env::temp_dir();
    let path = tmp_dir.join("perfgate-selfbench-workload.bin");

    // Write 1MB of data
    let data = vec![0u8; 1024 * 1024];
    fs::write(&path, &data)?;

    // Read it back
    let read = fs::read(&path)?;
    assert_eq!(read.len(), data.len());

    // Clean up
    let _ = fs::remove_file(&path);

    eprintln!("IO work complete. Duration: {:?}", start.elapsed());
    Ok(())
}

fn json_read(path: Option<&str>) -> anyhow::Result<()> {
    let start = Instant::now();
    let content = if let Some(p) = path {
        fs::read_to_string(p)?
    } else {
        // Default small JSON
        r#"{"foo": "bar", "count": 123, "active": true}"#.to_string()
    };

    let _val: serde_json::Value = serde_json::from_str(&content)?;
    eprintln!("JSON work complete. Duration: {:?}", start.elapsed());
    Ok(())
}
