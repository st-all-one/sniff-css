---
name: sniff-css
description: Use when working with computed-style capture, accessibility inspection, or visual regression via the sniffCSS toolset. Covers capture, deterministic diff, a11y checks, and MCP/CLI usage. Trigger on keywords: sniff, computed-style, cdp, css, contrast, accessibility, ax, a11y, diff, snapshot, regression, styling, layout, is_user_noticeable.
---

# sniffCSS — Active Usage Guide

**Role:** Capture real computed CSS + accessibility state from a live page over
raw CDP (WebSocket), diff two versions deterministically, and emit offline
PASS/WARN/FAIL checks. The JSONL snapshot IS the source of truth; the AI
interprets only the delta.

**When to load:** Any time you need to inspect real rendered styles, audit
accessibility, detect UI regressions, or answer "what actually changed?" on a
live page.

> **Detail lives in `docs/`.** This guide is the decision-oriented index. Full
> CLI flag reference: [`docs/usage.md`](docs/usage.md). AI consumption flows:
> [`docs/ai-usage.md`](docs/ai-usage.md). Diff/checks:
> [`docs/diff-checks.md`](docs/diff-checks.md). Accessibility audit:
> [`docs/accessibility.md`](docs/accessibility.md). Docker:
> [`docs/docker.md`](docs/docker.md). Determinism contract:
> [`docs/golden-run.md`](docs/golden-run.md).

---

## Core Pipeline

```
capture → diff → checks → AI interpretation (only the delta)
sniffCSS   sniffCSS-diff   sniffCSS-check / sniffCSS_check   eval-prompt
```

All steps before the AI are deterministic and cost ~0 tokens.

### Key capabilities

- **Real interactions** — `--click` / `--hover` / `--type` / `--upload` /
  `--action` dispatch **trusted input events** before capture to reveal
  elements that only exist after an action (modals, dropdowns, hover menus,
  type-ahead) and run real upload handlers. Each action waits for its own
  target and scrolls it into view; the wait pipeline then runs against the
  post-interaction DOM. `--action` is the ordered form for mixed flows
  (modal → mini-modal → input).
- **Header auth + persistent sessions** — `--header "Name: Value"` applies an
  HTTP header to **every** request (`Network.setExtraHTTPHeaders`), so a
  stateless CMS auth needs no URL token / `.env` / proxy;
  `--storage-state` / `--save-storage-state` round-trip a login (cookies +
  localStorage, Playwright `storageState` JSON) across browser restarts. On
  the MCP server, team defaults are set once via `SNIFF_DEFAULT_HEADERS`,
  `SNIFF_STORAGE_STATE` and `SNIFF_BASE_URL` — per-call values win on collision.
- **Deterministic pipeline** — capture → diff → checks are ~0-token binaries;
  the AI interprets only the delta/evidence.
- **Flutter/Dart backend** — `--backend flutter` (or MCP `sniffFlutter_page`)
  captures the widget tree of a **debug-mode** Flutter app on an Android
  emulator/device over the Dart VM Service (`flutter run/attach --machine` +
  `ext.flutter.inspector.*`). Same JSONL model (tag = widget class, colors
  normalized to `#rrggbb`, WCAG contrast derived), so `sniffCSS-diff` /
  `sniffCSS-check` work by path unchanged. Detail:
  [`docs/usage.md`](docs/usage.md#backend-flutterdart---backend-flutter) and
  [`docs/architecture.md`](docs/architecture.md#backend-flutter-sniff-flutter).

### Quick Reference

| Action | Command |
|--------|---------|
| Capture (summary digest is the default) | `sniffCSS -u URL -s SEL --depth N` |
| Full non-summarized snapshot (diff/check input) | `sniffCSS -u URL -s SEL --depth N --no-summary` |
| Full-fidelity capture | `sniffCSS -u URL -s SEL --depth N --no-summary --full` |
| Accessibility capture | `sniffCSS -u URL -s SEL --depth N --no-summary --ax-tree` |
| Reveal action-only elements | `sniffCSS -u URL -s ".modal" --click "#open"` |
| Ordered mixed-flow interactions | `sniffCSS -u URL -s SEL --action "click:#open:5000" --action "type:#q:shoes"` |
| Real file upload (hidden inputs ok) | `sniffCSS -u URL -s ".cropper" --action "upload:#file:/tmp/img.png"` |
| Authenticated capture (header on every request) | `sniffCSS -u URL -s "main" --header "X-CMS-AI-Token: <token>"` |
| Diff two snapshots | `sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5` |
| Diff summary (CI) | `sniffCSS-diff base.jsonl head.jsonl --stats-only` |
| Offline checks | `sniffCSS-check --input snap.jsonl --uniform --rules` |
| Visual evidence | `sniffCSS -u URL -s SEL --screenshot out.png` |
| Verbatim DOM attrs | `sniffCSS -u URL -s SEL --attrs name,value` |
| Flutter app (attach) | `sniffCSS --backend flutter --device emulator-5554 --depth N --no-summary` |
| Flutter app (run AVD) | `sniffCSS --backend flutter --avd pixel --project DIR --depth N --no-summary` |

> **Prefer the CLI.** `sniffCSS`, `sniffCSS-diff` and `sniffCSS-check` stream to
> stdout/redirectable files, accept `--persist` to mirror the MCP store layout,
> and add no client/server indirection. The MCP server (`sniffCSS_page` & co.)
> is a convenience wrapper for MCP-native clients — use it only when the harness
> demands tools (see [AI usage](docs/ai-usage.md)).

---

## Orientation (read this first)

The toolset is **measure, don't guess**: everything before the LLM is a
deterministic binary. There are three stages plus an MCP server that wraps them:

1. **Capture** (`sniffCSS`) — reads real computed CSS + accessibility state via
   raw CDP and emits a **snapshot**. Default output is the **summary digest**
   (token-lean per node); `--no-summary` emits the full JSONL.
2. **Diff** (`sniffCSS-diff`) — byte/number-level diff of two snapshots →
   **only what changed** (`CHANGED` per property, plus `ADDED`/`REMOVED`).
3. **Checks** (`sniffCSS-check`) — offline rules: uniformity (odd card) +
   derived WCAG rules (contrast, target size, focus, alt, hidden focusables).
4. **MCP server** (`sniffCSS-mcp`) — the same stages as tools over stdio, with
   a **snapshot store** on disk so full JSONL never round-trips through the
   LLM context.

Rules of thumb:

- **Never feed a full snapshot to the model** — diff or check it first; the AI
  interprets only the delta/evidence.
- **Determinism is a contract:** both diff sides must share URL, selector,
  viewport, wait strategy and mode (default+default or `--full`+`--full`, never
  mixed).
- **The AI only judges:** the measured facets (`contrast.ratio`, `aria.name`,
  `is_user_noticeable.accessibility_grade`, `ax.role`) are evidence the tools
  already computed — cite them, don't re-derive them.
- **Token budget:** capture once, persist, then reference by path
  (`base_path`/`head_path`/`path`) instead of pasting JSONL around.

---

## Reading the Facets (Interpretation)

| Facet value | Meaning / action |
|---|---|
| `contrast.aa == "fail"` | Real AA failure (1.4.3). Cite the ratio. |
| `contrast.aa == "unknown"` | Background image involved → manual review. |
| `accessibility_grade == "NONE"` | Hidden from AT (`aria-hidden`, `hidden`/`inert`, `display:none`, zero-size). |
| `accessibility_grade == "AA"` | In AX tree but off-screen / transparent / name-required role missing a name. |
| `accessibility_grade == "AAA"` | On screen, exposed, named when required. |
| `aria.name` empty on A/BUTTON/IMG | Candidate 1.1.1/2.4.4/4.1.2. **First check for a descendant `<img alt="...">`** — not yet derived by the tool. |
| No `H1` or skipped levels (H2→H4) | 1.3.1/2.4.6 structure failure. |
| Interactive `rect` < 24px | 2.5.8 target-size failure. |
| `display_visible:true` + grade `AA` | Present and accessible, below the fold — **not** a failure. |

Full facet docs: [`docs/usage.md`](docs/usage.md#formato-de-saída-jsonl) and
[`docs/accessibility.md`](docs/accessibility.md).

---

## Workflows

### Accessibility audit

```bash
sniffCSS -u "$URL" -s "body" --depth 5 --no-summary --ax-tree > body.jsonl
sniffCSS -u "$URL" -s "nav"    --depth 4 --no-summary > nav.jsonl
sniffCSS -u "$URL" -s "main"   --depth 5 --no-summary > main.jsonl
sniffCSS -u "$URL" -s "footer" --depth 6 --no-summary > footer.jsonl
sniffCSS-check --input main.jsonl --rules
```

Complete recipe (facets, jq queries, judgment checklist):
[`docs/accessibility.md`](docs/accessibility.md).

### Regression monitoring (CI)

```bash
sniffCSS -u "$URL" -s "$SEL" --no-summary --stable-key data-testid > base.jsonl
# ... deploy ...
sniffCSS -u "$URL" -s "$SEL" --no-summary --stable-key data-testid > head.jsonl
sniffCSS-diff base.jsonl head.jsonl --stats-only   # fail job if changed/added/removed > threshold
sniffCSS-diff base.jsonl head.jsonl --ignore-props transform,opacity --no-structural > delta.jsonl
```

### Reveal elements that only exist after an interaction

```bash
sniffCSS -u "$URL" -s ".modal" --click "#open-modal"        # click to reveal
sniffCSS -u "$URL" -s ".menu-panel" --hover "#user-menu"    # hover to reveal
sniffCSS -u "$URL" -s ".search-results" --type "#q:shoes"   # type to reveal
sniffCSS -u "$URL" -s ".result" --action "click:#open:5000" \
  --action "type:#q:shoes" --action "click:.result-item"    # ordered mixed flow
sniffCSS -u "$URL" -s "#cropper-wrapper" \
  --action "upload:#media-input:/tmp/pixel.png"             # real file upload
```

Each action waits for its own target, scrolls it into view and dispatches a
**real trusted input event**; the wait pipeline then runs against the
post-interaction DOM. `--stabilize` is re-applied for deterministic snapshots.
Upload works on visually hidden file inputs (the browser fires `change` itself).

### Authenticated / restricted captures

```bash
sniffCSS -u "$URL" -s "main" --header "X-CMS-AI-Token: <token>"   # header auth
sniffCSS -u "$URL/login" -s ".dashboard" \
  --type "#email:admin@x.com" --type "#password:secret" \
  --click "button[type=submit]" --save-storage-state /tmp/cms-state.json
sniffCSS -u "$URL/cms/dashboard" -s "main" --storage-state /tmp/cms-state.json
```

Server-side team defaults (`SNIFF_DEFAULT_HEADERS`, `SNIFF_STORAGE_STATE`,
`SNIFF_BASE_URL`) let the agent skip auth plumbing per call:
[`docs/ai-usage.md`](docs/ai-usage.md).

### Map what happened at the UI level (`__actions`)

With actions set (default ON; `--no-effects` to omit), each action emits an
entry in a reserved `__actions` line: `effect` (`revealed`/`hidden`/`changed`/
`moved`/`no_effect`), appeared/removed/changed elements with `rect`, `onscreen`,
`out_of_view.{above,below,left,right}`, `distance_from_action`, `direction`,
and a deterministic `summary`. `sniffCSS-diff` compares `__actions` blocks when
both sides carry them → `ACTION_CHANGED`/`ACTION_ADDED`/`ACTION_REMOVED`.

---

## Installation & releases

```bash
curl --proto '=https' --tlsv1.2 -sSf \
  https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh | sh
# pinned version:
curl --proto '=https' --tlsv1.2 -sSf \
  https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh | VERSION=v0.4.0 sh
```

Binaries: Linux glibc + musl (x86_64/aarch64), macOS, Windows — per GitHub
Release (semver tags), checksum-verified, installed to `~/.local/bin`. Env
overrides: `INSTALL_DIR`, `SNIFF_TARGET`, `SNIFF_REPO`, `SNIFF_BASE_URL`.
Docker image `stallonels/sniffcss` (multi-arch, self-contained Chromium):
[`docs/docker.md`](docs/docker.md). New release: `git tag vX.Y.Z && git push
origin vX.Y.Z` triggers `.github/workflows/release.yml`; MSRV 1.88. Changes
tracked in [`CHANGELOG.md`](CHANGELOG.md).

---

## Anti-patterns

- Diffing across different modes (default vs `--full`) or viewports → false positives.
- Feeding full snapshots to the model — always diff/check first.
- Passing inline `base_jsonl`/`head_jsonl`/`jsonl` to MCP tools when a persisted
  `path`/`base_path`/`head_path` exists — the inline string re-enters the LLM context.
- Calling `sniffCSS_page` with `return:"jsonl"` when only the reference/path is needed.
- Treating `unknown` contrast as pass/fail — it means "review manually".
- Reporting every empty `aria.name` without checking a descendant `img alt`.
- Flagging below-the-fold content as "invisible" — read `is_user_noticeable`.
- Blindly raising `--tolerance` — it swallows real small changes.
- Waiting on `element-ready` for a carousel's first hidden slide (use `delay` or the stable subtree).

---

## Known Limitations

- Link accessible name from inner `img alt` is not yet derived (verify before reporting).
- Contrast over background images is always `unknown` (cannot measure without pixels).
- Hidden panels (carousels/tabs) are still in the AX tree → grade `AA`, `display_visible:true`.
- Interaction actions are scoped to the main frame (elements inside `<iframe>` are not clickable).
- `type` inserts text (appends to the focused element); it does not clear the field first.
- `upload` file paths are resolved **by the browser process** — in a container the files must exist inside it (mount them). Hidden inputs are supported; non-file inputs error out.
- Session-state export captures `localStorage` only from the current page's origin; sub-origins (CDN hosts) are not represented. Restored cookies are host-scoped (a `.domain` prefix is normalized to host-only).
- CLI action specs split on `:`, so colon-bearing selectors (`.btn:hover`, `[data-x="a:b"]`) must go through the MCP `actions` object instead of `--click`/`--hover`/`--type`/`--upload` strings.
- **DOM attributes are not captured by default** — the tool is computed-style. Use `--attrs a,b` / MCP `attributes` for the specific attributes you need.
- **Server-side data is out of scope** — payloads, templates, routes and backend state are not sniffCSS's job; pair it with `curl`/API inspection when the question is about server data, not rendered styles.
- **The tool captures whatever the server serves** — a stale page cache produces a stale snapshot. Clear/invalidate the cache before capturing.
- **Flutter backend needs a debug build** — release APKs strip the Dart VM Service; `rect` is in device coordinates (not CSS viewport); widgets without a render box report no `rect`.

---

## Checklist

Before completing any sniff operation:

- [ ] **Prefer the CLI** over MCP when a shell is available?
- [ ] **Summary digest is the default** — use `--no-summary` explicitly when the full snapshot (diff/check/jq input) is required?
- [ ] **Same mode/viewport on both diff sides** (default+default, or `--full`+`--full`)?
- [ ] **Page stable** (`--stabilize` default ON, proper `--wait`)?
- [ ] **Stable keys** (`--stable-key data-testid`) when comparing across deploys?
- [ ] **`--ax-tree`** when the full accessibility subtree is required?
- [ ] **`sniffCSS-check` run** before the AI judges (measured evidence)?
- [ ] **Delta only** sent to the model, not full snapshots?
- [ ] **Diff/check via persisted paths** (CLI files or MCP `base_path`/`head_path`/`path`), not inline JSONL?
- [ ] **`--screenshot`/`screenshot:true`** when a human eye on the rendered page is needed?
- [ ] **`--attrs`/`attributes`** for verbatim DOM attributes (e.g. form `name` reindexing)?
- [ ] **Evidence cited** in the evaluation reason (`contrast.ratio`, `aria.role`)?
