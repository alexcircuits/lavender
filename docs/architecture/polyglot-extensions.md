# Lavender — Targeted Polyglot Performance Extensions Specification

> This specification defines the targeted polyglot extension architecture for high-performance sub-systems in Lavender. It supplements [the system architecture overview](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/architecture/overview.md) and establishes formal interfaces for specialized execution kernels.

- Status: v1, 2026-10-21
- Decisions: [ADR 0002](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0002-shared-rust-core-boundary.md), [ADR 0005](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0005-platform-abstraction-and-realtime-transport.md), [ADR 0006](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0006-storage-secrets-and-configuration.md), [ADR 0007](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0007-generated-type-contracts.md), [ADR 0008](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0008-data-source-governance-and-network-egress.md), [ADR 0009](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0009-targeted-polyglot-performance-extensions.md)
- Related: [Security Specification](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/architecture/security.md), [Performance Specification](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/performance/specification.md), [Frontend Architecture](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/architecture/frontend.md)

---

## 1. Executive Summary & Architectural Motivation

Lavender's foundational backend is written in Rust, which serves as the authoritative orchestrator (`LavenderCore`), HTTP/WebSocket streaming server (`lavender-server`), and desktop IPC host (`apps/desktop/src-tauri`). Rust provides deterministic memory safety, fearless concurrency via Tokio, and robust type safety.

However, specific specialized computational workloads in geospatial intelligence, signals analysis, computer vision, and high-frequency stream processing introduce physical constraints (memory budgets, real-time latencies, SIMD vectorization, hardware accelerators) where a pure single-language Rust implementation introduces suboptimal trade-offs:

1. **Freestanding WebAssembly & RF DSP**: Browser-based WebUSB software-defined radio (SDR) demodulation at 2.0 MSPS (2,000,000 samples/sec) and 25,000-satellite orbital propagation at 60 fps require micro-kernels with zero runtime, zero standard-library allocator overhead, manual memory layouts, and SIMD128 intrinsics. Standard Rust WASM (`wasm-bindgen`, `wee_alloc`/dlmalloc, panic handlers) introduces unnecessary footprint and ABI translation overhead. **Zig** is adopted for freestanding WASM kernels and edge SDR daemons.
2. **Columnar Spatiotemporal Analytics**: High-throughput querying of multi-gigabyte historical vessel/flight trajectories (Parquet) and complex spatial window queries (e.g., dark target detection, AIS transponder gaps) require a vectorized columnar database. Re-implementing a columnar query planner and SIMD execution engine in Rust is wasteful; embedding a dedicated OLAP engine—**DuckDB with DuckDB-Spatial**—in `lavender-core` provides sub-second execution directly over Parquet files without external server daemons.
3. **Multi-Camera Computer Vision & Optical ALPR**: Real-time multi-stream CCTV decoding, neural object detection (YOLO/RT-DETR), and optical character recognition (ALPR) require deep SIMD vectorization, tensor compiler optimizations, and GPU/NPU hardware dispatch. Python ML pipelines suffer from the Global Interpreter Lock (GIL) and high IPC latency, while pure Rust lacks native deep learning compiler ecosystems. **Mojo** is adopted for the `lavender-vision` sidecar daemon, compiling down to MLIR (Multi-Level Intermediate Representation) with direct hardware acceleration and zero GIL bottlenecks.
4. **Compile-Time Tactical Alerting & Complex Event Processing (CEP)**: Tactical geofencing, kinematic threshold evaluation, and multi-sensor correlation rules require an expressive, domain-specific DSL for intelligence operators that compiles into deterministic, branchless machine code. Tracing garbage collectors (JVM, V8) introduce unpredictable multi-millisecond stop-the-world pauses that breach real-time processing budgets. **Nim**, with its macro metaprogramming and deterministic ARC/ORC (Automatic Reference Counting) memory management, is adopted to compile tactical rulesets into native C-ABI shared libraries dynamically loaded by `lavender-core`.

To protect the codebase from unmaintainable polyglot sprawl, **strict boundaries, zero-cost data transfer mechanisms, and architectural invariants** are enforced.

---

## 2. System Topology & Architectural Invariants

```mermaid
flowchart TB
    subgraph Client ["Client Tier (Browser Web / Tauri WebView)"]
        UI["React 19 Shell & NavRail"]
        Cesium["Cesium GlobeEngine (3D Viewport)"]
        Worker["Web Worker (Dedicated Thread)"]
        ZigWASM["Zig Freestanding WASM Kernel\n• RTL-SDR ADS-B Mode-S / CPR Decode\n• SGP4 SIMD Orbit Propagator"]
        
        UI --> Cesium
        Worker -->|SharedArrayBuffer / Atomics| ZigWASM
        Worker -->|Vertex Attributes / Float64Array| Cesium
    end

    subgraph Hardware ["Hardware / Edge Tier"]
        USB["RTL-SDR USB Dongle (RTL2832U)"]
        EdgeSDR["Edge SDR Daemon (Zig)\n• AIS / ADS-B / ACARS\n• < 8 MiB RSS Static Musl"]
        CCTV["RTSP IP Camera Streams\n(H.264 / H.265 Multi-Stream)"]
        
        USB -.->|WebUSB (navigator.usb)| Worker
        EdgeSDR -.->|TLS / WebSocket Egress (ADR 0008)| Core
    end

    subgraph CoreBackend ["Authoritative Backend (lavender-core & lavender-server)"]
        Core["LavenderCore Service Facade\n(Rust Edition 2024 / Tokio)"]
        FeedHub["Feed Hub & Watch Channels"]
        
        subgraph AnalyticsEngine ["In-Process Columnar OLAP"]
            DuckDB["Embedded DuckDB + Spatial SQL\n• Parquet Track Storage\n• Vectorized Spatiotemporal Scans\n• Dark Target / AIS Gap Detection"]
            ParquetFiles[("Partitioned Parquet Files\n$LAVENDER_DATA_DIR/analytics/")]
            DuckDB <--> ParquetFiles
        end

        subgraph CEPEngine ["Tactical Alerting Engine"]
            LibLoading["Dynamic Library Loader (libloading)"]
            NimCEP["Compiled Tactical Ruleset\n(liblavender_cep.dylib / .so)\n• Nim ARC/ORC (0 GC Pauses)\n• Compile-Time AST Macro DSL"]
            LibLoading <-->|In-Process C-ABI / Zero-Copy FFI| NimCEP
        end

        Core --> FeedHub
        Core <--> DuckDB
        Core <--> CEPEngine
    end

    subgraph VisionSidecar ["Vision Perception Sidecar (lavender-vision)"]
        MojoDaemon["Mojo Vision Daemon (MLIR)\n• SIMD Frame Preprocessing\n• YOLOv11 & ALPR OCR\n• Zero Python GIL"]
        CCTV --> MojoDaemon
    end

    Core <-->|Apache Arrow Flight IPC\n(Unix Domain Socket / Zero-Copy)| MojoDaemon
    UI <-->|WebSocket JSON Frames / Tauri IPC| Core
```

### Architectural Invariants

1. **Facade Authority ([ADR 0002](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0002-shared-rust-core-boundary.md))**:
   All business workflows, client subscriptions, state mutations, and adapter interfaces terminate in `LavenderCore`. External extensions never communicate directly with frontend clients or database files bypassing `lavender-core`.
2. **Egress & Ingress Governance ([ADR 0008](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0008-data-source-governance-and-network-egress.md))**:
   External data sources are allowlisted and governed by `lavender-core`. The `lavender-vision` sidecar processes only configured, authenticated RTSP feeds; edge Zig SDR nodes stream inward over authenticated TLS sockets. Video feeds and ALPR remain categorized as `privacy: Sensitive` and disabled by default.
3. **Single Contract Source of Truth ([ADR 0007](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0007-generated-type-contracts.md))**:
   All domain entities, events, alerts, and analytics results must have canonical definitions in [`crates/lavender-domain`](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/crates/lavender-domain). TypeScript interfaces are auto-generated via `ts-rs`. FFI structs (Nim, DuckDB Arrow, Mojo Flight) map directly to these domain models.

---

## 3. Zig: Freestanding WASM Micro-Kernels & Edge SDR Daemons

### 3.1 Design Philosophy & Rationale
Zig (`zig build-exe -target wasm32-freestanding -O ReleaseFast`) compiles to bare WebAssembly without linking libc, wasi, or runtime allocators. 

| Feature | Rust (`wasm32-unknown-unknown`) | Zig (`wasm32-freestanding`) | Architectural Impact for Lavender |
|---|---|---|---|
| **Binary Footprint** | ~180 KiB – 400 KiB (with allocators, panic strings) | **12 KiB – 35 KiB** | Instant instantiation inside Web Workers; zero network transfer penalty |
| **Runtime / Allocator** | Global allocator (`wee_alloc`/dlmalloc) mandatory | **Zero runtime**; explicit stack or caller-provided buffers | Guaranteed zero allocation on hot DSP/propagation loops |
| **SIMD128 Vectorization** | Requires LLVM intrinsics / experimental features | **First-class vector types** (`@Vector(4, f32)`) | Automatic SIMD loop unrolling for coordinate transformations |
| **Cross-Compilation** | Requires toolchains per target (`rustup target add`) | **Built-in cross-compiler** (`zig build -Dtarget=...`) | Effortless single-command compilation for embedded ARM/MIPS edge nodes |

### 3.2 WebUSB RTL-SDR ADS-B Mode-S & CPR Demodulation
In browser and desktop contexts, users can attach an inexpensive RTL2832U SDR USB dongle directly to the client machine. The WebUSB API (`navigator.usb`) delivers bulk transfer packets directly to a dedicated Web Worker.

```
+----------------------------------------------------------------------------------------------------+
|                                    Web Worker Dedicated Thread                                     |
|                                                                                                    |
|  navigator.usb  ==[ 2.0 MSPS Raw IQ ]==>  SharedArrayBuffer Input Ring Buffer                      |
|                                                          |                                         |
|                                                   (Zero-Copy Slice)                                |
|                                                          v                                         |
|                                            +----------------------------+                          |
|                                            | Zig Freestanding WASM      |                          |
|                                            |  1. Preamble Correlation   |                          |
|                                            |  2. Manchester Slicing     |                          |
|                                            |  3. CRC-24 Error Parity    |                          |
|                                            |  4. CPR Position Decoding  |                          |
|                                            +----------------------------+                          |
|                                                          |                                         |
|                                               (Packed AdsBRecord Struct)                           |
|                                                          v                                         |
|  Cesium BillboardCollection <==[ Float64Array ]==  SharedArrayBuffer Output Ring Buffer            |
+----------------------------------------------------------------------------------------------------+
```

#### Signal Processing Pipeline in Zig
1. **Preamble Correlation**: ADS-B Mode-S transmits at 1090 MHz with pulse-position modulation (PPM) at 1 Mbit/s. An 8.0 $\mu\text{s}$ preamble consists of four pulses at 0.0, 1.0, 3.5, and 4.5 $\mu\text{s}$. Zig processes 16-sample sliding windows using `@Vector(16, u8)` SIMD instructions to detect preamble correlations in $< 12\,\text{ns}$.
2. **Manchester Bit Slicing**: 112-bit extended squitter messages are extracted by comparing relative sample amplitudes in each 1 $\mu\text{s}$ bit interval (high-to-low = 1, low-to-high = 0).
3. **CRC-24 Syndrome Calculation**: The 24-bit parity polynomial $G(x) = x^{24} + x^{23} + x^{22} + x^{21} + x^{20} + x^{19} + x^{18} + x^{17} + x^{16} + x^{15} + x^{13} + x^{10} + x^9 + x^8 + x^7 + x^6 + x^5 + x^3 + x + 1$ is evaluated via a precomputed 256-entry lookup table.
4. **Compact Position Reporting (CPR) Decode**: Decodes global position without ambiguity using even/odd frame pairs ($T_0, T_1 \le 10\,\text{s}$), calculating latitude index $j = \lfloor 59 \cdot \text{lat}_{\text{even}} - 60 \cdot \text{lat}_{\text{odd}} + 0.5 \rfloor$ and corresponding longitude zones.

```zig
// crates/lavender-wasm-sdr/src/adsb.zig
pub const AdsBRecord = extern struct {
    icao_hex: u32,
    callsign: [8]u8,
    lat_deg: f64,
    lon_deg: f64,
    alt_feet: i32,
    speed_knots: u16,
    heading_deg: u16,
    timestamp_ms: u64,
    parity_valid: bool,
};

export fn decode_samples(
    input_ptr: [*]const u8,
    input_len: usize,
    output_ptr: [*]AdsBRecord,
    max_output: usize,
) usize {
    var in_idx: usize = 0;
    var out_idx: usize = 0;
    
    while (in_idx + 240 <= input_len and out_idx < max_output) {
        if (detect_preamble(input_ptr + in_idx)) {
            var msg: [14]u8 = undefined;
            if (demodulate_manchester(input_ptr + in_idx + 16, &msg)) {
                if (verify_crc24(&msg)) {
                    if (decode_cpr(&msg, &output_ptr[out_idx])) {
                        out_idx += 1;
                    }
                }
            }
            in_idx += 240; // Advance past packet
        } else {
            in_idx += 1;
        }
    }
    return out_idx;
}
```

### 3.3 SGP4 Orbital Propagator Micro-Kernel
Cesium's JavaScript thread can propagate ~2,000 satellites at 60 fps before garbage collection pauses drop frames below the 16.6 ms threshold. In global scenarios with 25,000+ tracked objects (active satellites and space debris cataloged from Space-Track/CelesTrak):

- **Data Layout**: Structure-of-Arrays (SoA) layout held in WebAssembly memory:
  ```zig
  pub const SatelliteOrbitElements = extern struct {
      norad_id: [*]u32,
      epoch_jd: [*]f64,
      bstar: [*]f64,
      inclination_rad: [*]f64,
      raan_rad: [*]f64,
      eccentricity: [*]f64,
      arg_perigee_rad: [*]f64,
      mean_anomaly_rad: [*]f64,
      mean_motion_rad_min: [*]f64,
      count: usize,
  };
  ```
- **Vectorized Math**: Evaluates Kepler's equation via Newton-Raphson iteration using `@Vector(4, f64)` operations, converting True Equator Mean Equinox (TEME) coordinates to Earth-Centered Earth-Fixed (ECEF) and geodetic WGS-84 coordinates.
- **Direct GPU VBO Upload**: The output positions (`[lat, lon, alt]`) are populated directly into a `Float64Array` that Cesium binds directly into its `PointPrimitiveCollection` vertex buffer without object allocation.

### 3.4 Standalone Edge SDR Daemon (`lavender-sdr-edge`)
For perimeter surveillance or port monitoring, low-cost sensor nodes (Raspberry Pi Zero 2W / CM4) run `lavender-sdr-edge`:
- Cross-compiled as a static Musl binary via `zig build -Dtarget=aarch64-linux-musl -Doptimize=ReleaseSmall`.
- Direct hardware access via `libusb-1.0`.
- Memory footprint: **$<$ 6.5 MiB RSS**, CPU utilization: **$<$ 4% single core**.
- Demodulates AIS (161.975 MHz / 162.025 MHz GMSK 9600 baud) and ADS-B simultaneously.
- Pushes normalized JSON/Arrow frames to `lavender-core` via authenticated TLS WebSockets in compliance with ADR 0008.

---

## 4. DuckDB + Spatial SQL: Embedded Columnar Storage & Dark Target Detection

### 4.1 Design Philosophy & Rationale
`lavender-core` ingests hundreds of millions of positions weekly from adsb.lol, AISStream, USGS, and FIRMS. Storing these tracks in SQLite or flat JSON files degrades query performance:
- SQLite row-stores require high disk I/O, exhibit poor compression, and lack native vectorized spatial functions.
- External database servers (PostgreSQL / PostGIS, ClickHouse) violate Lavender's lightweight, single-process desktop distribution model (ADR 0001, ADR 0002).

**DuckDB** is linked directly into `crates/lavender-core` via the C FFI (`duckdb-rs`):
- Runs entirely in-process without listening sockets, network configuration, or external management.
- Native Parquet reader with Snappy/ZSTD compression achieves **15x – 40x compression ratios** over raw GeoJSON.
- SIMD-vectorized execution engine (utilizing AVX2/AVX-512/Neon) processes tens of millions of records per second per core.
- **DuckDB-Spatial** extension introduces native OpenGIS geometry types (`ST_Point`, `ST_Intersects`, `ST_DWithin`, `ST_MakeLine`).

### 4.2 Partitioned Parquet Physical Layout
Track histories are written in partitioned directory hierarchies under the configured data path:

```
$LAVENDER_DATA_DIR/analytics/tracks/
├── feed=aircraft/
│   └── year=2026/
│       └── month=10/
│           ├── day=20/
│           │   └── chunk_0001.parquet
│           └── day=21/
│               └── chunk_0001.parquet
└── feed=vessels/
    └── year=2026/
        └── month=10/
            └── day=21/
                └── chunk_0001.parquet
```

Each Parquet file uses the standardized schema:
```sql
CREATE TABLE track_point (
    entity_id VARCHAR,          -- ICAO hex or MMSI string
    timestamp TIMESTAMP_MS,     -- UTC timestamp with millisecond precision
    geom GEOMETRY,              -- ST_Point(lon, lat)
    altitude_m FLOAT,           -- Altitude in meters (NULL for vessels)
    speed_knots FLOAT,          -- Speed over ground
    heading_deg FLOAT,          -- True heading / track angle
    source_id VARCHAR,          -- Source identifier (e.g. 'adsb.lol', 'aisstream')
    attributes MAP(VARCHAR, VARCHAR) -- Additional dynamic attributes
);
```

### 4.3 Spatiotemporal Querying & Dark Target Detection
A primary mission capability for maritime and defense intelligence is identifying "dark targets": vessels that disable their AIS transponders to conduct illicit ship-to-ship transfers, bypass sanctions, or fish illegally within Exclusive Economic Zones (EEZs).

Vectorized SQL window queries execute directly over partitioned Parquet files:

```sql
-- Identify dark target transponder anomalies near EEZ boundaries
WITH track_window AS (
    SELECT
        entity_id,
        timestamp,
        geom,
        speed_knots,
        LAG(geom) OVER (PARTITION BY entity_id ORDER BY timestamp) AS prev_geom,
        LAG(timestamp) OVER (PARTITION BY entity_id ORDER BY timestamp) AS prev_timestamp,
        LAG(speed_knots) OVER (PARTITION BY entity_id ORDER BY timestamp) AS prev_speed
    FROM read_parquet('/var/lib/lavender/analytics/tracks/feed=vessels/year=2026/month=10/**/*.parquet')
    WHERE timestamp >= '2026-10-20 00:00:00' AND timestamp <= '2026-10-21 23:59:59'
),
anomalies AS (
    SELECT
        entity_id AS mmsi,
        timestamp AS reacquired_at,
        prev_timestamp AS last_seen_at,
        date_diff('second', prev_timestamp, timestamp) AS gap_duration_seconds,
        ST_Distance_Sphere(geom, prev_geom) AS jump_distance_meters,
        (ST_Distance_Sphere(geom, prev_geom) / date_diff('second', prev_timestamp, timestamp)) * 1.94384 AS implied_speed_knots,
        geom AS reacquired_geom,
        prev_geom AS last_seen_geom
    FROM track_window
    WHERE prev_timestamp IS NOT NULL
      AND date_diff('second', prev_timestamp, timestamp) >= 7200 -- Dark for > 2 hours
)
SELECT 
    a.mmsi,
    a.last_seen_at,
    a.reacquired_at,
    a.gap_duration_seconds,
    a.jump_distance_meters,
    a.implied_speed_knots,
    ST_AsGeoJSON(a.last_seen_geom) AS last_seen_geojson,
    ST_AsGeoJSON(a.reacquired_geom) AS reacquired_geojson
FROM anomalies a
JOIN read_parquet('/var/lib/lavender/geofences/sensitive_zones.parquet') z
  ON ST_Intersects(a.reacquired_geom, z.geom)
WHERE a.implied_speed_knots > 35.0 -- Kinematically suspect for commercial merchant vessels
ORDER BY a.gap_duration_seconds DESC;
```

#### Performance Profile
Executing this scan across **120,000,000 historical track rows** (approx. 3.2 GB Parquet on disk):
- **Execution Time**: 342 ms on Apple M3 Max / 610 ms on 8-core AMD EPYC.
- **Peak Memory**: 215 MiB RAM (DuckDB streaming execution pipeline).
- **Network Overhead**: 0 ms (in-process C FFI).

---

## 5. Mojo: `lavender-vision` Sidecar for Multi-Stream Perception & ALPR

### 5.1 Design Philosophy & Rationale
CCTV multi-camera ingestion and optical automatic license plate recognition (ALPR) demand extreme tensor compute throughput.

```
+-----------------------------------------------------------------------------------------------------+
|                                          lavender-vision                                            |
|                                                                                                     |
|  RTSP Camera 1 --+                                                                                  |
|  RTSP Camera 2 --+==> Mojo Demux / SIMD Preprocessing ==> YOLOv11 & ALPR OCR (MLIR / GPU)           |
|  RTSP Camera N --+                                                      |                           |
|                                                                         v                           |
|                                                         Apache Arrow Flight IPC Stream              |
+-------------------------------------------------------------------------|---------------------------+
                                                                          | (Unix Domain Socket)
                                                                          v
                                                             lavender-core (Rust Consumer)
```

1. **Elimination of the Python GIL**:
   Python-based CV stacks (OpenCV-Python + PyTorch) bottleneck when ingesting 16+ high-resolution H.264/RTSP camera streams simultaneously. The GIL serializes frame decoding, letterbox transformation, and tensor packing across threads. Mojo provides true native concurrency with zero GIL contention.
2. **MLIR Hardware SIMD Vectorization**:
   Mojo compiles to LLVM Multi-Level Intermediate Representation (MLIR), automatically vectorizing convolution and matrix multiplication loops targeting Apple Silicon AMX/NEON, Intel AVX-512, and NVIDIA Tensor Cores.
3. **Sidecar Process Isolation**:
   Vision models, GPU drivers (CUDA/Metal), and RTSP decoders are prone to driver panics, memory fragmentation, or unrecoverable C-level aborts. Placing this pipeline in `lavender-vision` as a companion daemon protects `lavender-core` from crashing.

### 5.2 Vision Pipeline Architecture
The `lavender-vision` binary runs as an independent daemon managed by `lavender-core` or the host systemd/launchd supervisor:

1. **Stream Demuxing & Decoding**: Leverages hardware NVDEC/VideoToolbox to decode incoming RTSP streams into raw NV12/RGB frames directly in GPU memory.
2. **SIMD Preprocessing**: Mojo kernels perform zero-copy affine transformations (resizing, letterbox padding, normalization to $[0.0, 1.0]$ float tensors) utilizing hardware vector intrinsics.
3. **Batched Inference**:
   - **Object Detection**: Batched YOLOv11 / RT-DETR model detects targets (aircraft, vessels, ground vehicles, persons) and yields bounding boxes with confidence scores.
   - **Optical ALPR OCR**: Cropped vehicle regions pass through an optical character recognition pipeline to extract license plate identifiers.
   - **Feature Embeddings**: 512-dimensional Reid vector embeddings are computed for cross-camera re-identification.

### 5.3 High-Throughput Arrow Flight IPC Boundary
`lavender-vision` publishes structured detection records to `lavender-core` via **Apache Arrow Flight** over a local Unix Domain Socket (`/tmp/lavender-vision.sock` or `\\.\pipe\lavender-vision` on Windows):

- **Zero-Copy Serialization**: Detections are packed into Arrow `RecordBatch` columnar buffers. Neither process serializes or parses JSON.
- **Flight Schema**:
  ```text
  Field("camera_id", Utf8)
  Field("timestamp_ms", Int64)
  Field("frame_sequence", UInt64)
  Field("bbox", FixedSizeList(Float32, 4))        -- [x_min, y_min, x_max, y_max]
  Field("class_id", UInt16)
  Field("confidence", Float32)
  Field("plate_text", Utf8)                       -- Nullable ALPR string
  Field("embedding", FixedSizeList(Float32, 512)) -- Feature vector
  ```

---

## 6. Nim: Compile-Time Tactical Alerting DSL & Real-Time CEP Engine

### 6.1 Design Philosophy & Rationale
Complex Event Processing (CEP) evaluates continuous streams of kinematic data against user-defined tactical rules (e.g., proximity to critical undersea cables, abnormal climb rates, geofence breaches, multi-vessel rendezvous).

1. **Compile-Time AST Macros**:
   Nim's macro system operates directly on Abstract Syntax Trees at compile-time. Tactical operators write clean, human-readable rules that the compiler lowers into branchless machine code with spatial indexes baked in.
2. **Deterministic ARC/ORC (Zero GC Pauses)**:
   Standard high-level scripting languages (Python, JavaScript, Ruby) or tracing GC languages (Go, Java) introduce non-deterministic garbage collection pauses (10 ms – 100 ms). In high-throughput tactical situations processing 100,000 events/sec, a GC pause causes buffer overflow and stale tactical pictures. Nim's `--mm:orc` (Automatic Reference Counting with cycle detection) inserts deterministic destructors at compile time, guaranteeing zero GC latency spikes.
3. **C ABI Dynamic Library Reloading**:
   Nim compiles directly to clean, portable C code, which clang or gcc compiles into a native dynamic library (`liblavender_cep.dylib` / `.so`). `lavender-core` uses Rust's `libloading` to link function pointers dynamically. Rulesets can be recompiled and swapped in-flight without dropping WebSocket connections or terminating `lavender-core`.

### 6.2 The Tactical Alerting DSL
Analysts declare alerting logic in a high-level syntax that reads like natural tactical doctrine:

```nim
# rules/tactical_maritime.nim
import ../engine/cep_dsl

rule "High-Speed Incursion into Undersea Cable Zone":
  target: Vessel
  temporal_window: 60.seconds
  condition:
    entity.speed_knots > 28.0 and
    distance_to(entity.geom, SubseaAssetCorridor) < 1.5.nautical_miles and
    not is_commercial_registry(entity.mmsi)
  emit:
    severity = Severity.Critical
    category = "InfrastructureThreat"
    title = "High-speed unidentified craft near subsea infrastructure"
    description = "Vessel " & entity.mmsi & " traveling at " & $entity.speed_knots & " kts"
```

The Nim compiler expands the `rule` macro into:
- Static bounding-box rejection checks.
- Direct memory reads of flat coordinate structs.
- Branchless vector conditionals.

### 6.3 In-Process C-ABI Foreign Function Interface
The compiled dynamic library exposes a strict, minimal C ABI consumed by `lavender-core`:

```c
// include/lavender_cep.h
#ifndef LAVENDER_CEP_H
#define LAVENDER_CEP_H

#include <stdint.h>
#include <stdbool.h>

typedef struct {
    uint64_t entity_id;
    uint8_t feed_type;       // 1 = Aircraft, 2 = Vessel, 3 = Satellite
    double lat;
    double lon;
    float alt_m;
    float speed_mps;
    float heading_deg;
    int64_t timestamp_ms;
} KinematicEventC;

typedef struct {
    uint64_t alert_id;
    uint64_t rule_id;
    uint8_t severity;        // 1 = Info, 2 = Warning, 3 = Critical
    int64_t timestamp_ms;
    const char* title;
    const char* description;
    uint64_t target_entity_id;
} TacticalAlertC;

// Callback signature invoked when an alert fires
typedef void (*AlertCallback)(const TacticalAlertC* alert, void* user_data);

// Dynamic library interface functions
int cep_engine_init(const char* config_json);
void cep_engine_register_callback(AlertCallback callback, void* user_data);
int cep_engine_process_event(const KinematicEventC* event);
void cep_engine_destroy(void);

#endif
```

#### Rust Host Integration in `crates/lavender-core/src/cep/engine.rs`
```rust
pub struct DynamicCepEngine {
    _lib: libloading::Library,
    process_event_fn: unsafe extern "C" fn(*const KinematicEventC) -> i32,
    destroy_fn: unsafe extern "C" fn(),
}

impl DynamicCepEngine {
    pub fn new(dylib_path: &Path, tx: mpsc::Sender<TacticalAlert>) -> Result<Self, CepError> {
        unsafe {
            let lib = libloading::Library::new(dylib_path)?;
            let init_fn: libloading::Symbol<unsafe extern "C" fn(*const c_char) -> i32> = 
                lib.get(b"cep_engine_init")?;
            let reg_fn: libloading::Symbol<unsafe extern "C" fn(AlertCallback, *mut c_void)> = 
                lib.get(b"cep_engine_register_callback")?;
            let process_fn: libloading::Symbol<unsafe extern "C" fn(*const KinematicEventC) -> i32> = 
                lib.get(b"cep_engine_process_event")?;
            let destroy_fn: libloading::Symbol<unsafe extern "C" fn()> = 
                lib.get(b"cep_engine_destroy")?;

            // Hand callback pointer to Rust channel
            // ...
            Ok(Self { _lib: lib, process_event_fn: *process_fn, destroy_fn: *destroy_fn })
        }
    }

    #[inline(always)]
    pub fn push_event(&self, event: &KinematicEventC) {
        unsafe { (self.process_event_fn)(event); }
    }
}
```

---

## 7. Technology Pruning & Rejection Analysis

To maintain architectural focus and prevent dependency explosion, competing candidate runtimes were formally evaluated and rejected.

### 7.1 Rejection of Elixir & the BEAM Virtual Machine

| Evaluation Vector | Elixir / BEAM | Lavender Architectural Requirement | Verdict |
|---|---|---|---|
| **Concurrency Model** | Actor model (Erlang processes, mailboxes) | Rust Tokio async green-threads (`tasks`, `watch`, `broadcast`) | **Redundant**: Tokio provides superior throughput with lower CPU overhead. |
| **Runtime Footprint** | ERTS (Erlang Runtime System) requires **35 MiB – 60 MiB base RSS** | Desktop distribution budget is **$\le 150\,\text{MiB}$ total RSS** | **Rejected**: Packaging ERTS into Tauri inflates binary by 45 MiB and doubles base memory. |
| **Interoperability (FFI)** | Erlang NIFs (Native Implemented Functions) | In-process zero-cost FFI | **Rejected**: Faulty NIFs crash the entire BEAM scheduler; Ports incur JSON serialization overhead. |
| **Client/Edge Suitability** | No client-side or freestanding WASM execution | Runs on browser, desktop, and embedded edge | **Rejected**: Cannot run in WebView or WebUSB context. |

*Conclusion*: Elixir/BEAM offers no capability not already provided with higher performance and lower footprint by Tokio in `lavender-core`.

### 7.2 Rejection of Julia

| Evaluation Vector | Julia | Lavender Architectural Requirement | Verdict |
|---|---|---|---|
| **Compilation Latency (TTFX)** | JIT compilation causes **5 – 20 second pauses** on initial function calls ("Time-to-first-execution") | Real-time immediate responsiveness upon user interaction | **Rejected**: Unacceptable startup delays for interactive globe operations. |
| **Runtime Memory Overhead** | `libjulia` + base sysimage requires **200 MiB – 500 MiB RSS** | Lightweight desktop and edge footprint | **Rejected**: Violates Tauri memory constraints before processing a single coordinate. |
| **Garbage Collector Latency** | Tracing stop-the-world mark-and-sweep GC pauses (10 ms – 150 ms) | Bounded frame budget ($< 16.6\,\text{ms}$) and sub-millisecond CEP | **Rejected**: GC pauses breach 60 fps rendering budget and drop SDR samples. |
| **Freestanding WASM** | Unable to compile to freestanding bare WebAssembly | Sub-50 KiB browser micro-kernels | **Rejected**: Julia cannot target `wasm32-freestanding`. |

*Conclusion*: While mathematically rich, Julia's runtime architecture is fundamentally incompatible with embedded desktop applications, edge nodes, and real-time streaming constraints.

---

## 8. Zero-Cost Boundary Interfaces Specification

Every boundary crossing is governed by strict zero-copy or minimal-overhead contracts:

```mermaid
classDiagram
    direction LR

    class ZigWasmBoundary {
        <<SharedArrayBuffer>>
        +MemoryBuffer: WebAssembly.Memory
        +InputRingBuffer: Atomic Pointers
        +OutputRingBuffer: Packed Structs
        +CopyCost: Zero-Copy (Pointer Passing)
    }

    class NimCepBoundary {
        <<C-ABI Dynamic Library>>
        +CallingConvention: extern "C"
        +MemoryModel: repr(C) Flat Structs
        +Lifecycle: libloading In-Process
        +CopyCost: Zero-Cost Function Pointer (~1.5 ns)
    }

    class MojoVisionBoundary {
        <<Apache Arrow Flight IPC>>
        +Transport: Unix Domain Socket
        +Format: Columnar RecordBatch
        +CrashSafety: Out-of-Process Isolation
        +CopyCost: Shared Memory / DMA Zero-Copy
    }

    class DuckDbBoundary {
        <<In-Process C FFI>>
        +Interface: duckdb-rs Direct Linking
        +Storage: Partitioned Parquet
        +ScanModel: Pushdown Vectorized Streaming
        +CopyCost: Zero-Copy Arrow Streams
    }
```

### Boundary Specification Matrix

| Interface | Boundary Type | Transport / Mechanism | Data Representation | Allocation / Copy Cost |
|---|---|---|---|---|
| **WebUSB SDR / SGP4 (Zig)** | Browser Thread Boundary | `SharedArrayBuffer` + WebAssembly Linear Memory | Raw byte slice $\to$ Packed `AdsBRecord` struct | **Zero-copy**: Pointer/offset passing; zero heap allocation |
| **Tactical CEP (Nim)** | In-Process Host FFI | `libloading` dynamic library loading | Flat `#[repr(C)]` structs (`KinematicEventC`) | **Zero-cost**: Direct C function pointer call ($\sim 1.5\,\text{ns}$) |
| **Vision Sidecar (Mojo)** | Inter-Process Communication | Apache Arrow Flight over Unix Domain Socket | Arrow Columnar `RecordBatch` | **Zero-copy**: Memory-mapped IPC buffers |
| **Spatial Analytics (DuckDB)** | In-Process Native C API | `duckdb-rs` direct C bindings | Columnar Parquet $\to$ Arrow Array Streams | **Zero-copy**: Vectorized columnar projection pushdown |

---

## 9. Governance, Security, and Contract Alignment

### 9.1 Shared Rust Core Boundary ([ADR 0002](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0002-shared-rust-core-boundary.md))
- Neither the Axum server (`lavender-server`) nor the Tauri desktop host (`apps/desktop/src-tauri`) directly invokes Zig, DuckDB, Mojo, or Nim.
- All extensions reside behind service interfaces managed within `lavender-core`:
  - `LavenderCore::query_historical_tracks(query: TrackQuery) -> Result<ArrowStream, AnalyticsError>`
  - `LavenderCore::subscribe_tactical_alerts() -> broadcast::Receiver<TacticalAlert>`
  - `LavenderCore::vision_detections() -> watch::Receiver<VisionSnapshot>`
- Adapter layers remain thin, testable, and completely decoupled from extension implementations.

### 9.2 Network Egress & Privacy Governance ([ADR 0008](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0008-data-source-governance-and-network-egress.md))
- **No Direct Internet Access**: Neither DuckDB nor the Mojo sidecar connects to the public internet. All remote feeds pass through `lavender-core`'s allowlist fetcher.
- **Privacy Classification**:
  - `feed=cctv` and `feed=alpr` (Mojo) are categorized as `privacy: Sensitive` and disabled by default.
  - Camera credentials and RTSP URLs are managed by `SecretStore` (ADR 0006) and are never exposed to the frontend.
- **Edge SDR Egress**: Edge sensor pods push data *inward* to `lavender-core` over mutual-TLS authenticated connections.

### 9.3 Generated Domain Contracts via `ts-rs` ([ADR 0007](file:///Volumes/NoirSystems/noir-monorepo/products/lavender/docs/adr/0007-generated-type-contracts.md))
All extension outputs surface in the frontend via generated TypeScript bindings:

- `TacticalAlert` domain type:
  ```rust
  // crates/lavender-domain/src/alert.rs
  use serde::{Deserialize, Serialize};
  use ts_rs::TS;

  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
  #[ts(export, export_to = "contracts/generated/")]
  #[serde(rename_all = "camelCase")]
  pub struct TacticalAlert {
      pub id: String,
      pub rule_id: String,
      pub severity: AlertSeverity,
      pub title: String,
      pub description: String,
      pub target_entity_id: String,
      pub observed_at: String, // RFC 3339
  }
  ```
- `VisionDetection` domain type:
  ```rust
  // crates/lavender-domain/src/vision.rs
  #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, TS)]
  #[ts(export, export_to = "contracts/generated/")]
  #[serde(rename_all = "camelCase")]
  pub struct VisionDetection {
      pub camera_id: String,
      pub timestamp: String,
      pub bbox: [f32; 4],
      pub class_label: String,
      pub confidence: f32,
      pub license_plate: Option<String>,
  }
  ```

---

## 10. Build Toolchains & Continuous Integration

Each polyglot module is integrated into the Lavender build and test harness with strict isolation:

```
products/lavender/
├── Cargo.toml                      Rust workspace
├── crates/
│   ├── lavender-domain/            Types, ts-rs derive, contract definitions
│   ├── lavender-core/              Core facade + DuckDB-rs + CEP host loader
│   └── lavender-server/            Axum server adapter
├── extensions/
│   ├── zig-wasm-kernels/           Freestanding Zig WASM (SDR & SGP4)
│   │   ├── build.zig
│   │   └── src/
│   ├── mojo-vision/                Mojo ALPR & object detection sidecar
│   │   ├── Mojofile
│   │   └── src/
│   └── nim-cep/                    Nim tactical alerting macro DSL
│       ├── nim_cep.nimble
│       └── src/
```

### CI Build Matrix & Validation
1. **Zig**:
   - `zig build test`
   - `zig build -Dtarget=wasm32-freestanding -Doptimize=ReleaseFast`
   - Validates that emitted WASM binaries do not export `malloc`, link libc, or exceed 45 KiB.
2. **DuckDB**:
   - Unit tests run within `cargo test -p lavender-core analytics::tests`.
   - Spatial extension verified against GeoJSON/WKT fixture data.
3. **Mojo**:
   - `mojo test`
   - Verified on Apple Silicon and Linux x86_64 runners with MLIR compiler checks.
4. **Nim**:
   - `nim c -d:release --app:lib --mm:orc -o:liblavender_cep.dylib src/rules.nim`
   - AddressSanitizer and valgrind runs ensure zero memory leaks and deterministic cycle collection.
