"""
Apache Arrow Flight Schema for Lavender Vision Perception Sidecar (Mojo / Python).

Defines the zero-copy binary IPC format connecting `lavender-vision` to `lavender-core`.
Matches docs/architecture/polyglot-extensions.md §5.3.
"""

import pyarrow as pa

# Canonical Arrow Schema for vision detections
VISION_DETECTION_SCHEMA = pa.schema([
    pa.field("camera_id", pa.utf8(), nullable=False),
    pa.field("timestamp_ms", pa.int64(), nullable=False),
    pa.field("frame_sequence", pa.uint64(), nullable=False),
    pa.field("bbox", pa.list_(pa.float32(), 4), nullable=False), # [xmin, ymin, xmax, ymax] normalized [0.0, 1.0]
    pa.field("class_id", pa.uint16(), nullable=False),           # 0: person, 1: car, 2: truck, 3: vessel, 4: aircraft
    pa.field("confidence", pa.float32(), nullable=False),
    pa.field("plate_text", pa.utf8(), nullable=True),            # ALPR license plate if detected
])

def create_detection_batch(camera_id: str, timestamp_ms: int, frame_sequence: int, detections: list[dict]) -> pa.RecordBatch:
    """
    Packs a list of detections into a zero-copy Arrow RecordBatch.
    """
    n = len(detections)
    camera_ids = [camera_id] * n
    timestamps = [timestamp_ms] * n
    frame_sequences = [frame_sequence] * n
    bboxes = [d["bbox"] for d in detections]
    class_ids = [d["class_id"] for d in detections]
    confidences = [d["confidence"] for d in detections]
    plates = [d.get("plate_text") for d in detections]

    return pa.RecordBatch.from_arrays(
        [
            pa.array(camera_ids, type=pa.utf8()),
            pa.array(timestamps, type=pa.int64()),
            pa.array(frame_sequences, type=pa.uint64()),
            pa.array(bboxes, type=pa.list_(pa.float32(), 4)),
            pa.array(class_ids, type=pa.uint16()),
            pa.array(confidences, type=pa.float32()),
            pa.array(plates, type=pa.utf8()),
        ],
        schema=VISION_DETECTION_SCHEMA,
    )
