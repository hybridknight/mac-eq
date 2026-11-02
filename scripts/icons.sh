#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Keep generated assets checked in so a normal cargo build needs no Swift step.
# Rebuild them whenever the SVG sources or this renderer change.
regenerate=false
for source in assets/AppIcon.svg assets/Artwork.svg assets/MenuBarTemplate.svg scripts/render-icons.swift scripts/icons.sh; do
  if [ ! -f bundle/generated/MacEQ.icns ] || [ "$source" -nt bundle/generated/MacEQ.icns ]; then
    regenerate=true
  fi
done
for generated in AppIcon.png AppIcon256.png Artwork.png MenuBarTemplate.png MenuBarTemplate@2x.png; do
  if [ ! -f "bundle/generated/$generated" ]; then regenerate=true; fi
done
if [ "$regenerate" = true ]; then
  mkdir -p target/swift-cache
  swift -module-cache-path target/swift-cache scripts/render-icons.swift
  iconutil -c icns bundle/generated/MacEQ.iconset -o bundle/generated/MacEQ.icns
fi
