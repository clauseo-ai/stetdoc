# Self-hosting the MCP server

stetdoc ships no hosted service. `stetdoc mcp` is a local server that speaks MCP on stdin and stdout. A cloud app such
as Claude Cowork, ChatGPT on the web or grok.com needs a remote server it can reach over HTTPS instead. This page shows
how to run one yourself: `stetdoc mcp --root DIR` behind a bridge from stdio to streamable HTTP, with authentication and
TLS in front of it.

```
cloud app ──HTTPS──▶ reverse proxy (TLS, auth) ──HTTP──▶ bridge ──stdio──▶ stetdoc mcp --root /data/<user>
```

## The pieces

**The server.** `stetdoc mcp --root DIR` confines every path argument of every tool call to `DIR`, and resolves relative
paths against it. Its tools are the commands: `unpack`, `pack`, `view`, `inspect`, `edit`, `replace`, `comment`,
`compare`, `check`, `lookup` and `skills_get`. Each returns the text the command prints, and the JSON of `--json` in
`structuredContent`. A relative path that is not there is looked for in the folders of the documents the session has
named, and stays confined to `DIR`.

**The bridge.** [supergateway](https://github.com/supercorp-ai/supergateway) runs a stdio server and serves it over
streamable HTTP. It is maintained, and its 4.1.0 release was the current one on 2026-10-11:

```
npx -y supergateway@4.1.0 \
  --stdio "stetdoc mcp --root /data/alice" \
  --outputTransport streamableHttp --stateful \
  --host 127.0.0.1 --port 8000
```

The endpoint is `http://127.0.0.1:8000/mcp`. Bind the bridge to `127.0.0.1` (or a private network) so that only the
proxy can reach it. Do not rely on the bridge for authentication: the README on its main branch documents an `--apiKey`
flag, but the 4.1.0 release does not have it and accepts every request. [mcp-proxy](https://github.com/sparfenyuk/mcp-proxy)
is an alternative in Python that also serves stdio over streamable HTTP.

**The proxy.** A reverse proxy in front terminates TLS and checks credentials before anything reaches the bridge. With
[Caddy](https://caddyserver.com/docs/caddyfile), which gets a certificate for the domain by itself:

```
mcp.example.com {
	@authorized header Authorization "Bearer {$STETDOC_MCP_TOKEN}"
	handle @authorized {
		reverse_proxy 127.0.0.1:8000
	}
	respond 401
}
```

Start it with `STETDOC_MCP_TOKEN` set to a long random secret, and give the app the URL `https://mcp.example.com/mcp`
and the bearer token. A static token suits one user and a client that can send a header. Some apps add custom connectors
only with OAuth. For those, put an OAuth-capable proxy in front instead, and check the app's connector docs for what it
needs.

## One root per user

Run one bridge and one `stetdoc mcp` per user or tenant, each with its own root, such as `/data/alice` and `/data/bob`,
each behind its own credentials. The root is the only thing that separates one user's documents from another's, so
never share a server between users. Route each user's token to their own bridge in the proxy.

## Getting files in and out

stetdoc reads and writes files in the root, and has no upload or download of its own. The flow is:

1. Your system puts the `.docx` into the user's root: an upload endpoint, a synced bucket, or `scp`.
2. The agent calls the tools with paths relative to the root: `unpack contract.docx`, then `view`, `edit` and `check`,
   then `pack`, which writes `contract.edited.docx` beside it.
3. Your system hands the output back to the user: a download link, or a sync back to their storage.

Clean the root up on the schedule your users expect. Working copies (`*.stet/`) hold the full document.

## The author

Every revision needs an author, and on a hosted server it must be the user's name, not the server's. Give it per call or
per server, never through a config file:

- per call: the agent, or the app in front of it, passes `author` in each `edit`, `pack` or `compare` call;
- per server: start each user's server with `stetdoc mcp --root /data/alice --author "Alice Example"`, or with
  `STETDOC_AUTHOR` in that server's environment.

A call's `author` comes first, then `--author`, then `STETDOC_AUTHOR`. Do not run `stetdoc init` on the server: the name
it saves in `stetdoc/config.toml` is the last fallback, and it would sign every user's revisions with one name. With no
author at all, a call that writes revisions fails and says so; stetdoc never takes a name from git or the system.

The date defaults to the server's local clock, so set `TZ` to the user's time zone, or have
the agent pass `date`.

## Security

What `--root` does:

- A relative path is joined to the root, and an absolute path is taken as it is. The path is normalized, and refused
  when a `..` takes it out of the root.
- The deepest existing part of the path is resolved, following every symbolic link, and refused when it lands outside
  the root. So a link inside the root that points out is refused.
- Paths the server computes itself, such as `pack`'s default output, are confined the same way.

What it does not do:

- It is not a sandbox. The process can read and write anything its user can. Only path arguments are checked.
- A link created between the check and the command's use of the path is not caught. The root is meant for a folder that
  only the server writes, so do not let users create links in it by other means.
- It does not limit disk use, the number of files, or how long a call runs. A call is bounded by stetdoc's input limits,
  and a worst-case call on a 300-page document takes about a second.

So:

- Run the server as an unprivileged user that owns the root and nothing else, or in a container with only the root
  mounted.
- Put authentication in front of every server, and TLS on every connection that leaves the machine.
- Treat document content as untrusted input. stetdoc never touches the network and fails on hostile input with an error
  code, but the text it shows the agent can contain instructions written by whoever wrote the document.

See [`SECURITY.md`](../SECURITY.md) for how hostile input is tested and how to report a vulnerability.

## A container

This Dockerfile downloads the release archive for the platform it builds on, checks the archive's SHA-256 against the
release's `.sha256` file the way [`packaging/install.sh`](../packaging/install.sh) does, and runs stetdoc behind
supergateway as an unprivileged user, with `/data` as the root. Linux gets the static musl build, as it does from the
install script. `STETDOC_VERSION` pins the release.

```dockerfile
FROM debian:bookworm-slim AS download
ARG STETDOC_VERSION=0.9.0
ARG TARGETARCH
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
 && arch="${TARGETARCH:-$(dpkg --print-architecture)}" \
 && case "$arch" in amd64) cpu=x86_64 ;; arm64) cpu=aarch64 ;; *) echo "no stetdoc build for $arch" >&2; exit 1 ;; esac \
 && name="stetdoc-$cpu-unknown-linux-musl" \
 && base="https://github.com/clauseo-ai/stetdoc/releases/download/v$STETDOC_VERSION" \
 && cd /tmp \
 && curl -fsSLO "$base/$name.tar.gz" \
 && curl -fsSLO "$base/$name.tar.gz.sha256" \
 && sha256sum -c "$name.tar.gz.sha256" \
 && tar -xzf "$name.tar.gz" \
 && install -m 0755 "$name/stetdoc" /usr/local/bin/stetdoc

FROM node:22-bookworm-slim
RUN npm install -g supergateway@4.1.0 \
 && useradd --system --create-home --uid 10001 stetdoc \
 && mkdir /data && chown stetdoc /data
COPY --from=download /usr/local/bin/stetdoc /usr/local/bin/stetdoc
USER stetdoc
EXPOSE 8000
CMD ["supergateway", "--stdio", "stetdoc mcp --root /data", "--outputTransport", "streamableHttp", "--stateful", "--host", "0.0.0.0", "--port", "8000"]
```

A checksum that does not match stops the build at `sha256sum -c`, before anything copies the binary. Build and run it
from the folder that holds the Dockerfile:

```
docker build -t stetdoc-mcp .
docker run -d --name stetdoc-alice -p 127.0.0.1:8001:8000 -v /srv/stetdoc/alice:/data \
  -e STETDOC_AUTHOR="Alice Example" -e TZ=Europe/London stetdoc-mcp
```

The image sets no author and holds no config file, so each container gets its user's name from `STETDOC_AUTHOR`, or
from the calls. uid 10001 must be able to write to the mounted folder. Publish the port on `127.0.0.1` only, run one
container per user with that user's folder mounted at `/data`, and point the proxy at each one. Inside the container
the bridge listens on every interface, because the container's network is the boundary.
