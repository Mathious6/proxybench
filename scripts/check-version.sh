#!/bin/sh
set -eu

tag="${GITHUB_REF_NAME:-}"
if [ -z "$tag" ]; then
	echo "GITHUB_REF_NAME is empty."
	exit 1
fi

echo "$tag" | grep -Eq '^v[0-9]+\.[0-9]+\.[0-9]+$' || {
	echo "Tag $tag is not vX.Y.Z."
	exit 1
}

version="${tag#v}"
pkg=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' package.json | head -n 1)
tauri=$(sed -n 's/^  "version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json | head -n 1)
cargo=$(sed -n 's/^version = "\(.*\)"$/\1/p' src-tauri/Cargo.toml | head -n 1)

if [ "$pkg" != "$version" ] || [ "$tauri" != "$version" ] || [ "$cargo" != "$version" ]; then
	echo "Tag $tag does not match package.json ($pkg), tauri.conf.json ($tauri), Cargo.toml ($cargo)."
	exit 1
fi

echo "Tag $tag matches $version."
