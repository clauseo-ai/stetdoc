# Security policy

## Supported versions

stetdoc is under development toward 1.0.0, and nothing has been released. Until the first
release, fixes go to the main branch only. After 1.0.0, this section will list the
supported release lines.

## Reporting a vulnerability

Please report vulnerabilities privately, through GitHub's private vulnerability reporting
for this repository. Open the repository's **Security** tab and choose **Report a
vulnerability**. If you cannot use GitHub's reporting, email rohan@clauseo.chat instead. Do
not open a public issue or pull request for a security problem.

Include:

- what you found and why you think it is a vulnerability
- the smallest input that shows it (a `.docx`, a working copy, or a batch), or steps to
  reproduce
- the version or commit you tested, and your platform

We will acknowledge your report, keep you informed while we investigate, and credit you in
the advisory unless you prefer otherwise.

## What counts

stetdoc reads untrusted `.docx` files, so the following are in scope:

- a panic, hang, or unbounded memory or time use on any input, including ZIP bombs,
  entity declarations, path traversal and truncated archives
- reading or writing outside the intended files, or any network access
- an unedited round trip that changes bytes, or an edit that writes a change that was not
  requested or not tracked
- a working copy whose complement can make `resume` trust data it should have verified

Bad input is meant to fail cleanly with a stable [error code](docs/reference/errors.md).
A failure that does not is a bug, and a security bug when it can be triggered by a
hostile file.

## How hostile input is tested

- The hostile tier of the corpus (ZIP bombs, path traversal, entity declarations, truncated and
  malformed archives and XML) runs in `cargo xtask check`: each document must be refused cleanly,
  as the corpus manifest expects, and no document outside that tier may be refused.
- Four fuzz targets cover the entry points: `package` (the ZIP container), `xml` (the parser and
  editor), `batch` (operation batches applied to a document) and `files` (edits of the readable
  files). Each runs for 60 seconds in CI on every pull request and for at least 24 hours before a
  release; a crash, a hang over 30 seconds or a broken law fails the run. See
  [`fuzz/`](fuzz/) and `cargo xtask fuzz`.
