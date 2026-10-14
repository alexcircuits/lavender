//! Run the real hook in a child process, never mutate parallel tests' global panic hook.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
const CANARY: &str = "PANIC_PAYLOAD_SECRET_CANARY_9af73";

#[test]
#[ignore = "isolated child for redaction regression"]
fn panic_hook_child() {
    if std::env::var_os("LAVENDER_PANIC_HOOK_TEST").is_none() {
        return;
    }
    lavender_core::diagnostics::install_redacting_panic_hook();
    panic!("{CANARY}");
}

#[test]
fn actual_panic_payload_never_reaches_stdout_or_stderr() {
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", "panic_hook_child", "--nocapture"])
        .env("LAVENDER_PANIC_HOOK_TEST", "1")
        .env("RUST_BACKTRACE", "1")
        .output()
        .unwrap();
    assert!(!child.status.success());
    let stdout = String::from_utf8_lossy(&child.stdout);
    let stderr = String::from_utf8_lossy(&child.stderr);
    assert!(stderr.contains("payload suppressed"));
    assert!(!stdout.contains(CANARY));
    assert!(!stderr.contains(CANARY));
}
