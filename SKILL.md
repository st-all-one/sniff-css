---
name: sniff-css
description: Use when working with computed-style capture, accessibility inspection, or visual regression via the sniffCSS toolset. Covers capture, deterministic diff, a11y checks, and MCP/CLI usage. Trigger on keywords: sniff, computed-style, cdp, css, contrast, accessibility, ax, a11y, diff, snapshot, regression, styling, layout, is_user_noticeable.
---

# sniffCSS — Active Usage Guide

**Role:** Capture real computed CSS + accessibility state from a live page over raw CDP (WebSocket), diff two versions deterministically, and emit offline PASS/WARN/FAIL checks. The JSONL snapshot IS the source of truth; the AI interprets only the delta.

**When to load:** Any time you need to inspect real rendered styles, audit accessibility, detect UI regressions, or answer "what actually changed?" on a live page.

---

## Core Pipeline

```
capture → diff → checks → AI interpretation (only the delta)
sniffCSS   sniffCSS-diff   sniffCSS-check / sniffCSS_check   eval-prompt
```

All steps before the AI are deterministic and cost ~0 tokens.

### Quick Reference

| Action | Command |
|--------|---------|
| Capture a node/subtree (default is AI-optimized) | `sniffCSS -u URL -s SEL --depth N` |
| Full-fidelity capture | `sniffCSS -u URL -s SEL --depth N --full` |
| Accessibility capture | `sniffCSS -u URL -s SEL --depth N --ax-tree` (contrast/ax já ON) |
| Diff two snapshots | `sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5` |
| Diff summary (CI) | `sniffCSS-diff base.jsonl head.jsonl --stats-only` |
| Offline checks | `sniffCSS-check --input snap.jsonl --uniform --rules` |
| MCP capture (low-token) | `sniffCSS_page` (persists + returns `__sniff` reference) |
| MCP diff (low-token) | `sniffCSS_diff` with `base_path`/`head_path` |
| MCP checks (low-token) | `sniffCSS_check` with `path` |
| MCP list persisted | `sniffCSS_snapshots` |

---

## General orientation (read this first)

The toolset is **measure, don't guess**: everything before the LLM is a
deterministic binary that answers "what *actually* is / changed on this page".
There are three stages plus an MCP server that wraps the first three:

1. **Capture** (`sniffCSS` / MCP `sniffCSS_page`) — reads real computed
   CSS + accessibility state from a live page over raw CDP and emits a **JSONL
   snapshot**. The snapshot is the source of truth.
2. **Diff** (`sniffCSS-diff` / MCP `sniffCSS_diff`) — byte/number-level diff of
   two snapshots → **only what changed** (`CHANGED` per property, plus
   `ADDED`/`REMOVED`).
3. **Checks** (`sniffCSS-check` / MCP `sniffCSS_check`) — offline rules on a snapshot:
   uniformity (odd card) + derived WCAG rules (contrast, target size, focus,
   alt, hidden focusables).
4. **MCP server** (`sniffCSS-mcp`) — same stages as tools over stdio, with a
   **snapshot store** on disk so full JSONL never has to round-trip through the
   LLM context.

Rules of thumb:

- **Never feed a full snapshot to the model** — diff or check it first; the AI
  interprets only the delta/evidence.
- **Determinism is a contract:** both sides of a diff must share URL, selector,
  viewport, wait strategy and mode (default+default or `--full`+`--full`, never
  mixed).
- **The AI only judges:** the measured facets (`contrast.ratio`, `aria.name`,
  `is_user_noticeable.accessibility_grade`, `ax.role`) are evidence the tools
  already computed — cite them, don't re-derive them.
- **Token budget:** capture once, persist, then reference by path
  (`base_path`/`head_path`/`path`) instead of pasting JSONL around.

---

## Capture Commands (`sniffCSS`)

### Core

```bash
# Default is already AI-optimized: compact + custom-props + stabilize +
# contrast + ax are all ON. Just pass url + selector (+ --depth/--stable-key).
sniffCSS -u http://localhost:3000 -s ".btn-primary"
sniffCSS -u "$URL" -s "main" --depth 5 --ax-tree
sniffCSS -u "$URL" -s "nav" --depth 4 > nav.jsonl
sniffCSS -u "$URL" -s "footer" --depth 6 > footer.jsonl
```

**Categories:** `box-model` · `layout` · `typography` · `visual` · `transform` · `animation` · `interaction` · `accessibility` · `all`

### Flags

| Flag | Purpose |
|------|---------|
| `-u, --url` / `-s, --selector` | Page URL + CSS selector (required) |
| `--depth N` | Levels of children (0 = element only) |
| `-c, --categories` | Category subset (default `all`) |
| `--props a,b` / `--pseudo ::before` | Extra props / pseudo-elements |
| `--wait spec` | Repeatable: `delay:ms`, `network-idle:idle[:t]`, `element-ready:sel:cond[:t]`, `fonts-loaded[:t]`, `app-flag:flag[:t]`, `selector:sel[:t]` |
| `--compact` | **default ON** — ~55% fewer tokens (dedup + suppress defaults + scoped css_variables) |
| `--custom-props` | **default ON** — all CSS variables (`--*`), global in `__meta` |
| `--stabilize` | **default ON** — freeze animations/transitions for deterministic snapshots |
| `--contrast` | **default ON** — measured WCAG ratio + AA/AAA per node (effective background resolved in-page) |
| `--ax` | **default ON** — browser AX node (CDP) |
| `--ax-tree` | opt-in — full AX subtree document (implies `--ax`) |
| `--full` | **opt-in** — disable all five optimizers at once (full-fidelity) |
| `--no-compact` / `--no-custom-props` / `--no-stabilize` / `--no-contrast` / `--no-ax` | opt-in fine control, override the defaults individually |
| `--stable-key attr` | Stable selectors (`data-testid`) across deploys |
| `--viewport WxH` | Emulated viewport (default `1366x768`) — affects media queries/%/vh |
| `--connect ws://...` | Attach to an already-running browser (`ws://` direct, or `http://host:port` resolved via `/json/version`; default from `SNIFF_CONNECT` env) |
| `--output jsonl\|jsonl-flat\|json` | Output shape (default `jsonl`) |
| `--no-visible` | Include invisible elements |
| `--exclude sel`, `--min-width`, `--min-height` | Element filters |

### Output node shape (JSONL)

```json
{
  "id": 1, "parent_id": null, "tag": "DIV", "selector": "div#primary",
  "path": "body > main > div.card", "depth": 0,
  "rect": {"x": 8.0, "y": 8.0, "width": 300.0, "height": 56.0},
  "metrics": {"z_index": "auto", "stacking_context": false},
  "is_user_noticeable": {"display_visible": true, "accessibility_grade": "AAA"},
  "computed_style_hash": "afbd33ba764bb8d4",
  "aria": {"role": "button", "name": "Salvar", "focusable": true, "has_text": true},
  "contrast": {"ratio": 5.17, "foreground": "#2563eb", "background": "#ffffff", "large": false, "aa": "pass", "aaa": "fail", "unknown_reason": null},
  "ax": {"role": "button", "name": "Salvar", "focusable": true, "ignored": false},
  "styles": {"box_model": {"width": "300px"}, "typography": {"font-size": "16px"}, "visual": {"background-color": "#2563eb"}},
  "children": []
}
```

---

## Deterministic Diff (`sniffCSS-diff`)

```bash
sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5 > delta.jsonl
sniffCSS-diff base.jsonl head.jsonl --stats-only
# nodes: 14 -> 14 | changed: 1 | added: 0 | removed: 0
```

| Flag | Purpose |
|------|---------|
| `--tolerance N` | Absorb subpixel jitter in the same unit (default `0.5`); `16px` vs `16rem` never equal |
| `--ignore-props a,b` | Volatile props never mark a node changed |
| `--no-structural` | Suppress ADDED/REMOVED (variable-count lists) |
| `--stats-only` | Print only the summary |

**Output:** `CHANGED` (per-property `before`/`after` incl. `styles`, `pseudo`, `aria`, `contrast`, `ax`, `rect`, `metrics`, `is_user_noticeable`), `ADDED`/`REMOVED` (full snapshot), `__diff_summary`.

> ⚠️ **Determinism:** both runs must share URL, selector, viewport, wait, mode (`--compact` both sides, never mixed).

---

## Deterministic Checks (`sniffCSS-check`)

```bash
sniffCSS-check --input snap.jsonl --uniform --tolerance 0.5   # the "odd card"
sniffCSS-check --input snap.jsonl --rules                     # PASS/WARN/FAIL
```

| Check | Detects |
|-------|---------|
| `--uniform` | Sibling instances deviating from the group norm (median/mode) |
| `--rules` → `contrast-aa/aaa` | Uses the measured `contrast` facet; `fail` = real, `warn` = background-image |
| `--rules` → `target-size` | Interactive element < 24×24px (WCAG 2.2) |
| `--rules` → `focus-indicator` | Focusable with suppressed outline and no box-shadow |
| `--rules` → `hidden-focusable` | Focusable with `accessibility_grade == NONE` |
| `--rules` → `empty-alt-image` | Large image with empty `alt` |

---

## MCP Tools (`sniffCSS-mcp`, stdio)

### Snapshot store (default behavior)

Every `sniffCSS_page` call **persists** the snapshot to
`sniffCSS/[domain]/[path]-[selector]-[UTC].jsonl` — relative to the server's
working directory (the project root the MCP server was launched from), or the
`SNIFF_SNAPSHOT_DIR` env var when set. The UTC stamp orders files
chronologically, so "latest capture for a target" = last file in its directory.

The store is why diff/check can run **path-first**: the full JSONL stays on
disk and never enters the tool call or the returned content.

| Tool | Inputs | Returns |
|------|--------|---------|
| `sniffCSS_page` | url, selector, depth, categories, compact, custom_props, stable_key, pseudo, wait, viewport, format, stabilize, contrast, include_ax, ax_tree, **full** (default `false`), **persist** (default `true`), **return** (default `"reference"`) | `{"__sniff": {path, url, selector, nodes}}` by default; full JSONL with `return:"jsonl"` (+ `notifications/progress` per phase) |
| `sniffCSS_snapshots` | domain, target, limit (all optional) | JSONL lines: `{domain, target, path, created_at, size}`, newest first |
| `sniffCSS_diff` | **base_path, head_path** (preferred) or base_jsonl, head_jsonl; tolerance, ignore_props, ignore_structural | CHANGED/ADDED/REMOVED delta + `__diff_summary` |
| `sniffCSS_check` | **path** (preferred) or jsonl; uniform, rules, tolerance | PASS/WARN/FAIL lines + outliers + `__check_summary` |
| `sniffCSS_categories` | — | accepted categories |

### Per-tool orientation

- **`sniffCSS_page`** — the capture tool. The AI-optimized defaults are already
  ON: `compact:true` (dedup, ~55% fewer tokens), `custom_props:true` (CSS
  variables), `stabilize:true` (deterministic snapshots), `contrast:true`
  (measured WCAG ratio) and `include_ax:true` (browser AX node) — plus
  `persist:true` (writes the snapshot to the store) and `return:"reference"`
  (answers with only the tiny `__sniff` line). Reach for `stable_key:
  "data-testid"` for cross-deploy diffs and `ax_tree:true` for the full AX
  subtree. Pass `full:true` for full-fidelity output (disables all five), or set
  any flag to `false` to opt out individually. Use `return:"jsonl"` **only**
  when you genuinely need the inline values; otherwise reference the path later.
- **`sniffCSS_snapshots`** — the memory of the store. When you need a base/head
  pair (or the path of the latest capture for a target), query here instead of
  guessing filenames. Filter `domain`/`target`; `limit` caps the newest N.
- **`sniffCSS_diff`** — pass `base_path`/`head_path` from two `__sniff`
  references (or `sniffCSS_snapshots`) so the full snapshots never enter the
  conversation. Tune `tolerance` (default `0.5`) for subpixel jitter, add
  `ignore_props:["transform","opacity"]` for animated props, and
  `ignore_structural:true` when a list's item count varies by design. The
  returned delta is the input for your evaluation prompt.
- **`sniffCSS_check`** — pass `path` to a persisted snapshot. Runs uniformity
  (odd card out) + derived rules (contrast AA/AAA, 24×24 target size, visible
  focus indicator, hidden focusables, empty alt on large images). The output is
  **measured evidence** to cite in evaluations — no LLM involved.
- **`sniffCSS_categories`** — trivia: lists the accepted `categories` values.

### Low-token workflow (recommended)

```text
1. sniffCSS_page(url, selector, stable_key:"data-testid", ax_tree:true)
   # compact/custom_props/stabilize/contrast/include_ax are default ON
   -> {"__sniff": {"path": "localhost_3000/checkout-form-20260812T101530Z.jsonl", "nodes": 42}}
2. ... change happens / deploy ...
3. sniffCSS_page(same params) -> another __sniff reference
4. sniffCSS_diff(base_path:"<base>", head_path:"<head>", tolerance:0.5)
   -> only the delta + __diff_summary   (full JSONL never in context)
5. sniffCSS_check(path:"<head>") -> PASS/WARN/FAIL evidence
6. sniffCSS_snapshots(domain:"localhost_3000") -> find any base/head pair
```

When an evaluation needs a specific fact the delta omits, read the file
directly from disk rather than re-capturing.

### Notes

- `persist:false` disables disk writes; `return:"reference"` without
  `persist` is rejected.
- Paths in `__sniff`/`sniffCSS_snapshots` are **root-relative** (use them as-is in
  `base_path`/`head_path`/`path`); the server resolves them inside the store
  and rejects anything escaping the root.
- Embedded resources: `sniffCSS://prompts/eval`, `sniffCSS://schemas/eval`,
  `sniffCSS://guides/golden`.

---

## Output Formats

All query commands support `--output jsonl` (nested tree, one line per root), `jsonl-flat` (one node per line with `id`/`parent_id`), `json` (single array), plus `--pretty`.

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

---

## Workflows

### Accessibility audit

```bash
# compact/contrast/ax já vêm ON por padrão; --ax-tree é o único opt-in.
sniffCSS -u "$URL" -s "body" --depth 5 --ax-tree > body.jsonl
sniffCSS -u "$URL" -s "nav"    --depth 4 > nav.jsonl
sniffCSS -u "$URL" -s "main"   --depth 5 > main.jsonl
sniffCSS -u "$URL" -s "footer" --depth 6 > footer.jsonl
sniffCSS-check --input main.jsonl --rules

# contrast failures, missing names, headings
jq -r '.. | objects | select(.contrast.aa == "fail") | [.tag,.selector,.contrast.ratio] | @tsv' main.jsonl
jq -r '.. | objects | select(.tag=="A" or .tag=="BUTTON" or .tag=="IMG")
  | select(.aria.name==null or .aria.name=="") | select(.is_user_noticeable.accessibility_grade!="NONE")
  | [.tag,.selector] | @tsv' body.jsonl
jq -r '.. | objects | select(.tag|test("^H[1-6]$")) | [.tag,(.aria.name//"-")] | @tsv' body.jsonl
```

### Regression monitoring (CI)

```bash
# default otimizado; use --full nos DOIS lados se quiser full-fidelity
sniffCSS -u "$URL" -s "$SEL" --stable-key data-testid > base.jsonl
# ... deploy ...
sniffCSS -u "$URL" -s "$SEL" --stable-key data-testid > head.jsonl
sniffCSS-diff base.jsonl head.jsonl --stats-only   # fail job if changed/added/removed > threshold
sniffCSS-diff base.jsonl head.jsonl --ignore-props transform,opacity --no-structural > delta.jsonl
```

### Debug one element

```bash
sniffCSS -u "$URL" -s ".btn-primary" --categories visual,typography \
  | jq '{color:.styles.visual.color, font:.styles.typography."font-size"}'
```

---

## Common Patterns

### Find the odd card in a grid
```bash
sniffCSS -u "$URL" -s ".card" --depth 1 | sniffCSS-check --input - --uniform
```

### Real AA contrast fails only
```bash
sniffCSS-check --input main.jsonl --rules | jq -r 'select(.check=="contrast-aa" and .status=="fail") | [.tag,.selector,.evidence] | @tsv'
```

### Touch targets below 24px
```bash
jq -r '.. | objects | select(.tag=="A" or .tag=="BUTTON")
  | select(.is_user_noticeable.display_visible==true and (.rect.width<24 or .rect.height<24))
  | [.tag,.selector,(.aria.name//"-")] | @tsv' body.jsonl
```

### Stable subtree for lazy/dynamic content
```bash
sniffCSS -u "$URL" -s "footer" --depth 2 --wait "delay:1500"
```

---

## Docker (self-contained Chromium)

`docker/` ships the full toolchain + Chromium in a container independent of the
host, fidelity-first: the GUI Chromium (`http://localhost:3001`) runs with
**FullColor 4:4:4** by default and a CDP port on loopback (`127.0.0.1:9222`);
`sniffCSS`/`sniffCSS-mcp` **attach to that same browser**, so what you see is
exactly what is captured. `SNIFF_CONNECT=http://127.0.0.1:9222` is the default.

```bash
docker compose -f docker/docker-compose.yml up -d
docker compose -f docker/docker-compose.yml exec sniffcss \
  sniffCSS -u "$URL" -s "$SEL" --stable-key data-testid
```

MCP inside the container (stdio; client launches via `docker exec -i`):
`command: docker`, `args: [compose, -f, docker/docker-compose.yml, exec, -i, sniffcss, sniffCSS-mcp]`.
Snapshots persist to `/config/sniffCSS` (volume `./chromium-config:/config`).
`--connect` also accepts `http://host:port` (resolved via `/json/version`) and
defaults to `SNIFF_CONNECT`.

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

---

## Checklist

Before completing any sniff operation:

- [ ] **Same mode/viewport on both diff sides** (default+default, or `--full`+`--full`)?
- [ ] **Page stable** (`--stabilize` default ON, proper `--wait`)?
- [ ] **Stable keys** (`--stable-key data-testid`) when comparing across deploys?
- [ ] **Optimized default** for LLM consumption (compact/contrast/ax already ON; `--full` only when raw props needed)?
- [ ] **`--ax-tree`** when the full accessibility subtree is required?
- [ ] **`sniffCSS-check` run** before the AI judges (measured evidence)?
- [ ] **Delta only** sent to the model, not full snapshots?
- [ ] **MCP diff/check via paths** (`base_path`/`head_path`/`path`), not inline JSONL?
- [ ] **`sniffCSS_page` on the token-efficient defaults** (`persist:true`, `return:"reference"`)?
- [ ] **Evidence cited** in the evaluation reason (`contrast.ratio`, `aria.role`)?
