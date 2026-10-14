//! Safe process diagnostics for adapters; reusable core construction never changes hooks.
use std::io::Write;

/// Installs the process panic hook before adapters process untrusted data.
/// Payloads/backtraces may contain secrets, so only source location is emitted.
/// Executables call this explicitly; embedded consumers opt in.
pub fn install_redacting_panic_hook() {
    std::panic::set_hook(Box::new(|info| {
        let mut stderr = std::io::stderr().lock();
        if let Some(location) = info.location() {
            let _ = writeln!(
                stderr,
                "Lavender: panic at {}:{} (payload suppressed)",
                location.file(),
                location.line()
            );
        } else {
            let _ = writeln!(stderr, "Lavender: panic (payload suppressed)");
        }
    }));
}
