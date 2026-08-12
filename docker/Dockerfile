# sniffCSS + linuxserver/chromium — Docker container
#
# Two-stage build:
#   1. Build the sniffCSS toolchain (Rust release, LTO).
#   2. Runtime: linuxserver/chromium (digest-pinned) with the GUI Chromium
#      exposing a CDP port, plus sniffCSS/sniffCSS-diff/sniffCSS-check/
#      sniffCSS-mcp installed. sniffCSS attaches to the SAME browser the GUI
#      shows (port 3001) for maximum fidelity; FullColor 4:4:4 is enabled by
#      default so colors are true 8-bit.

# ── Stage 1: build ────────────────────────────────────────────────────────
FROM rust:1.97-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# sniff-css-mcp embeds docs/eval-prompt.md, docs/golden-run.md and
# docs/sniffCSS-eval.schema.json at compile time.
COPY docs ./docs

RUN cargo build --release --workspace --bins

# ── Stage 2: runtime ───────────────────────────────────────────────────────
FROM lscr.io/linuxserver/chromium:latest@sha256:348379fe0ff2a8d96d1085c568936730f7ae60b8d354fc728848164298fe18c8

# Real binaries live out of the way; /usr/local/bin holds wait-wrappers.
COPY --from=builder /src/target/release/sniffCSS      /opt/sniffcss/bin/sniffCSS
COPY --from=builder /src/target/release/sniffCSS-diff /opt/sniffcss/bin/sniffCSS-diff
COPY --from=builder /src/target/release/sniffCSS-check /opt/sniffcss/bin/sniffCSS-check
COPY --from=builder /src/target/release/sniffCSS-mcp  /opt/sniffcss/bin/sniffCSS-mcp

COPY docker/sniffcss-wrapper.sh /opt/sniffcss/wrapper.sh
RUN chmod +x /opt/sniffcss/bin/* /opt/sniffcss/wrapper.sh \
 && for b in sniffCSS sniffCSS-diff sniffCSS-check sniffCSS-mcp; do \
      ln -sf /opt/sniffcss/wrapper.sh /usr/local/bin/"$b"; \
    done

# ── Defaults (overridable) ────────────────────────────────────────────────
# Fidelity: FullColor 4:4:4 encoding so the GUI shows true 8-bit color.
ENV SELKIES_H264_FULLCOLOR=true

# Expose the GUI Chromium's DevTools port so sniffCSS can attach to it. The
# address stays on loopback: sniffCSS runs inside the container (docker exec),
# so the CDP endpoint is never exposed to the host.
ENV CHROME_CLI="--remote-debugging-port=9222 --remote-allow-origins=*"

# Default target for sniffCSS and sniffCSS-mcp: the in-container GUI browser.
ENV SNIFF_CONNECT="http://127.0.0.1:9222"

# Persist snapshots under /config (the linuxserver config volume).
ENV SNIFF_SNAPSHOT_DIR="/config/sniffCSS"

# linuxserver defaults.
ENV PUID=1000 \
    PGID=1000 \
    TZ=Etc/UTC
