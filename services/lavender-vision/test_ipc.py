"""
Unit tests for Lavender Vision Arrow IPC serialization.
"""

import io
import pyarrow as pa
from schema import VISION_DETECTION_SCHEMA, create_detection_batch

def test_arrow_batch_serialization():
    detections = [
        {
            "bbox": [0.1, 0.2, 0.4, 0.5],
            "class_id": 1, # car
            "confidence": 0.94,
            "plate_text": "AB123CD",
        },
        {
            "bbox": [0.6, 0.7, 0.9, 0.85],
            "class_id": 3, # vessel
            "confidence": 0.88,
            "plate_text": None,
        }
    ]

    batch = create_detection_batch("CCTV-CAM-01", 1700000000000, 1042, detections)
    assert batch.num_rows == 2
    assert batch.schema.equals(VISION_DETECTION_SCHEMA)

    # Serialize to Arrow IPC Stream
    sink = io.BytesIO()
    with pa.ipc.new_stream(sink, VISION_DETECTION_SCHEMA) as writer:
        writer.write_batch(batch)

    # Deserialize back from stream bytes
    sink.seek(0)
    with pa.ipc.open_stream(sink) as reader:
        read_batch = reader.read_next_batch()
        assert read_batch.num_rows == 2
        assert read_batch.column("camera_id")[0].as_py() == "CCTV-CAM-01"
        assert read_batch.column("plate_text")[0].as_py() == "AB123CD"
        assert read_batch.column("plate_text")[1].as_py() is None

    print("All Arrow IPC serialization tests passed successfully!")

if __name__ == "__main__":
    test_arrow_batch_serialization()
