#!/bin/sh
# Installs the stetdoc command-line tool from a GitHub release.
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/clauseo-ai/stetdoc/main/packaging/install.sh | sh
#   STETDOC_VERSION=v1.0.0 STETDOC_INSTALL_DIR=/usr/local/bin sh install.sh
#
# It detects the OS and the CPU, downloads the matching archive from the release, checks its SHA-256 against the
# release's `.sha256` file (written by .github/workflows/release.yml), and installs the `stetdoc` binary. Linux gets
# the static musl build, which runs on glibc and musl alike. Windows is not covered: use the zip from the release
# page, or `npm install -g stetdoc`.
#
# Environment:
#   STETDOC_VERSION      a release tag such as v1.0.0, or `latest` (the default)
#   STETDOC_INSTALL_DIR  where to put the binary (default: $HOME/.local/bin)
#   STETDOC_REPO         the GitHub owner/repo to download from (default: the REPO line below)
#   STETDOC_BASE_URL     a directory holding the archives and their .sha256 files, for a mirror or a test; overrides the above
#
# POSIX sh only. It needs curl or wget, tar, and sha256sum or shasum.
set -eu

# The repository the release archives come from.
REPO="${STETDOC_REPO:-clauseo-ai/stetdoc}"

VERSION="${STETDOC_VERSION:-latest}"
INSTALL_DIR="${STETDOC_INSTALL_DIR:-$HOME/.local/bin}"

die() {
  printf 'install.sh: %s\n' "$1" >&2
  exit 1
}

say() {
  printf 'install.sh: %s\n' "$1"
}

# The Rust target triple of the archive for this machine. Linux always gets the musl build, which is static.
detect_triple() {
  os="$(uname -s)"
  machine="$(uname -m)"
  case "$machine" in
    x86_64 | amd64) cpu=x86_64 ;;
    arm64 | aarch64) cpu=aarch64 ;;
    *) die "unsupported CPU: $machine. Build from source with cargo install --path crates/stetdoc-cli" ;;
  esac
  case "$os" in
    Darwin)
      # A shell under Rosetta reports x86_64 on an Apple silicon Mac. Prefer the native build there.
      if [ "$(sysctl -n hw.optional.arm64 2> /dev/null || echo 0)" = "1" ]; then cpu=aarch64; fi
      printf '%s-apple-darwin\n' "$cpu"
      ;;
    Linux) printf '%s-unknown-linux-musl\n' "$cpu" ;;
    *) die "unsupported OS: $os. On Windows, download the zip from the release page or run npm install -g stetdoc" ;;
  esac
}

# Downloads a URL to a file: curl if present, else wget.
fetch() {
  if command -v curl > /dev/null 2>&1; then
    curl -fsSL --retry 3 -o "$2" "$1"
  elif command -v wget > /dev/null 2>&1; then
    wget -q -O "$2" "$1"
  else
    die "needs curl or wget to download the release"
  fi
}

# The SHA-256 of a file, as hex, with sha256sum (Linux) or shasum (macOS).
sha256_of() {
  if command -v sha256sum > /dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

if ! command -v sha256sum > /dev/null 2>&1 && ! command -v shasum > /dev/null 2>&1; then
  die "needs sha256sum or shasum to check the download"
fi
command -v tar > /dev/null 2>&1 || die "needs tar to unpack the download"

triple="$(detect_triple)"
asset="stetdoc-$triple.tar.gz"
if [ -n "${STETDOC_BASE_URL:-}" ]; then
  base="$STETDOC_BASE_URL"
elif [ "$VERSION" = "latest" ]; then
  base="https://github.com/$REPO/releases/latest/download"
else
  base="https://github.com/$REPO/releases/download/$VERSION"
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/stetdoc-install.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT INT TERM

say "downloading $asset from $REPO ($VERSION)"
fetch "$base/$asset" "$tmp/$asset" || die "could not download $base/$asset. Check STETDOC_VERSION and that the release is published"
fetch "$base/$asset.sha256" "$tmp/$asset.sha256" || die "could not download $base/$asset.sha256"

# The checksum file is `<hash>  <name>`. Compare the hash alone, so a file saved under another name still checks.
expected="$(awk '{print $1}' "$tmp/$asset.sha256")"
case "$expected" in
  [0-9a-f]*) ;;
  *) die "the checksum file for $asset is malformed" ;;
esac
if [ "${#expected}" -ne 64 ]; then die "the checksum file for $asset is malformed"; fi
actual="$(sha256_of "$tmp/$asset")"
if [ "$actual" != "$expected" ]; then
  die "SHA-256 mismatch for $asset: expected $expected, got $actual. Nothing was installed"
fi
say "SHA-256 verified"

tar -xzf "$tmp/$asset" -C "$tmp"
binary="$tmp/stetdoc-$triple/stetdoc"
[ -f "$binary" ] || die "the archive does not hold stetdoc-$triple/stetdoc"

mkdir -p "$INSTALL_DIR"
install -m 0755 "$binary" "$INSTALL_DIR/stetdoc"
say "installed $INSTALL_DIR/stetdoc"
"$INSTALL_DIR/stetdoc" --version || true

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) say "$INSTALL_DIR is not on your PATH. Add it to your shell profile to run stetdoc by name." ;;
esac
