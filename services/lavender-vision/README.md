# Lavender Vision Perception Sidecar (`lavender-vision`)

Companion computer vision daemon for Lavender GEOINT/OSINT platform.

## Architecture

Matches **ADR 0009** and `docs/architecture/polyglot-extensions.md` §5.

- **Engine**: Mojo (MLIR) / Python high-throughput vision pipeline.
- **Hardware Acceleration**: Auto-vectorized tensor inference targeting Apple Silicon Neural Engine, NVIDIA Tensor Cores, and Intel AVX-512 with zero GIL bottleneck.
- **IPC Protocol**: Apache Arrow Flight / Unix Domain Socket stream (`/tmp/lavender-vision.sock`).
- **Isolation**: Video decoding (NVDEC/VideoToolbox) and model failures remain isolated from `lavender-core`.

## Tasks Covered
- **CCTV Maritime & Aviation Detection**: Bounding boxes, object classes, and kinematic tracks from public traffic and harbor webcams (`ENG-964`).
- **ALPR Optical Recognition**: Automated license plate character extraction from traffic camera feeds.

## Integration Boundary
- `lavender-core` connects as an IPC client to `/tmp/lavender-vision.sock`.
- Detections are dispatched to `FeedHub` as `FeedSnapshot` updates with `privacy: Sensitive` governance (ADR 0008).
