# SniffCSS — Docker Hub Overview

## O que é

**SniffCSS** captura o **computed style real** de elementos de uma página via
**Chrome DevTools Protocol (CDP)**, com saída estruturada e determinística para
desenvolvimento assistido por IA. Esta imagem é **self-contained** — Chromium +
toolchain em um único container, independente do host.

- **Fidelidade-first**: o Chromium da GUI (`http://localhost:3001`) roda com
  **FullColor 4:4:4** (cores reais de 8 bits) e expõe CDP em loopback.
  `sniffCSS` e `sniffCSS-mcp` **anexam ao mesmo browser que você vê na tela** —
  o que aparece na GUI é exatamente o que é capturado.
- **Multi-arch**: `linux/amd64` + `linux/arm64`.

## Quickstart

```bash
docker run -d --name sniffcss \
  -p 3000:3000 -p 3001:3001 \
  -v "$(pwd)/sniffcss-config:/config" \
  --shm-size 1gb \
  stallonels/sniffcss:latest
```

- **GUI**: `http://localhost:3001` (Chromium com FullColor 4:4:4)
- **Snapshots**: persistidos em `./sniffcss-config/sniffCSS/`

### Capture

```bash
docker exec sniffcss sniffCSS \
  -u http://localhost:3000 -s ".btn-primary" \
  --depth 0 --wait "network-idle:1200:60000"
```

### Diff / checks (determinísticos, offline)

```bash
docker exec sniffcss sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5
docker exec sniffcss sniffCSS-check --input snap.jsonl --uniform --rules
```

## MCP (agentes de IA)

O `sniffCSS-mcp` roda via **stdio** dentro do container, anexando ao Chromium da
GUI. Configuração no agente:

```json
{
  "mcpServers": {
    "sniffcss": {
      "command": "docker",
      "args": ["exec", "-i", "sniffcss", "sniffCSS-mcp"]
    }
  }
}
```

Ferramentas: `sniffCSS_page`, `sniffCSS_diff`, `sniffCSS_check`,
`sniffCSS_snapshots`, `sniffCSS_categories`. O `sniffCSS_page` persiste cada
captura e retorna só um `__sniff` reference — o JSONL completo **nunca** entra
no contexto do LLM.

## Variáveis de ambiente (defaults)

| Var                        | Default                                        | Descrição |
|----------------------------|------------------------------------------------|-----------|
| `SNIFF_CONNECT`            | `http://127.0.0.1:9222`                        | CDP do Chromium da GUI (loopback) |
| `SNIFF_SNAPSHOT_DIR`       | `/config/sniffCSS`                             | Onde o MCP persiste snapshots |
| `SNIFF_DEFAULT_HEADERS`    | —                                              | JSON de headers HTTP aplicados a todo request do MCP (ex. `{"X-CMS-AI-Token":"<token>"}`) — auth de área restrita sem repetir por chamada |
| `SNIFF_STORAGE_STATE`      | —                                              | Path de estado de sessão (cookies + `localStorage`) restaurado antes de toda navegação |
| `SNIFF_BASE_URL`           | —                                              | Base URL prefixada a `url` relativas (ex. `cms/dashboard` → `http://localhost:10011/cms/dashboard`) |
| `SELKIES_H264_FULLCOLOR`   | `true`                                         | FullColor 4:4:4 (fidelidade de cor) |
| `CHROME_CLI`               | `--remote-debugging-port=9222 --remote-allow-origins=*` | Flags do Chromium |
| `PUID` / `PGID`            | `1000` / `1000`                                | UID/GID do usuário linuxserver |
| `TZ`                       | `Etc/UTC`                                      | Fuso horário |

## docker-compose.yml otimizado para integração em projetos

Versão recomendada para usar junto com o seu app (ex.:
`docker-compose.override.yml` no seu projeto): o container **anexa à rede do seu
dev server**, para capturar `http://app:3000` (ou `http://host.docker.internal:PORT`).

```yaml
# docker-compose.override.yml (no SEU projeto)
services:
  sniffcss:
    image: stallonels/sniffcss:latest
    container_name: sniffcss
    # Compartilha a rede com seu app → capture http://app:3000
    network_mode: service:app        # ou: networks: [default] + services do app
    environment:
      - SELKIES_H264_FULLCOLOR=true
      - SNIFF_CONNECT=http://127.0.0.1:9222
      - SNIFF_SNAPSHOT_DIR=/config/sniffCSS
      - PUID=1000
      - PGID=1000
      - TZ=Etc/UTC
    volumes:
      - ./sniffcss-config:/config
    # A GUI do Chromium (porta 3001) fica acessível no host:
    ports:
      - "3001:3001"
    shm_size: "1gb"
    restart: unless-stopped

  # Opcional: GPU do host para renderização acelerada
  #   devices:
  #     - /dev/dri:/dev/dri
  #   environment:
  #     - PIXELFLUX_WAYLAND=true
  #     - DRI_NODE=/dev/dri/renderD128
```

Uso:

```bash
docker compose -f docker-compose.override.yml up -d sniffcss
docker exec sniffcss sniffCSS -u http://app:3000 -s ".btn-primary"
```

> Com `network_mode: service:app` o sniffcss usa a rede do serviço `app`, então
> `http://app:3000` resolve direto; sem isso, use `http://host.docker.internal:<porta>`.

## Integração em CI

```bash
docker run --rm -v "$PWD:/ws" stallonels/sniffcss \
  sniffCSS -u "$URL" -s "$SEL" --output "$PWD/base.jsonl"
docker run --rm -v "$PWD:/ws" stallonels/sniffcss \
  sniffCSS-diff "$PWD/base.jsonl" "$PWD/head.jsonl" --tolerance 0.5
```

## Links

- Repositório: https://github.com/st-all-one/sniff-css
- Releases (binários Linux/macOS/Windows + `install.sh`):
  https://github.com/st-all-one/sniff-css/releases
- Docs: `docs/usage.md`, `docs/ai-usage.md`, `docs/diff-checks.md`
