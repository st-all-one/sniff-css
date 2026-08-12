# sniffCSS + linuxserver/chromium — Docker container
#
# Por padrão os binários PRÉ-COMPILADOS são baixados do GitHub Release que
# corresponde a $VERSION (ARG), um por arquitetura (linux/amd64 ou linux/arm64,
# ambos musl estático). Assim a imagem não compila nada e fica leve e rápida.
#
# Fallback para desenvolvimento local (sem Release publicado ainda):
#   docker build -f docker/Dockerfile --build-arg BUILD_FROM_SOURCE=1 .
# o que compila do fonte no stage `builder` (com a toolchain Rust embarcada).
#
# Variáveis:
#   ARG VERSION           tag do release (ex.: v0.2.0) — default latest
#   ARG TARGETARCH        definido pelo buildx (amd64/arm64)
#   ARG BUILD_FROM_SOURCE 0 (download do Release) ou 1 (build do fonte)

# ── Stage 1a: build do fonte (usado só quando BUILD_FROM_SOURCE=1) ──────────
FROM rust:1.97-bookworm AS builder

ARG BUILD_FROM_SOURCE=0
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
# sniff-css-mcp embeds docs/eval-prompt.md, docs/golden-run.md e
# docs/sniffCSS-eval.schema.json em tempo de compilação.
COPY docs ./docs

RUN if [ "${BUILD_FROM_SOURCE}" = "1" ]; then \
        cargo build --release --workspace --bins \
        && mkdir -p /out \
        && cp target/release/sniffCSS target/release/sniffCSS-diff \
           target/release/sniffCSS-check target/release/sniffCSS-mcp /out/; \
    else \
        mkdir -p /out; \
    fi

# ── Stage 1b: fonte de verdade dos binários ─────────────────────────────────
# Se BUILD_FROM_SOURCE=1, usa o que o builder compilou. Caso contrário baixa
# os binários do GitHub Release e verifica o checksum SHA-256.
FROM alpine:3.20 AS downloader

ARG BUILD_FROM_SOURCE=0
ARG VERSION=latest
ARG TARGETARCH=amd64
ARG REPO=st-all-one/sniff-css
# Host do GitHub (override para testes; default é o real).
ARG BASE_HOST=https://github.com

# Reaproveita o /out do builder (vazio se BUILD_FROM_SOURCE=0).
COPY --from=builder /out/ /out/

RUN if [ "${BUILD_FROM_SOURCE}" = "1" ]; then \
        echo "usando binários compilados do fonte"; \
    else \
        set -eu; \
        apk add --no-cache curl tar; \
        # resolve a tag real se pediram latest (segue o redirect do /releases/latest)
        V="${VERSION}"; \
        if [ "${V}" = "latest" ]; then \
            V="$(curl -fsSL -I -o /dev/null -w '%{url_effective}' "${BASE_HOST}/${REPO}/releases/latest")"; \
            V="${V##*/tag/}"; \
        fi; \
        VER="${V#v}"; \
        # linuxserver/chromium roda em amd64 ou arm64 → binário musl estático
        case "${TARGETARCH}" in \
          amd64) TRIPLE="x86_64-unknown-linux-musl" ;; \
          arm64) TRIPLE="aarch64-unknown-linux-musl" ;; \
          *) echo "TARGETARCH não suportado: ${TARGETARCH}" >&2; exit 1 ;; \
        esac; \
        ASSET="sniff-css-${VER}-${TRIPLE}.tar.gz"; \
        BASE="${BASE_HOST}/${REPO}/releases/download/${V}"; \
        mkdir -p /tmp/dl; \
        curl -fsSL -o "/tmp/dl/${ASSET}" "${BASE}/${ASSET}"; \
        curl -fsSL -o /tmp/dl/sha256sums.txt "${BASE}/sha256sums.txt"; \
        ( cd /tmp/dl && grep -F "  ${ASSET}" sha256sums.txt > checksum.one \
            && sha256sum -c checksum.one ); \
        tar -xzf "/tmp/dl/${ASSET}" -C /out; \
    fi

# ── Stage 2: runtime ─────────────────────────────────────────────────────────
FROM lscr.io/linuxserver/chromium:latest@sha256:348379fe0ff2a8d96d1085c568936730f7ae60b8d354fc728848164298fe18c8

# Binários reais fora do caminho; /usr/local/bin tem os wait-wrappers.
COPY --from=downloader /out/sniffCSS        /opt/sniffcss/bin/sniffCSS
COPY --from=downloader /out/sniffCSS-diff   /opt/sniffcss/bin/sniffCSS-diff
COPY --from=downloader /out/sniffCSS-check  /opt/sniffcss/bin/sniffCSS-check
COPY --from=downloader /out/sniffCSS-mcp    /opt/sniffcss/bin/sniffCSS-mcp

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
