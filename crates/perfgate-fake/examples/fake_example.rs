//! Basic example demonstrating fake implementations for testing.
//!
//! Run with: cargo run -p perfgate-fake --example basic

use perfgate_adapters::{CommandSpec, ProcessRunner};
use perfgate_fake::{FakeProcessRunner, MockProcessBuilder};

fn main() {
    println!("=== perfgate-fake Basic Example ===\n");

    println!("1. Creating a fake process runner:");
    let runner = FakeProcessRunner::new();
    println!("   Created empty FakeProcessRunner");

    println!("\n2. Configuring mock results with MockProcessBuilder:");
    let success_result = MockProcessBuilder::new()
        .exit_code(0)
        .wall_ms(100)
        .stdout(b"hello world".to_vec())
        .stderr(b"".to_vec())
        .build();

    runner.set_result(&["echo", "hello"], success_result);
    println!("   Configured: echo hello -> exit 0, 100ms, stdout='hello world'");

    let slow_result = MockProcessBuilder::new()
        .exit_code(0)
        .wall_ms(500)
        .stdout(b"slow output".to_vec())
        .build();

    runner.set_result(&["slow", "command"], slow_result);
    println!("   Configured: slow command -> exit 0, 500ms");

    let failure_result = MockProcessBuilder::new()
        .exit_code(1)
        .wall_ms(50)
        .stderr(b"error: something went wrong".to_vec())
        .build();

    runner.set_result(&["failing", "cmd"], failure_result);
    println!("   Configured: failing cmd -> exit 1, 50ms");

    println!("\n3. Running commands with the fake runner:");
    let spec1 = CommandSpec {
        argv: vec!["echo".to_string(), "hello".to_string()],
        cwd: None,
        env: vec![],
        timeout: None,
        output_cap_bytes: 1024,
    };

    match runner.run(&spec1) {
        Ok(result) => {
            println!("   echo hello:");
            println!("     Exit code: {}", result.exit_code);
            println!("     Wall time: {} ms", result.wall_ms);
            println!("     Stdout: {:?}", String::from_utf8_lossy(&result.stdout));
        }
        Err(e) => println!("   Error: {}", e),
    }

    let spec2 = CommandSpec {
        argv: vec!["failing".to_string(), "cmd".to_string()],
        cwd: None,
        env: vec![],
        timeout: None,
        output_cap_bytes: 1024,
    };

    match runner.run(&spec2) {
        Ok(result) => {
            println!("   failing cmd:");
            println!("     Exit code: {}", result.exit_code);
            println!("     Stderr: {:?}", String::from_utf8_lossy(&result.stderr));
        }
        Err(e) => println!("   Error: {}", e),
    }

    println!("\n4. Testing deterministic behavior:");
    let spec3 = CommandSpec {
        argv: vec!["slow".to_string(), "command".to_string()],
        cwd: None,
        env: vec![],
        timeout: None,
        output_cap_bytes: 1024,
    };

    let r1 = runner.run(&spec3).unwrap();
    let r2 = runner.run(&spec3).unwrap();
    println!("   First run: {} ms", r1.wall_ms);
    println!("   Second run: {} ms", r2.wall_ms);
    println!("   Same result: {}", r1.wall_ms == r2.wall_ms);

    println!("\n5. Default behavior for unconfigured commands:");
    let unknown_spec = CommandSpec {
        argv: vec!["unknown".to_string(), "command".to_string()],
        cwd: None,
        env: vec![],
        timeout: None,
        output_cap_bytes: 1024,
    };

    match runner.run(&unknown_spec) {
        Ok(result) => {
            println!("   Unknown command returned default:");
            println!("     Exit code: {}", result.exit_code);
            println!("     Wall time: {} ms", result.wall_ms);
        }
        Err(e) => println!("   Error: {}", e),
    }

    println!("\n6. Setting fallback result for all unconfigured commands:");
    let fallback_result = MockProcessBuilder::new()
        .exit_code(127)
        .wall_ms(1)
        .stderr(b"command not found".to_vec())
        .build();

    runner.set_fallback(fallback_result);
    println!("   Set fallback: exit 127, 1ms, stderr='command not found'");

    let another_unknown = CommandSpec {
        argv: vec!["another".to_string(), "unknown".to_string()],
        cwd: None,
        env: vec![],
        timeout: None,
        output_cap_bytes: 1024,
    };

    match runner.run(&another_unknown) {
        Ok(result) => {
            println!("   Another unknown:");
            println!("     Exit code: {}", result.exit_code);
            println!("     Stderr: {:?}", String::from_utf8_lossy(&result.stderr));
        }
        Err(e) => println!("   Error: {}", e),
    }

    println!("\n7. Configuring with CPU and memory metrics:");
    let detailed_result = MockProcessBuilder::new()
        .exit_code(0)
        .wall_ms(250)
        .cpu_ms(200)
        .max_rss_kb(4096)
        .stdout(b"detailed output".to_vec())
        .build();

    runner.set_result(&["detailed"], detailed_result);

    let detailed_spec = CommandSpec {
        argv: vec!["detailed".to_string()],
        cwd: None,
        env: vec![],
        timeout: None,
        output_cap_bytes: 1024,
    };

    if let Ok(result) = runner.run(&detailed_spec) {
        println!("   Detailed result:");
        println!("     Wall time: {} ms", result.wall_ms);
        println!("     CPU time: {:?} ms", result.cpu_ms);
        println!("     Max RSS: {:?} KB", result.max_rss_kb);
    }

    println!("\n=== Example complete ===");
}
