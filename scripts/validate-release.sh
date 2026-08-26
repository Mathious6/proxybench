#!/bin/sh
set -eu

repo="${REPO:?REPO is required}"
tag="${TAG:?TAG is required}"
release_id="${RELEASE_ID:?RELEASE_ID is required}"
release="$(gh api "repos/$repo/releases/$release_id")"

test "$(printf '%s' "$release" | jq -r .draft)" = true
test "$(printf '%s' "$release" | jq -r .tag_name)" = "$tag"
assets="$(printf '%s' "$release" | jq -r '.assets[].name')"
test "$(printf '%s\n' "$assets" | grep -c '\.dmg$')" -eq 2
test "$(printf '%s\n' "$assets" | grep -c '\.app\.tar\.gz$')" -eq 2
test "$(printf '%s\n' "$assets" | grep -c '\.app\.tar\.gz\.sig$')" -eq 2
test "$(printf '%s\n' "$assets" | grep -c -- '-setup\.exe$')" -eq 1
test "$(printf '%s\n' "$assets" | grep -c -- '-setup\.exe\.sig$')" -eq 1
test "$(printf '%s\n' "$assets" | grep -c '^latest\.json$')" -eq 1

latest_url="$(printf '%s' "$release" | jq -r '.assets[] | select(.name == "latest.json") | .url')"
gh api -H 'Accept: application/octet-stream' "$latest_url" > latest.json
test "$(jq -r .version latest.json)" = "${tag#v}"

for platform in darwin-aarch64 darwin-x86_64 windows-x86_64; do
  url="$(jq -er --arg platform "$platform" '.platforms[$platform].url' latest.json)"
  signature="$(jq -er --arg platform "$platform" '.platforms[$platform].signature' latest.json)"
  test -n "$signature"
  asset="$(printf '%s' "$release" | jq -cer --arg url "$url" '.assets[] | select(.url == $url or .browser_download_url == $url)')"
  name="$(printf '%s' "$asset" | jq -r .name)"
  case "$platform:$name" in
    darwin-aarch64:*_aarch64.app.tar.gz) ;;
    darwin-x86_64:*_x64.app.tar.gz) ;;
    windows-x86_64:*_x64-setup.exe) ;;
    *) exit 1 ;;
  esac
  asset_url="$(printf '%s' "$asset" | jq -r .url)"
  gh api -H 'Accept: application/octet-stream' "$asset_url" > "$name"
  printf '%s' "$signature" | base64 --decode > "$name.sig"
  public_key="$(jq -r '.plugins.updater.pubkey' src-tauri/tauri.conf.json)"
  printf '%s' "$public_key" | base64 --decode > updater.pub
  minisign -Vm "$name" -p updater.pub -x "$name.sig"
done
