# Homebrew formula for the stetdoc command-line tool. This is a template, not a formula you can install as it stands.
#
# The release job in .github/workflows/release.yml fills the @VERSION@ and @SHA256_*@ placeholders from the release's
# `.sha256` files (see packaging/scripts/fill-formula.mjs) and attaches the result to the draft release as stetdoc.rb.
# To publish it, a maintainer copies that file into a tap repository, as Formula/stetdoc.rb in a repository named
# homebrew-stetdoc. Nothing here pushes to a tap. See https://docs.brew.sh/Taps and https://docs.brew.sh/Formula-Cookbook.
#
# The archives hold a single directory, stetdoc-<triple>/, with the binary and the licence files in it. Homebrew enters
# that directory when it unpacks an archive, so the glob below finds the binary either way.
class Stetdoc < Formula
  desc "Turn a .docx into an agent-editable working copy and back, with real Word tracked changes"
  homepage "https://github.com/clauseo-ai/stetdoc"
  version "@VERSION@"
  license any_of: ["MIT", "Apache-2.0"]

  on_macos do
    on_arm do
      url "https://github.com/clauseo-ai/stetdoc/releases/download/v@VERSION@/stetdoc-aarch64-apple-darwin.tar.gz"
      sha256 "@SHA256_AARCH64_APPLE_DARWIN@"
    end
    on_intel do
      url "https://github.com/clauseo-ai/stetdoc/releases/download/v@VERSION@/stetdoc-x86_64-apple-darwin.tar.gz"
      sha256 "@SHA256_X86_64_APPLE_DARWIN@"
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/clauseo-ai/stetdoc/releases/download/v@VERSION@/stetdoc-aarch64-unknown-linux-musl.tar.gz"
      sha256 "@SHA256_AARCH64_UNKNOWN_LINUX_MUSL@"
    end
    on_intel do
      url "https://github.com/clauseo-ai/stetdoc/releases/download/v@VERSION@/stetdoc-x86_64-unknown-linux-musl.tar.gz"
      sha256 "@SHA256_X86_64_UNKNOWN_LINUX_MUSL@"
    end
  end

  def install
    bin.install Dir["**/stetdoc"].first => "stetdoc"
  end

  test do
    assert_match version.to_s, shell_output("#{bin}/stetdoc --version")
  end
end
