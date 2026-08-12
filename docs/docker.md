# Docker

Container **self-contained** do SniffCSS: Chromium + toolchain em um único
container, independente do host.

- **Fidelidade**: o Chromium da GUI (`http://localhost:3001`) roda com
  **FullColor 4:4:4** (`SELKIES_H264_FULLCOLOR=true`) e expõe CDP em
  `127.0.0.1:9222`. O `sniffCSS` e o `sniffCSS-mcp` **anexam ao mesmo browser**
  que você vê na tela — o que aparece na GUI é exatamente o que é capturado.
- **Multi-arch**: `linux/amd64` + `linux/arm64`.
- **Imagem publicada** no [Docker Hub](https://hub.docker.com/r/stallonels/sniffcss)
  (`stallonels/sniffcss`), tag igual à do GitHub Release; `latest` aponta para
  o último.

## Quickstart

### docker run

```bash
docker run -d --name sniffcss \
  -p 3000:3000 -p 3001:3001 \
  -v "$(pwd)/sniffcss-config:/config" \
  --shm-size 1gb \
  stallonels/sniffcss:latest
```

- **GUI**: `http://localhost:3001` (Chromium com FullColor 4:4:4)
- **Snapshots**: persistidos em `./sniffcss-config/sniffCSS/`

Capture:

```bash
docker exec sniffcss sniffCSS \
  -u http://localhost:3000 -s ".btn-primary" \
  --depth 0 --wait "network-idle:1200:60000"
```

### docker compose (repositório)

```bash
docker compose -f docker/docker-compose.yml up -d
docker compose -f docker/docker-compose.yml exec sniffcss \
  sniffCSS -u "$URL" -s "$SEL" --stable-key data-testid
```

## docker-compose.yml otimizado para integração em projetos

Para usar junto com o seu app: o container **compartilha a rede do seu dev
server** (via `network_mode: service:app`), então capture `http://app:3000`.

```yaml
# docker-compose.override.yml (no SEU projeto)
services:
  sniffcss:
    image: stallonels/sniffcss:latest
    container_name: sniffcss
    network_mode: service:app        # usa a rede do seu app → http://app:3000
    environment:
      - SELKIES_H264_FULLCOLOR=true
      - SNIFF_CONNECT=http://127.0.0.1:9222
      - SNIFF_SNAPSHOT_DIR=/config/sniffCSS
      - PUID=1000
      - PGID=1000
      - TZ=Etc/UTC
    volumes:
      - ./sniffcss-config:/config
    ports:
      - "3001:3001"                  # GUI do Chromium no host
    shm_size: "1gb"
    restart: unless-stopped

  # Opcional: GPU do host para renderização acelerada
  #   devices:
  #     - /dev/dri:/dev/dri
  #   environment:
  #     - PIXELFLUX_WAYLAND=true
  #     - DRI_NODE=/dev/dri/renderD128
```

> Com `network_mode: service:app`, `http://app:3000` resolve direto. Sem isso,
> use `http://host.docker.internal:<porta>`.

## MCP via Docker

O `sniffCSS-mcp` roda via **stdio** dentro do container, anexando ao Chromium
da GUI (`SNIFF_CONNECT=http://127.0.0.1:9222` é o default da imagem). O jeito
mais simples (sem compose file, direto do Docker Hub) é usar o wrapper
[`scripts/mcp-docker.sh`](../scripts/mcp-docker.sh): ele cria o container
`stallonels/sniffcss:latest` se necessário, espera o CDP subir e executa o MCP
via `docker exec -i` — o banner de init do linuxserver nunca chega ao stdio, o
que mantém o canal JSON-RPC limpo.

### Configuração no agente (opencode)

```jsonc
// opencode.jsonc
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "sniffcss": {
      "type": "local",
      "command": ["/caminho/para/sniff-css/scripts/mcp-docker.sh"],
      "enabled": true,
      "environment": {
        "SNIFF_CONFIG_DIR": "/tmp/sniffcss-config",   // volume /config (snapshots)
        "RUST_LOG": "warn"
      }
    }
  }
}
```

Variáveis do wrapper: `SNIFF_DOCKER_IMAGE` (default `stallonels/sniffcss:latest`),
`SNIFF_DOCKER_CONTAINER` (default `sniffcss-mcp`), `SNIFF_CONFIG_DIR` (volume
`/config`, onde ficam os snapshots), `SNIFF_DOCKER_SHM` (default `1gb`).

### Configuração no agente (MCP genérico, container já rodando)

```json
{
  "mcpServers": {
    "sniffcss": {
      "command": "docker",
      "args": ["exec", "-i", "sniffcss-mcp", "/opt/sniffcss/bin/sniffCSS-mcp"]
    }
  }
}
```

### Uso manual

```bash
scripts/mcp-docker.sh   # inicia o container se preciso e sobe o MCP via stdio
# ou, com o container já rodando:
docker exec -i sniffcss-mcp /opt/sniffcss/bin/sniffCSS-mcp
```

### Ferramentas e persistência

- Ferramentas: `sniffCSS_page`, `sniffCSS_diff`, `sniffCSS_check`,
  `sniffCSS_snapshots`, `sniffCSS_categories`.
- O `sniffCSS_page` persiste cada captura e retorna só um `__sniff` reference —
  o JSONL completo **nunca** entra no contexto do LLM.
- Snapshots persistidos em `/config/sniffCSS` (volume `./chromium-config:/config`
  no compose do repositório, ou `./sniffcss-config:/config` no override acima).

### Uso manual (fora de agente)

Recomendado (direto do Docker Hub, sem compose):

```bash
scripts/mcp-docker.sh
```

Ou com o container do compose já rodando:

```bash
docker compose -f docker/docker-compose.yml exec -i sniffcss sniffCSS-mcp
```

## Diff / checks dentro do container

```bash
docker compose -f docker/docker-compose.yml exec sniffcss \
  sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5
docker compose -f docker/docker-compose.yml exec sniffcss \
  sniffCSS-check --input snap.jsonl --uniform --rules
```

## Variáveis de ambiente (defaults)

| Var | Default | Descrição |
|---|---|---|
| `SNIFF_CONNECT` | `http://127.0.0.1:9222` | CDP do Chromium da GUI (loopback) |
| `SNIFF_SNAPSHOT_DIR` | `/config/sniffCSS` | Onde o MCP persiste snapshots |
| `SELKIES_H264_FULLCOLOR` | `true` | FullColor 4:4:4 (fidelidade de cor) |
| `CHROME_CLI` | `--remote-debugging-port=9222 --remote-allow-origins=*` | Flags do Chromium |
| `PUID`/`PGID` | `1000`/`1000` | UID/GID do usuário linuxserver |
| `TZ` | `Etc/UTC` | Fuso horário |

## Como a imagem é construída

- Por padrão o `docker/Dockerfile` **baixa os binários pré-compilados do
  GitHub Release** (`--build-arg VERSION=v0.2.1`) e verifica o `sha256sums.txt`
  — não compila nada.
- Para desenvolvimento local sem Release publicado:
  `scripts/docker.sh build-source` (compila do fonte, `--build-arg BUILD_FROM_SOURCE=1`).
- Helpers: `scripts/docker.sh` (`build | build-source | up | down | exec | mcp`).
