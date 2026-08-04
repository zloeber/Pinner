#!/usr/bin/env zsh
# Install pinner from GitHub Releases into ~/.local/bin (or PINNER_INSTALL_DIR).
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/zloeber/Pinner/main/scripts/install.sh | zsh
#   PINNER_VERSION=0.2.0 zsh scripts/install.sh
#   PINNER_INSTALL_DRY_RUN=1 zsh scripts/install.sh   # print URL + path only
set -euo pipefail

REPO="${PINNER_REPO:-zloeber/Pinner}"
INSTALL_DIR="${PINNER_INSTALL_DIR:-${HOME}/.local/bin}"
DRY_RUN="${PINNER_INSTALL_DRY_RUN:-0}"

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"

  case "${os}:${arch}" in
    Darwin:x86_64) print -r -- x86_64-apple-darwin ;;
    Darwin:arm64) print -r -- aarch64-apple-darwin ;;
    Linux:x86_64) print -r -- x86_64-unknown-linux-gnu ;;
    Linux:aarch64 | Linux:arm64) print -r -- aarch64-unknown-linux-gnu ;;
    *)
      print -r -- "install.sh: unsupported platform ${os}/${arch}" >&2
      print -r -- "install.sh: supported: linux/darwin on x86_64 or arm64" >&2
      print -r -- "install.sh: Windows users: download .zip from GitHub Releases" >&2
      return 1
      ;;
  esac
}

resolve_version() {
  if [[ -n "${PINNER_VERSION:-}" ]]; then
    print -r -- "${PINNER_VERSION}"
    return 0
  fi

  local api_url="https://api.github.com/repos/${REPO}/releases/latest"
  local tag
  tag="$(curl -fsSL "$api_url" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\([^"]*\)".*/\1/p' | head -1)"
  if [[ -z "$tag" ]]; then
    print -r -- "install.sh: failed to resolve latest release from ${api_url}" >&2
    print -r -- "install.sh: set PINNER_VERSION to pin a version" >&2
    return 1
  fi
  print -r -- "$tag"
}

path_hint() {
  local dir="$1"
  case ":${PATH}:" in
    *":${dir}:"*) return 0 ;;
  esac
  print -r -- ""
  print -r -- "Add ${dir} to PATH, for example:"
  print -r -- "  export PATH=\"${dir}:\$PATH\""
}

main() {
  local target version asset url stage dest tmpdir

  target="$(detect_target)"
  version="$(resolve_version)"
  asset="pinner-${version}-${target}.tar.gz"
  url="https://github.com/${REPO}/releases/download/v${version}/${asset}"
  dest="${INSTALL_DIR}/pinner"

  if [[ "$DRY_RUN" == "1" ]]; then
    print -r -- "URL: ${url}"
    print -r -- "INSTALL_DIR: ${INSTALL_DIR}"
    exit 0
  fi

  tmpdir="$(mktemp -d -t pinner-install.XXXXXX)"
  trap 'rm -rf "$tmpdir"' EXIT INT TERM

  print -r -- "Downloading pinner ${version} (${target})..."
  curl -fsSL "$url" -o "${tmpdir}/${asset}"

  stage="pinner-${version}-${target}"
  tar -xzf "${tmpdir}/${asset}" -C "$tmpdir"

  if [[ ! -f "${tmpdir}/${stage}/pinner" ]]; then
    print -r -- "install.sh: expected binary at ${stage}/pinner inside archive" >&2
    return 1
  fi

  mkdir -p "$INSTALL_DIR"
  install -m 755 "${tmpdir}/${stage}/pinner" "$dest"

  print -r -- "Installed pinner ${version} -> ${dest}"
  path_hint "$INSTALL_DIR"
}

main "$@"
