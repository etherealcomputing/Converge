use std::path::Path;
use std::process::Command;

fn converge() -> String {
    std::env::var("CARGO_BIN_EXE_converge").unwrap_or_else(|_| {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/debug/converge")
            .to_string_lossy()
            .to_string()
    })
}

fn example(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples")
        .join(name)
        .to_string_lossy()
        .to_string()
}

fn perturb(args: &[&str]) -> std::process::Output {
    Command::new(converge())
        .arg("perturb")
        .args(args)
        .output()
        .expect("run converge perturb")
}

#[test]
fn perturb_cli_runs() {
    let out = perturb(&[&example("envelope.cv"), "--steps", "4", "--bisect", "2"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json = String::from_utf8(out.stdout).expect("utf8");
    assert!(json.contains("\"envelope_version\""));
    assert!(json.contains("\"operator\": \"prune\""));
}

// The envelope is only evidence if it's reproducible. Two invocations of the binary have to
// produce the same bytes.
#[test]
fn perturb_output_is_byte_identical_across_runs() {
    let args = [example("envelope.cv"), "--steps".into(), "4".into()];
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let a = perturb(&args);
    let b = perturb(&args);
    assert!(a.status.success());
    assert_eq!(a.stdout, b.stdout);
}

#[test]
fn perturb_accepts_an_operator_subset() {
    let out = perturb(&[
        &example("envelope.cv"),
        "--steps",
        "2",
        "--operators",
        "prune,gain.attenuate",
    ]);
    assert!(out.status.success());
    let json = String::from_utf8(out.stdout).expect("utf8");
    assert!(json.contains("\"operator\": \"prune\""));
    assert!(json.contains("\"operator\": \"gain.attenuate\""));
    assert!(!json.contains("\"operator\": \"spike_jitter\""));
}

#[test]
fn perturb_rejects_an_unknown_operator() {
    let out = perturb(&[&example("envelope.cv"), "--operators", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("unknown operator `nope`"), "{stderr}");
}

#[test]
fn perturb_reports_validation_errors() {
    let out = perturb(&[&example("invalid_unknown_neuron.cv")]);
    assert_eq!(out.status.code(), Some(1));
}
