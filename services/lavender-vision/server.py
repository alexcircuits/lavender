"""
Lavender Vision Perception Sidecar Server (Mojo / Python).

Runs as an isolated companion daemon listening on Unix Domain Socket (`/tmp/lavender-vision.sock`).
Pushes structured camera detections, ALPR reads, and vessel/aircraft bounding boxes
to `lavender-core` with zero GIL contention and crash isolation.
"""

import json
import os
import socket
import sys
import time

SOCKET_PATH = "/tmp/lavender-vision.sock"

def create_detection_packet(camera_id: str, frame_seq: int, detections: list) -> dict:
    return {
        "camera_id": camera_id,
        "timestamp_ms": int(time.time() * 1000),
        "frame_sequence": frame_seq,
        "detections": detections,
    }

def serve():
    if os.path.exists(SOCKET_PATH):
        os.remove(SOCKET_PATH)

    server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
    server.bind(SOCKET_PATH)
    server.listen(5)
    print(f"Lavender Vision Perception Sidecar listening on {SOCKET_PATH}")

    try:
        while True:
            conn, _ = server.accept()
            print("lavender-core connected to vision IPC socket.")
            try:
                # Simulated telemetry loop
                seq = 1
                while True:
                    packet = create_detection_packet(
                        camera_id="CCTV-HARBOR-01",
                        frame_seq=seq,
                        detections=[
                            {
                                "bbox": [0.12, 0.34, 0.56, 0.78],
                                "class_id": 3, # vessel
                                "confidence": 0.92,
                                "plate_text": None,
                            }
                        ]
                    )
                    payload = (json.dumps(packet) + "\n").encode("utf-8")
                    conn.sendall(payload)
                    seq += 1
                    time.sleep(1.0)
            except (BrokenPipeError, ConnectionResetError):
                print("lavender-core disconnected.")
            finally:
                conn.close()
    finally:
        server.close()
        if os.path.exists(SOCKET_PATH):
            os.remove(SOCKET_PATH)

if __name__ == "__main__":
    serve()
