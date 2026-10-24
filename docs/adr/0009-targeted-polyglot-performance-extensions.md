# ADR 0009 — Targeted polyglot performance extensions (Zig, DuckDB, Mojo, Nim)

- Status: Accepted
- Date: 2026-10-21
- Related: [Polyglot Extensions Specification](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/architecture/polyglot-extensions.md), [ADR 0002](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0002-shared-rust-core-boundary.md), [ADR 0005](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0005-platform-abstraction-and-realtime-transport.md), [ADR 0006](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0006-storage-secrets-and-configuration.md), [ADR 0007](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0007-generated-type-contracts.md), [ADR 0008](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0008-data-source-governance-and-network-egress.md), [Performance Specification](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/performance/specification.md)

## Problem

Lavender’s backend is standardized on Rust ([ADR 0002](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0002-shared-rust-core-boundary.md)), which serves as the core facade, stream hub, and network egress layer. However, specialized sub-domains introduce distinct physical constraints where a pure single-language Rust implementation produces undesirable compromises:

1. **Client-side edge DSP and high-density orbital mechanics**: Demodulating 2.0 MSPS raw RF I/Q streams from WebUSB RTL-SDR dongles and propagating 25,000+ satellite orbits at 60 fps in the browser require micro-kernels with zero standard library runtime, zero allocator overhead, and direct SIMD128 memory access. Standard Rust WASM (`wasm-bindgen` + allocator) introduces 200–400 KiB binaries and marshalling friction.
2. **Columnar historical spatiotemporal analytics**: Running sub-second analytical queries over multi-gigabyte Parquet archives (e.g., detecting "dark targets"—vessels disabling AIS transponders or exhibiting kinematic anomalies near EEZ boundaries) requires a vectorized columnar OLAP engine with spatial SQL capabilities. Hand-rolling a columnar execution engine in Rust is cost-prohibitive, while hosting an external database daemon (PostgreSQL/PostGIS, ClickHouse) violates Lavender’s zero-daemon desktop deployment model.
3. **Multi-camera computer vision and ALPR OCR**: Real-time decoding of 16+ CCTV RTSP video streams, SIMD frame preprocessing, neural target detection (YOLO/RT-DETR), and optical character recognition (ALPR) require heterogeneous hardware acceleration (Apple Silicon AMX/NEON, NVIDIA TensorRT/CUDA, Intel AVX-512) without Python’s Global Interpreter Lock (GIL). Isolating these compute kernels prevents GPU driver crashes from killing the core.
4. **Compile-time tactical alerting and Complex Event Processing (CEP)**: Tactical operators need a high-level, expressive DSL to define real-time spatial and kinematic triggers (e.g., speed violations near submarine cables). Traditional scripting engines introduce stop-the-world garbage collection pauses (10–100 ms) that violate real-time budgets, while writing dynamic rules directly in Rust requires full binary recompilation.

Lavender requires a principled architecture for targeted polyglot extensions that provides optimal execution performance without degrading system reliability or multiplying operational complexity.

## Options

### 1. Monolithic Pure Rust Across All Domains
Implement SDR demodulation in Rust WASM, build a custom Parquet columnar engine in Rust, write vision pipelines in pure Rust (`tract` / `ort`), and implement an in-tree interpreted AST rule engine for alerts.
- *Pros*: Single toolchain (`cargo`), uniform language, single CI build step.
- *Cons*: Bloated client-side WASM modules; prohibitive development cost re-implementing a spatial OLAP query planner; lack of mature ML compiler infrastructure in pure Rust compared to MLIR; interpreted rule engines suffer high overhead per event.

### 2. Unrestricted Polyglot Microservices
Adopt standard industry stacks for each domain as independent networked microservices (e.g., Python for CV, ClickHouse for analytics, Elixir/BEAM for stream handling, Julia for orbital math).
- *Pros*: Developers use familiar ecosystems without custom FFI.
- *Cons*: Total breakdown of Lavender’s desktop distribution (Tauri); excessive memory footprints (hundreds of megabytes per runtime); serialization overhead over loopback sockets; severe violation of [ADR 0002](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0002-shared-rust-core-boundary.md) and [ADR 0008](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0008-data-source-governance-and-network-egress.md).

### 3. Targeted Polyglot Extensions with Strict Zero-Cost Boundaries (Selected)
Surgically adopt specialized languages/engines strictly where their runtime characteristics solve domain-specific physical constraints, bound by zero-cost, type-safe, sandboxed interfaces:
- **Zig** for freestanding WASM micro-kernels (WebUSB SDR & SGP4) and lightweight edge SDR daemons.
- **Embedded DuckDB + Spatial SQL** in `lavender-core` for columnar Parquet analytics and dark target detection.
- **Mojo** for the `lavender-vision` sidecar (CCTV decoding, YOLOv11, ALPR OCR via MLIR SIMD).
- **Nim** for compile-time tactical alerting DSL & CEP engine (ARC/ORC zero GC pauses, compiled to a native C-ABI shared library).

## Decision

Adopt **Option 3**. The targeted polyglot components are incorporated into the system topology under the strict boundaries specified below.

### 1. Freestanding WASM & Edge SDR: Zig
- **WebUSB ADS-B Demodulator**: Zig compiles to `wasm32-freestanding` with `-O ReleaseFast` without linking libc or wasi, yielding micro-kernels of 12–35 KiB. Raw 2.0 MSPS I/Q samples transferred via WebUSB are processed in a dedicated Web Worker. Manchester decoding, preamble correlation, CRC-24 verification, and Compact Position Reporting (CPR) even/odd frame decoding execute with zero heap allocation using SIMD128 intrinsics.
- **SGP4 Orbit Propagator**: Batch orbital propagation of 25,000 satellites executes via Structure-of-Arrays (SoA) memory in Zig WASM, yielding geodetic coordinates directly into a `SharedArrayBuffer` bound to Cesium vertex buffers without intermediate JavaScript object instantiation.
- **Edge SDR Daemon (`lavender-sdr-edge`)**: A standalone, static Musl binary (< 6.5 MiB RSS) runs on low-power edge nodes (e.g., Raspberry Pi) capturing direct RF from RTL-SDR dongles and streaming parsed telemetry inward to `lavender-core` via authenticated TLS WebSockets ([ADR 0008](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0008-data-source-governance-and-network-egress.md)).

### 2. Embedded Columnar Analytics: DuckDB + Spatial SQL
- Embedded directly into `lavender-core` via C FFI (`duckdb-rs`). No external database daemon or open network port.
- Historical vessel and aircraft tracks are stored in partitioned Parquet files under `$LAVENDER_DATA_DIR/analytics/tracks/`.
- Vectorized execution engine runs spatial window queries over millions of coordinates in milliseconds, executing dark target detection, transponder gap analysis, and kinematics-based AIS spoofing detection.
- Results stream back to `lavender-core` as zero-copy Apache Arrow `RecordBatch` streams.

### 3. Vision Perception Sidecar: Mojo (`lavender-vision`)
- Deployed as an isolated companion process (`lavender-vision`) supervised by `lavender-core`.
- Eliminates Python’s Global Interpreter Lock (GIL) to achieve true concurrent decoding of 16–64 RTSP camera feeds.
- Leverages MLIR (Multi-Level Intermediate Representation) compiler optimizations to dispatch SIMD operations directly to Apple Silicon AMX/NEON, Intel AVX-512, and NVIDIA Tensor Cores.
- Computes multi-class object detection (YOLOv11/RT-DETR), optical ALPR character recognition, and Reid visual embeddings.
- Communicates with `lavender-core` over **Apache Arrow Flight** via local Unix Domain Sockets (`/tmp/lavender-vision.sock`), providing zero-copy columnar data transfer and crash isolation.

### 4. Tactical Alerting DSL & Complex Event Processing: Nim
- Intelligence analysts declare tactical alerting rules in a domain-specific macro language.
- Nim’s compile-time AST macros lower rules directly into branchless, optimized C machine code with spatial indexing baked in.
- Memory management uses **ARC/ORC** (`--mm:orc`), inserting deterministic destructors at compile-time and guaranteeing zero stop-the-world garbage collection pauses.
- Compiles to a native dynamic library (`liblavender_cep.dylib` / `.so`), loaded into `lavender-core` via `libloading`. Rulesets can be recompiled and hot-reloaded dynamically without restarting the server or terminating client connections.

### 5. Explicit Rejection of Alternative Stacks

| Candidate | Primary Reason for Rejection |
|---|---|
| **Elixir / BEAM** | **Redundant with Tokio; heavy ERTS footprint**. Tokio in `lavender-core` already provides scalable M:N green-thread concurrency, watch channels, and task hierarchies. Packaging the Erlang Runtime System (ERTS) into Tauri inflates binary installers by 45 MiB and adds 35–60 MiB base memory, violating desktop resource budgets ($\le 150\,\text{MiB}$ total RSS). Erlang NIF crashes terminate the scheduler. |
| **Julia** | **Severe JIT latency ("Time-to-first-plot") and massive runtime overhead**. Julia incurs 5–20 second JIT pauses on cold execution paths. The runtime (`libjulia` + sysimage) consumes 200–500 MiB RSS before processing data, and its tracing mark-and-sweep GC pauses (10–100 ms) violate the 16.6 ms frame rendering budget. Furthermore, Julia cannot compile to freestanding sub-50 KiB browser WASM. |

### 6. Strict Zero-Cost Boundary Interfaces

| Boundary | Mechanism | Data Representation | Copy Cost |
|---|---|---|---|
| **Zig WASM $\leftrightarrow$ Web Frontend** | `SharedArrayBuffer` + WebAssembly Linear Memory | Raw byte slice $\to$ Packed `AdsBRecord` struct | Zero-copy pointer offsets |
| **Nim CEP $\leftrightarrow$ Rust Core** | In-Process C-ABI (`libloading`) | Flat `#[repr(C)]` structs | Direct C function pointer call ($\sim 1.5\,\text{ns}$) |
| **Mojo Vision $\leftrightarrow$ Rust Core** | Apache Arrow Flight IPC (Unix Domain Socket) | Arrow Columnar `RecordBatch` | Zero-copy memory-mapped IPC buffers |
| **DuckDB $\leftrightarrow$ Rust Core** | In-Process C FFI (`duckdb-rs`) | Partitioned Parquet $\to$ Arrow Array Streams | Zero-copy vectorized pushdown |

### 7. Governance and Contract Compatibility
- **ADR 0002 Compliance**: `LavenderCore` remains the single orchestrator and facade. Adapters (Axum and Tauri) never call polyglot extensions directly.
- **ADR 0008 Compliance**: No extension accesses external networks directly. Vision feeds (CCTV/ALPR) remain marked `privacy: Sensitive` and disabled by default.
- **ADR 0007 Compliance**: All output event types (`TacticalAlert`, `VisionDetection`, `HistoricalTrackQuery`) are defined in `lavender-domain` with `serde` and `ts-rs` derive, ensuring continuous synchronization with frontend TypeScript bindings.

## Consequences

### Positive
- Sub-millisecond spatiotemporal analytics directly on historical Parquet data without external database infrastructure.
- In-browser SDR capability allowing users to plug in RTL-SDR dongles via WebUSB and track aircraft in real-time without backend dependencies.
- High-performance multi-camera CCTV perception and ALPR without Python GIL bottlenecks or CUDA crash propagation.
- Zero-pause, deterministic complex event processing with dynamic ruleset reloading.
- Minimal client and desktop memory footprint; zero runtime bloat from heavy virtual machines (BEAM, Julia).

### Negative / Trade-offs
- Build environment complexity increases: CI and developer workstations require `zig`, `duckdb`, `mojo`, and `nim` compilers for full-tree builds (mitigated by optional build flags where extensions are precompiled or stubbed for basic UI development).
- Cross-language debugging requires proficiency with native profiling tools (lldb, Instruments, Valgrind, Tracy) across multiple language ABIs.
- FFI boundaries require strict ABI stability reviews and automated integration tests in CI.
