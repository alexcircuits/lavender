//! Nim Complex Event Processing (CEP) foreign function interface (FFI) host.
//!
//! Exposes the safe host integration layer for evaluating high-frequency kinematic
//! telemetry against compiled AST tactical alerting rules (`liblavender_rules.dylib`).

#![allow(unsafe_code)]

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::sink::AnalyticsError;

/// Function pointer type matching Nim C-ABI rule evaluator signature:
/// `proc evaluate_event(event: ptr KinematicEventC, alert: ptr AlertResultC): bool`
pub type EvaluateFn = unsafe extern "C" fn(*const KinematicEventC, *mut AlertResultC) -> bool;

/// C-ABI compliant kinematic telemetry event passed into the CEP engine.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct KinematicEventC {
    /// 32-byte null-padded or exact entity identifier (e.g., ICAO hex or MMSI).
    pub entity_id: [u8; 32],
    /// Latitude in WGS84 decimal degrees.
    pub lat: f64,
    /// Longitude in WGS84 decimal degrees.
    pub lon: f64,
    /// Altitude in meters above mean sea level.
    pub altitude_m: f32,
    /// Speed over ground in knots.
    pub speed_knots: f32,
    /// True heading or track angle in degrees (0.0 .. 360.0).
    pub heading_deg: f32,
    /// UTC timestamp in seconds since Unix epoch.
    pub timestamp_epoch_s: i64,
    /// Transponder active flag (false = dark target / AIS or ADS-B disabled).
    pub is_transponder_on: bool,
}

impl KinematicEventC {
    /// Constructs a new C-ABI kinematic event.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        entity_id: &str,
        lat: f64,
        lon: f64,
        altitude_m: f32,
        speed_knots: f32,
        heading_deg: f32,
        timestamp_epoch_s: i64,
        is_transponder_on: bool,
    ) -> Self {
        let mut id_buf = [0u8; 32];
        let bytes = entity_id.as_bytes();
        let len = bytes.len().min(32);
        id_buf[..len].copy_from_slice(&bytes[..len]);

        Self {
            entity_id: id_buf,
            lat,
            lon,
            altitude_m,
            speed_knots,
            heading_deg,
            timestamp_epoch_s,
            is_transponder_on,
        }
    }

    /// Returns the entity ID as a UTF-8 string slice, stripping trailing null bytes.
    #[must_use]
    pub fn entity_id_str(&self) -> &str {
        let len = self.entity_id.iter().position(|&b| b == 0).unwrap_or(self.entity_id.len());
        std::str::from_utf8(&self.entity_id[..len]).unwrap_or("")
    }
}

/// C-ABI compliant alert result buffer populated by the CEP rule engine.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AlertResultC {
    /// Identifier of the fired rule (e.g. 101, 102).
    pub rule_id: u32,
    /// Alert severity: 0 = Info, 1 = Low, 2 = Warning, 3 = Critical.
    pub severity: u8,
    /// Detection confidence score (0.0 to 1.0).
    pub confidence: f32,
    /// Null-terminated human-readable alert message.
    pub message: [u8; 128],
}

impl Default for AlertResultC {
    fn default() -> Self {
        Self { rule_id: 0, severity: 0, confidence: 0.0, message: [0u8; 128] }
    }
}

impl AlertResultC {
    /// Creates a populated alert result struct.
    #[must_use]
    pub fn new(rule_id: u32, severity: u8, confidence: f32, msg: &str) -> Self {
        let mut message = [0u8; 128];
        let bytes = msg.as_bytes();
        let len = bytes.len().min(127);
        message[..len].copy_from_slice(&bytes[..len]);

        Self { rule_id, severity, confidence, message }
    }

    /// Returns the message as a UTF-8 string slice.
    #[must_use]
    pub fn message_str(&self) -> &str {
        let len = self.message.iter().position(|&b| b == 0).unwrap_or(self.message.len());
        std::str::from_utf8(&self.message[..len]).unwrap_or("")
    }
}

/// Normalized domain representation of a tactical alert raised by the CEP engine.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TacticalAlert {
    pub rule_id: u32,
    pub severity: u8,
    pub confidence: f32,
    pub message: String,
    pub entity_id: String,
    pub timestamp_epoch_s: i64,
}

impl TacticalAlert {
    #[must_use]
    pub fn rule_id(&self) -> u32 {
        self.rule_id
    }

    #[must_use]
    pub fn severity(&self) -> u8 {
        self.severity
    }

    #[must_use]
    pub fn confidence(&self) -> f32 {
        self.confidence
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn entity_id(&self) -> &str {
        &self.entity_id
    }
}

/// Deterministic built-in fallback evaluator matching rule specifications:
/// - Rule 101: Supersonic low-altitude event (speed >= 600 kt, altitude <= 2000m)
/// - Rule 102: Dark target speed drift (transponder disabled while underway, speed > 2.0 kt)
///
/// # Safety
/// Pointers must be valid and aligned for reads/writes.
pub unsafe extern "C" fn fallback_evaluate_event(
    event: *const KinematicEventC,
    result: *mut AlertResultC,
) -> bool {
    if event.is_null() || result.is_null() {
        return false;
    }
    let e = unsafe { &*event };
    let r = unsafe { &mut *result };

    // Rule 101: Supersonic low-altitude trajectory
    // Speed >= Mach 1 (~660 kt, or >= 600.0 kt) at low altitude (altitude <= 2000.0 m)
    if e.speed_knots >= 600.0 && e.altitude_m <= 2000.0 && e.altitude_m >= 0.0 {
        *r = AlertResultC::new(
            101,
            3, // Critical
            0.98,
            "Supersonic low-altitude event detected",
        );
        return true;
    }

    // Rule 102: Dark target speed drift
    // Transponder disabled while entity is underway (> 2.0 knots)
    if !e.is_transponder_on && e.speed_knots > 2.0 {
        *r = AlertResultC::new(
            102,
            2, // Warning
            0.92,
            "Dark target speed drift detected",
        );
        return true;
    }

    false
}

#[cfg(unix)]
struct DynamicLibrary {
    handle: *mut std::ffi::c_void,
}

#[cfg(unix)]
unsafe impl Send for DynamicLibrary {}
#[cfg(unix)]
unsafe impl Sync for DynamicLibrary {}

#[cfg(unix)]
impl DynamicLibrary {
    fn load(path: &Path) -> Result<Self, AnalyticsError> {
        use std::ffi::{CString, c_char, c_int};

        const RTLD_LAZY: c_int = 1;
        const RTLD_LOCAL: c_int = 4;

        unsafe extern "C" {
            fn dlopen(filename: *const c_char, flags: c_int) -> *mut std::ffi::c_void;
            fn dlerror() -> *const c_char;
        }

        let c_path = CString::new(path.to_str().unwrap_or(""))
            .map_err(|e| AnalyticsError::Engine(format!("invalid dynamic library path: {e}")))?;

        let handle = unsafe { dlopen(c_path.as_ptr(), RTLD_LAZY | RTLD_LOCAL) };
        if handle.is_null() {
            let err_msg = unsafe {
                let err_ptr = dlerror();
                if err_ptr.is_null() {
                    "unknown dlopen error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(err_ptr).to_string_lossy().into_owned()
                }
            };
            return Err(AnalyticsError::Engine(format!(
                "failed to load library {}: {err_msg}",
                path.display()
            )));
        }

        Ok(Self { handle })
    }

    fn get_symbol<T>(&self, symbol: &str) -> Result<T, AnalyticsError> {
        use std::ffi::{CString, c_char};

        unsafe extern "C" {
            fn dlsym(handle: *mut std::ffi::c_void, symbol: *const c_char)
            -> *mut std::ffi::c_void;
            fn dlerror() -> *const c_char;
        }

        let c_sym = CString::new(symbol)
            .map_err(|e| AnalyticsError::Engine(format!("invalid symbol name: {e}")))?;

        let ptr = unsafe { dlsym(self.handle, c_sym.as_ptr()) };
        if ptr.is_null() {
            let err_msg = unsafe {
                let err_ptr = dlerror();
                if err_ptr.is_null() {
                    "symbol not found".to_string()
                } else {
                    std::ffi::CStr::from_ptr(err_ptr).to_string_lossy().into_owned()
                }
            };
            return Err(AnalyticsError::Engine(format!(
                "failed to resolve symbol {symbol}: {err_msg}"
            )));
        }

        unsafe { Ok(std::mem::transmute_copy(&ptr)) }
    }
}

#[cfg(unix)]
impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe extern "C" {
                fn dlclose(handle: *mut std::ffi::c_void) -> std::ffi::c_int;
            }
            unsafe {
                let _ = dlclose(self.handle);
            }
        }
    }
}

#[cfg(not(unix))]
struct DynamicLibrary;

#[cfg(not(unix))]
impl DynamicLibrary {
    fn load(_path: &Path) -> Result<Self, AnalyticsError> {
        Err(AnalyticsError::Engine(
            "dynamic library loading is only supported on POSIX/Unix targets".into(),
        ))
    }
    fn get_symbol<T>(&self, _symbol: &str) -> Result<T, AnalyticsError> {
        Err(AnalyticsError::Engine(
            "dynamic library loading is only supported on POSIX/Unix targets".into(),
        ))
    }
}

/// Safe host wrapper managing CEP tactical rule evaluation.
///
/// Can dynamically load `liblavender_rules.dylib` (or `.so`) if present via dynamic linking,
/// or seamlessly provide a deterministic fallback evaluator when running without native libraries.
#[derive(Clone)]
pub struct NimRuleEngine {
    eval_fn: EvaluateFn,
    source_name: String,
    lib_handle: Option<Arc<DynamicLibrary>>,
}

impl NimRuleEngine {
    /// Creates an engine utilizing the built-in deterministic fallback evaluator.
    #[must_use]
    pub fn fallback() -> Self {
        Self {
            eval_fn: fallback_evaluate_event,
            source_name: "builtin-fallback".to_string(),
            lib_handle: None,
        }
    }

    /// Creates an engine with an explicitly injected C-ABI evaluation function pointer.
    #[must_use]
    pub fn from_fn(eval_fn: EvaluateFn, source_name: impl Into<String>) -> Self {
        Self { eval_fn, source_name: source_name.into(), lib_handle: None }
    }

    /// Attempts to dynamically load the dynamic library at `path` and lookup the `evaluate_event` symbol.
    ///
    /// # Errors
    /// Returns `AnalyticsError::Engine` if the library cannot be opened or if the symbol is missing.
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, AnalyticsError> {
        let path = path.as_ref();
        let lib = DynamicLibrary::load(path)?;
        let eval_fn: EvaluateFn = lib.get_symbol("evaluate_event")?;
        Ok(Self {
            eval_fn,
            source_name: path.display().to_string(),
            lib_handle: Some(Arc::new(lib)),
        })
    }

    /// Discovers and loads `liblavender_rules.dylib` (or `.so`) if present,
    /// otherwise seamlessly defaults to the deterministic fallback evaluator.
    #[must_use]
    pub fn load_or_fallback() -> Self {
        let candidates = [
            std::env::var("LAVENDER_RULES_LIB").ok().map(PathBuf::from),
            Some(PathBuf::from("liblavender_rules.dylib")),
            Some(PathBuf::from("liblavender_rules.so")),
            Some(PathBuf::from("kernels/nim/liblavender_rules.dylib")),
            Some(PathBuf::from("kernels/nim/liblavender_rules.so")),
            Some(PathBuf::from("/usr/local/lib/liblavender_rules.dylib")),
            Some(PathBuf::from("/usr/local/lib/liblavender_rules.so")),
        ];

        for candidate in candidates.into_iter().flatten() {
            if !candidate.exists() {
                continue;
            }
            if let Ok(engine) = Self::load_from_path(&candidate) {
                tracing::info!(path = %candidate.display(), "Loaded dynamic CEP rule engine");
                return engine;
            }
        }

        Self::fallback()
    }

    /// Description or path of the active evaluation source.
    #[must_use]
    pub fn source_name(&self) -> &str {
        &self.source_name
    }

    /// Returns `true` if the engine is running on the built-in fallback evaluator.
    #[must_use]
    pub fn is_fallback(&self) -> bool {
        self.lib_handle.is_none()
    }

    /// Evaluates a kinematic event against configured CEP rules.
    /// Returns `Some(TacticalAlert)` if any rule is triggered, `None` otherwise.
    #[must_use]
    pub fn evaluate(&self, event: &KinematicEventC) -> Option<TacticalAlert> {
        let mut alert_c = AlertResultC::default();
        let triggered = unsafe { (self.eval_fn)(std::ptr::from_ref(event), &raw mut alert_c) };
        if triggered {
            Some(TacticalAlert {
                rule_id: alert_c.rule_id,
                severity: alert_c.severity,
                confidence: alert_c.confidence,
                message: alert_c.message_str().to_string(),
                entity_id: event.entity_id_str().to_string(),
                timestamp_epoch_s: event.timestamp_epoch_s,
            })
        } else {
            None
        }
    }
}

impl Default for NimRuleEngine {
    fn default() -> Self {
        Self::load_or_fallback()
    }
}

impl fmt::Debug for NimRuleEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NimRuleEngine")
            .field("source_name", &self.source_name)
            .field("is_fallback", &self.is_fallback())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, offset_of, size_of};

    #[test]
    fn c_abi_struct_layout_and_alignment() {
        // KinematicEventC layout verification
        assert_eq!(size_of::<KinematicEventC>(), 80);
        assert_eq!(align_of::<KinematicEventC>(), 8);
        assert_eq!(offset_of!(KinematicEventC, entity_id), 0);
        assert_eq!(offset_of!(KinematicEventC, lat), 32);
        assert_eq!(offset_of!(KinematicEventC, lon), 40);
        assert_eq!(offset_of!(KinematicEventC, altitude_m), 48);
        assert_eq!(offset_of!(KinematicEventC, speed_knots), 52);
        assert_eq!(offset_of!(KinematicEventC, heading_deg), 56);
        assert_eq!(offset_of!(KinematicEventC, timestamp_epoch_s), 64);
        assert_eq!(offset_of!(KinematicEventC, is_transponder_on), 72);

        // AlertResultC layout verification
        assert_eq!(size_of::<AlertResultC>(), 140);
        assert_eq!(align_of::<AlertResultC>(), 4);
        assert_eq!(offset_of!(AlertResultC, rule_id), 0);
        assert_eq!(offset_of!(AlertResultC, severity), 4);
        assert_eq!(offset_of!(AlertResultC, confidence), 8);
        assert_eq!(offset_of!(AlertResultC, message), 12);
    }

    #[test]
    fn supersonic_low_altitude_event_triggers_rule_101() {
        let engine = NimRuleEngine::fallback();

        // Supersonic (750 kt > 600 kt, Mach ~1.13) at 850m low altitude
        let event =
            KinematicEventC::new("MIG-31B", 55.75, 37.61, 850.0, 750.0, 180.0, 1_700_000_000, true);

        let alert = engine.evaluate(&event);
        assert!(alert.is_some(), "Expected rule 101 to trigger for supersonic low altitude");

        let alert = alert.unwrap();
        assert_eq!(alert.rule_id, 101);
        assert_eq!(alert.entity_id, "MIG-31B");
        assert_eq!(alert.severity, 3); // Critical
        assert!(alert.confidence >= 0.95);
        assert!(alert.message.contains("Supersonic low-altitude"));
    }

    #[test]
    fn dark_target_speed_drift_triggers_rule_102() {
        let engine = NimRuleEngine::fallback();

        // Dark target: transponder off (false) moving at 15.4 knots
        let event = KinematicEventC::new(
            "DARK-VESSEL-88",
            24.5,
            56.8,
            0.0,
            15.4,
            120.0,
            1_700_000_100,
            false,
        );

        let alert = engine.evaluate(&event);
        assert!(alert.is_some(), "Expected rule 102 to trigger for dark target drift");

        let alert = alert.unwrap();
        assert_eq!(alert.rule_id, 102);
        assert_eq!(alert.entity_id, "DARK-VESSEL-88");
        assert_eq!(alert.severity, 2); // Warning
        assert!(alert.confidence >= 0.90);
        assert!(alert.message.contains("Dark target"));
    }

    #[test]
    fn normal_traffic_does_not_trigger_alerts() {
        let engine = NimRuleEngine::fallback();

        // Normal airliner at 11,000m cruising at 460 knots with transponder active
        let normal_plane =
            KinematicEventC::new("BAW249", 51.5, -0.1, 11_000.0, 460.0, 270.0, 1_700_000_000, true);
        assert!(engine.evaluate(&normal_plane).is_none());

        // Stationary vessel with transponder off (parked/docked at 0 knots)
        let stationary_vessel =
            KinematicEventC::new("TUG-01", 51.9, 4.4, 0.0, 0.0, 0.0, 1_700_000_000, false);
        assert!(engine.evaluate(&stationary_vessel).is_none());
    }

    #[test]
    fn dynamic_function_injection() {
        // Custom evaluator function verifying function pointer injection
        unsafe extern "C" fn custom_eval(
            event: *const KinematicEventC,
            result: *mut AlertResultC,
        ) -> bool {
            let e = unsafe { &*event };
            let r = unsafe { &mut *result };
            if e.speed_knots > 999.0 {
                *r = AlertResultC::new(999, 3, 1.0, "Hypersonic detection");
                true
            } else {
                false
            }
        }

        let engine = NimRuleEngine::from_fn(custom_eval, "custom-hypersonic-evaluator");
        assert_eq!(engine.source_name(), "custom-hypersonic-evaluator");

        let sub_hypersonic = KinematicEventC::new("TEST", 0.0, 0.0, 1000.0, 500.0, 0.0, 0, true);
        assert!(engine.evaluate(&sub_hypersonic).is_none());

        let hypersonic = KinematicEventC::new("TEST", 0.0, 0.0, 1000.0, 1200.0, 0.0, 0, true);
        let alert = engine.evaluate(&hypersonic).unwrap();
        assert_eq!(alert.rule_id, 999);
        assert_eq!(alert.message, "Hypersonic detection");
    }
}
