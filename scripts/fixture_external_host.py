#!/usr/bin/env python3
"""Small deterministic RawWeave external-host fixture for integration tests."""
from __future__ import annotations

import hashlib
import json
import os
import struct
import sys

ROOT = os.environ.get("RAWWEAVE_EXTERNAL_DATA_ROOT", ".")
NEGOTIATED = False


def protocol():
    return {
        "major": int(os.environ.get("RAWWEAVE_FIXTURE_PROTOCOL_MAJOR", "1")),
        "minor": 0,
    }


def capabilities():
    return {
        "pixel_formats": ["Rgba32Float", "Mask32Float"],
        "roi": False,
        "full_frame": True,
        "multi_input": False,
        "multi_output": False,
        "thread_safety": "SingleThreaded",
        "gpu": False,
        "custom_ui": False,
        "deterministic": True,
        "data_plane": os.environ.get("RAWWEAVE_FIXTURE_DATA_PLANE", "1") == "1",
    }


def descriptors():
    common = {
        "version": 1,
        "inputs": [],
        "outputs": [],
        "parameters": [],
        "capabilities": capabilities(),
    }
    image = dict(common)
    image.update(
        {
            "type_id": "fixture.image-pass",
            "name": "Fixture Image Pass",
            "inputs": [{"id": "image", "name": "Image", "data_type": "core.Image", "required": True}],
            "outputs": [{"id": "image", "name": "Image", "data_type": "core.Image", "required": False}],
            "parameters": [
                {"id": "strength", "name": "Strength", "data_type": "float", "default": {"Float": 1.0}}
            ],
        }
    )
    mask = dict(common)
    mask.update(
        {
            "type_id": "fixture.mask-pass",
            "name": "Fixture Mask Pass",
            "inputs": [{"id": "mask", "name": "Mask", "data_type": "core.Mask", "required": True}],
            "outputs": [{"id": "mask", "name": "Mask", "data_type": "core.Mask", "required": False}],
        }
    )
    return [image, mask]


def response(request_id, payload):
    return {"Response": {"id": request_id, "protocol": protocol(), "payload": payload}}


def error(request_id, code, message):
    return response(
        request_id,
        {"result": "Error", "data": {"code": code, "message": message, "retryable": False, "details": {}}},
    )


def buffer_value(value):
    if not isinstance(value, dict) or "Buffer" not in value:
        return None
    descriptor = dict(value["Buffer"])
    source = os.path.join(ROOT, descriptor["relative_path"])
    with open(source, "rb") as handle:
        data = handle.read()
    descriptor["sha256"] = hashlib.sha256(data).hexdigest()
    descriptor["byte_len"] = len(data)
    descriptor["ownership"] = "Shared"
    return {"Buffer": descriptor}


def handle(request):
    global NEGOTIATED
    request_id = request["id"]
    payload = request["payload"]
    operation = payload["operation"]
    data = payload.get("data") or {}
    log_path = os.environ.get("RAWWEAVE_FIXTURE_OPERATION_LOG")
    if log_path:
        with open(log_path, "a", encoding="utf-8") as log:
            log.write(f"{operation}\n")
    if operation == "CapabilityQuery":
        NEGOTIATED = True
        return response(request_id, {"result": "Capabilities", "data": {"capabilities": capabilities()}})
    if not NEGOTIATED:
        return error(request_id, "negotiation_required", "capability query required")
    if operation == "Discover":
        return response(
            request_id,
            {"result": "Discovered", "data": {"descriptors": descriptors(), "capabilities": capabilities()}},
        )
    if operation == "Describe":
        found = next((item for item in descriptors() if item["type_id"] == data["type_id"]), None)
        return response(request_id, {"result": "Described", "data": {"descriptor": found}}) if found else error(request_id, "missing_node", "node not found")
    if operation == "Instantiate":
        return response(request_id, {"result": "Instantiated", "data": {"instance_id": data["instance_id"]}})
    if operation == "SetParameters":
        return response(request_id, {"result": "Acknowledged"})
    if operation == "Evaluate":
        inputs = data.get("inputs", {})
        output = next((buffer_value(value) for value in inputs.values() if buffer_value(value)), None)
        if output is None:
            return error(request_id, "missing_input", "fixture requires a typed buffer")
        return response(request_id, {"result": "Evaluated", "data": {"outputs": {"image": output, "mask": output}}})

    if operation in ("Status", "Result", "Cancel", "SerializeState", "Destroy"):
        return response(request_id, {"result": "Acknowledged"})
    return error(request_id, "unsupported", operation)


def main():
    while True:
        header = sys.stdin.buffer.read(4)
        if not header:
            return 0
        if len(header) != 4:
            return 2
        length = struct.unpack(">I", header)[0]
        body = sys.stdin.buffer.read(length)
        if len(body) != length:
            return 2
        message = json.loads(body)
        request = message.get("Request")
        if request is None:
            return 2
        encoded = json.dumps(handle(request), separators=(",", ":")).encode()
        sys.stdout.buffer.write(struct.pack(">I", len(encoded)) + encoded)
        sys.stdout.buffer.flush()


if __name__ == "__main__":
    raise SystemExit(main())
