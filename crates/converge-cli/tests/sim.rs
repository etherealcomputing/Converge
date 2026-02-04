use std::process::Command;

#[test]
fn sim_cli_runs() {
    let exe = std::env::var("CARGO_BIN_EXE_converge").unwrap_or_else(|_| {
        let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest
            .join("../../target/debug/converge")
            .to_string_lossy()
            .to_string()
    });
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let example = manifest.join("../../examples/poisson.cv");
    let output = Command::new(exe)
        .args(["sim", example.to_string_lossy().as_ref()])
        .output()
        .expect("run converge sim");
    assert!(output.status.success());
}
