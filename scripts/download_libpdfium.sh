#!/bin/bash

# Detect OS and Architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

# Default Tag
TAG="chromium/7543"
BASE_URL="https://github.com/bblanchon/pdfium-binaries/releases/download/$TAG"

FILE=""
EXTRACT_FILE=""
DEST_NAME=""

case "$OS" in
    Darwin)
        EXTRACT_FILE="lib/libpdfium.dylib"
        DEST_NAME="libpdfium.dylib"
        if [ "$ARCH" == "arm64" ]; then
            FILE="pdfium-mac-arm64.tgz"
        else
            FILE="pdfium-mac-x64.tgz"
        fi
        ;;
    Linux)
        EXTRACT_FILE="lib/libpdfium.so"
        DEST_NAME="libpdfium.so"
         if [ "$ARCH" == "aarch64" ]; then
            FILE="pdfium-linux-arm64.tgz"
        else
            FILE="pdfium-linux-x64.tgz"
        fi
        ;;
    MINGW*|CYGWIN*|MSYS*)
        OS="Windows"
        FILE="pdfium-win-x64.zip" 
        EXTRACT_FILE="bin/pdfium.dll"
        DEST_NAME="pdfium.dll"
        ;;
    *)
        echo "Unsupported OS: $OS"
        exit 1
        ;;
esac

echo "Detected Platform: $OS ($ARCH)"
echo "Downloading $FILE from $BASE_URL/$FILE..."
curl -L -o pdfium_archive "$BASE_URL/$FILE"

echo "Extracting $EXTRACT_FILE..."
if [ "$OS" == "Windows" ]; then
    unzip -p pdfium_archive "$EXTRACT_FILE" > "$DEST_NAME"
else
    tar -xzf pdfium_archive "$EXTRACT_FILE"
    # Move from lib/ to current dir to handle extraction structure
    if [ -f "$EXTRACT_FILE" ]; then
        mv "$EXTRACT_FILE" "$DEST_NAME"
    fi
fi

# Move to target directories
TARGET_DIR="knot-app/bin"
ROOT_DIR="knot-app"

mkdir -p "$TARGET_DIR"
cp "$DEST_NAME" "$TARGET_DIR/"
cp "$DEST_NAME" "$ROOT_DIR/"

# Clean up
if [ "$OS" != "Windows" ]; then
    rm -rf lib
fi
rm pdfium_archive
rm "$DEST_NAME"

echo "Done! $DEST_NAME placed in $TARGET_DIR and $ROOT_DIR"
