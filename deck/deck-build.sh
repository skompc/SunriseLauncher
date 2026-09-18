#!/bin/bash

set -euo pipefail


PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

BUILD_DIR="$PROJECT_ROOT/src-tauri/target/release"

WORK_DIR="$BUILD_DIR/steamdeck-package"

STAGE_DIR="$WORK_DIR/stage"

LAUNCHER="$WORK_DIR/sunrise-launcher"

PAYLOAD="$WORK_DIR/payload.tar.gz"

OUTPUT="$PROJECT_ROOT/Project Sunrise Launcher"


echo
echo "========================================"
echo " Project Sunrise Launcher"
echo " Steam Deck Package"
echo "========================================"
echo


rm -rf "$WORK_DIR"

mkdir -p "$STAGE_DIR/app"


echo "[1/5] Compiling native launcher..."

cc \
    -O2 \
    -Wall \
    -Wextra \
    -o "$LAUNCHER" \
    "$PROJECT_ROOT/deck/sunrise-launcher.c"


chmod +x "$LAUNCHER"


echo "[2/5] Preparing application..."

cp \
    "$BUILD_DIR/project-sunrise-launcher" \
    "$STAGE_DIR/app/project-sunrise-launcher"


chmod +x \
    "$STAGE_DIR/app/project-sunrise-launcher"


echo "[3/5] Adding application icon..."

mkdir -p "$STAGE_DIR/icons"

cp \
    "$PROJECT_ROOT/src-tauri/icons/128x128.png" \
    "$STAGE_DIR/icons/128x128.png"


echo "[4/5] Creating compressed payload..."

tar \
    -C "$STAGE_DIR" \
    -czf "$PAYLOAD" \
    .


LAUNCHER_SIZE="$(stat -c '%s' "$LAUNCHER")"

PAYLOAD_SIZE="$(stat -c '%s' "$PAYLOAD")"


echo "[5/5] Building single-file launcher..."

cp \
    "$LAUNCHER" \
    "$OUTPUT"


cat \
    "$PAYLOAD" \
    >> "$OUTPUT"


python3 - "$OUTPUT" "$LAUNCHER_SIZE" "$PAYLOAD_SIZE" <<'PY'
import struct
import sys

output = sys.argv[1]
payload_offset = int(sys.argv[2])
payload_size = int(sys.argv[3])

magic = b"SUNRISE_PAYLOAD1"

with open(output, "ab") as file:
    file.write(magic)
    file.write(struct.pack("<Q", payload_offset))
    file.write(struct.pack("<Q", payload_size))
PY


chmod +x "$OUTPUT"


rm -rf "$WORK_DIR"


echo
echo "========================================"
echo " Build complete"
echo "========================================"
echo
echo "Output:"
echo
echo "  $OUTPUT"
echo
echo "File information:"
file "$OUTPUT"
echo
echo "Size:"
du -h "$OUTPUT"
echo