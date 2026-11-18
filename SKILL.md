---
name: sniff-css
description: Use when working with computed-style capture, accessibility inspection, or visual regression via the sniff toolset. Covers capture, deterministic diff, offline checks, modern opportunities and MCP/CLI usage. Trigger on: sniff, computed-style, cdp, css, contrast, a11y, diff, snapshot, regression, layout.
---

# sniff — Active Usage Guide

**Role.** Capture real computed CSS + accessibility state from a live page over
raw CDP (or a Flutter debug build via the Dart VM Service), then diff/check the
JSONL **offline**. The snapshot is the source of truth; the AI reads only
evidence/delta. Never guess styles.

**When to load.** Any rendered-style question, accessibility audit, visual
regression, or "what actually changed?" investigation.

> Details live in `docs/`. This file is a decision index. Full flag reference:
> [`docs/usage.md`](docs/usage.md). CLI surface:
> [`docs/cli-surface.md`](docs/cli-surface.md). Checks/opportunities:
> [`docs/diff-checks.md`](docs/diff-checks.md). Accessibility:
> [`docs/accessibility.md`](docs/accessibility.md). Determinism:
> [`docs/golden-run.md`](docs/golden-run.md).

---

## Pipeline

```
sniff capture → sniff check → sniff diff → AI interprets only evidence/delta
```

`sniff init` writes the managed protocol into the project's `AGENTS.md`;
`sniff prime` prints the same protocol (static, `--long` for the full contract).
Everything before the AI is deterministic and offline after capture.

### Defaults that matter

| Area | Default | Escape hatch |
|---|---|---|
| Output | `summary` digest (token-lean) | `--no-summary` → full JSONL for diff/check/jq |
| v2 context | `__document`, `__env`, `__declared`, attrs preset `modern` | `--no-document`, `--no-env`, `--no-attrs`, `--minimal` |
| Pseudo styles | `::before`,`::after` | `--pseudo ...` |
| Matrix | 3 viewports (1366×768, 360×800, 768×1024) × 4 profiles (light, dark, reduce, coarse) = **12 states** | `--emulate K=V` → one profile; `--minimal` → no matrix |
| Actions map | `__actions` emitted even when empty | `--no-effects` |
| `sniff check` | uniformity + bug rules + modern opportunities, `--responsive` on | `--rules`, `--modern`, `--no-modern`, `--no-responsive` |
| Node checks | first matrix state only; cross-state checks use all states | — |
| Stabilization | animations frozen, reduce emulated unless overridden | `--no-stabilize`, `--emulate prefers-reduced-motion=no-preference` |
| Profile | opportunities are `warn` | `--profile strict` → `fail` for CI |

Cost note: the 12-state capture is intentionally heavier (~20–30 s, hundreds of
KB). Use `--emulate` or `--minimal` when the question does not need the matrix.

### Quick reference

| Action | Command |
|---|---|
| Capture (default digest) | `sniff capture URL -s SEL --depth N` |
| Full snapshot (diff/check input) | `sniff capture URL -s SEL --depth N --no-summary` |
| Full-fidelity capture | `sniff capture URL -s SEL --depth N --no-summary --full` |
| Accessibility subtree | `sniff capture URL -s SEL --no-summary --ax-tree` |
| Reveal action-only UI | `sniff capture URL -s ".modal" --click "#open"` |
| Ordered mixed flow | `sniff capture URL -s SEL --action "click:#open:5000" --action "type:#q:shoes"` |
| Real key event | `sniff capture URL -s "dialog" --action "key:Escape"` |
| Real file upload | `sniff capture URL -s ".cropper" --action "upload:#file:/tmp/img.png"` |
| Auth header / session | `--header "X-CMS-AI-Token: T"` · `--storage-state FILE` · `--save-storage-state FILE` |
| Self-signed TLS | `--ignore-certificate-errors` (or `SNIFF_IGNORE_CERTIFICATE_ERRORS=1`) |
| Diff / CI scale | `sniff diff base.jsonl head.jsonl --tolerance 0.5` · `--stats-only` |
| Checks | `sniff check snap.jsonl` · `--rules` · `--modern` · `--no-modern` |
| One structured call / batch | `--params '<json>'` · `--batch file.jsonl` (`-` = stdin) |
| Piping | `sniff capture URL -s SEL --no-summary \| sniff check -` |
| Persist / list | `--persist [PATH]` · `sniff snapshots` |
| Onboard / protocol | `sniff init [DIR]` (`--force` overwrites `AGENTS.md`) · `sniff prime [--long]` |
| Flutter (debug build) | `sniff capture flutter://emulator-5554 --project DIR --depth N` |
| Binary utilities | `sniff self setup pi` · `sniff self completions bash` · `sniff self upgrade --dry-run` |

**Prefer the CLI** when a shell exists. MCP (`sniff-mcp`) is a wrapper for
MCP-native harnesses; use persisted paths (`base_path`, `head_path`, `path`),
never inline JSONL, to keep full snapshots out of the model context.

---

## Interpreting results

### Bug rules (`class:"bug"`)

| Evidence | Meaning / action |
|---|---|
| `contrast.aa == "fail"` | AA failure (1.4.3). Cite `evidence.observed`. |
| `contrast.aa == "unknown"` | Background image involved → **manual review**, not pass/fail. |
| `accessibility_grade == "NONE"` | Hidden from AT (`aria-hidden`, `hidden`, `inert`, zero size). |
| `accessibility_grade == "AA"` | In AX tree but off-screen/transparent/name-required role unnamed. |
| `accessibility_grade == "AAA"` | On screen, exposed, named when required. |
| Empty `aria.name` on A/BUTTON/IMG | Candidate 1.1.1/2.4.4/4.1.2 — first check for descendant `<img alt>`. |
| Interactive `rect` < 24px | 2.5.8 target-size failure. |
| `display_visible:true` + grade `AA` | Below the fold — **not** a failure. |
| `occluded` fail/warn | ≥75% / 50–75% covered by another painted node. |
| `backdrop-over-modal` fail | Scrim paints over modal content — check stacking order. |
| `horizontal-overflow`, `sticky-in-overflow-hidden`, `fixed-broken-by-transform` | Deterministic positioning bugs. |

### Modern opportunities (`class:"opportunity"`)

Platform-debt hints derived only from the snapshot. `warn` by default; `fail`
appears only with an explicit WCAG/WAI-ARIA criterion. Fields: `guide` (corpus
id), typed `evidence` (`source`, optional `metric`/`observed`/`threshold`),
`severity`. `keepWhen` is documented in `docs/modernization/findings/` and
tested, but deliberately **not serialized**.

**Triage order:** filter by `class` → read `evidence.source` → open `guide` →
check `keepWhen` → confirm on the rendered page → only then change code. Use
`--ignore CHECK` for a single noisy id, never a blanket suppression.

**Heuristics needing human confirmation (not bugs):**

- `breakpoint-jump` — large vertical shifts between matrix states are often
  legitimate reflow.
- `dark-mode-ignored` — transparent/inherited backgrounds can look "unchanged".
- `overflow-only-at-width` — confirm the overflowing element, not just the body.
- `inline-nontrivial-style`, transition-absence and `@view-transition` checks —
  low-severity modernization prompts.

**False positives already fixed** (regression tests exist): same-origin SRI with
explicit `:443`/`:80`, pseudo `content:none`, viewport-proportional/height
reflow in `breakpoint-jump`, and node rules repeated across matrix states.
If a new one appears, add a `keepWhen` + negative test rather than weakening
the check globally.

Facet absent ⇒ check **silent**, never `pass`. Do not paste full JSONL into the
model; diff/check first.

---

## Workflows

### Accessibility audit

```bash
sniff capture "$URL" -s "body"   --depth 5 --no-summary --ax-tree > body.jsonl
sniff capture "$URL" -s "main"   --depth 5 --no-summary > main.jsonl
sniff capture "$URL" -s "footer" --depth 6 --no-summary > footer.jsonl
sniff check main.jsonl --rules
```

### CI regression

```bash
sniff capture "$URL" -s "$SEL" --no-summary --stable-key data-testid > base.jsonl
# ... deploy ...
sniff capture "$URL" -s "$SEL" --no-summary --stable-key data-testid > head.jsonl
sniff diff base.jsonl head.jsonl --stats-only
sniff diff base.jsonl head.jsonl --ignore-props transform,opacity --no-structural > delta.jsonl
```

### Interactions

`click`/`hover` selectors may contain `:` (pseudo-classes like
`:nth-child(2)`); only trailing all-digit fields are timeout/settle. For
`type`/`upload`, the first `:` splits selector from text/files — use an
attribute/`aria-label` anchor. Actions are ordered, trusted input events; the
wait pipeline runs against the post-interaction DOM. `key:Escape` requests
native dialog dismissal. `__actions` records appeared/removed/changed/moved,
rects, on-screen state and distance; `sniff diff` compares both sides.

### Auth / restricted pages

```bash
sniff capture "$URL/login" -s ".dashboard" \
  --type "#email:admin@x.com" --type "#password:secret" \
  --click "button[type=submit]" --save-storage-state /tmp/cms-state.json
sniff capture "$URL/cms/dashboard" -s "main" --storage-state /tmp/cms-state.json
```

MCP team defaults: `SNIFF_DEFAULT_HEADERS`, `SNIFF_STORAGE_STATE`,
`SNIFF_BASE_URL` — per-call values win on collision.

---

## Installation, onboarding, release

```bash
curl --proto '=https' --tlsv1.2 -sSf \
  https://raw.githubusercontent.com/st-all-one/sniff-css/main/install.sh | sh
# pinned: ... | VERSION=v0.6.0 sh
sniff init          # upsert managed AGENTS.md block
sniff init --force  # overwrite AGENTS.md (destructive)
```

Binaries: Linux glibc/musl (x86_64/aarch64), macOS (arm64), Windows; checksums
per GitHub Release; installed to `~/.local/bin` (legacy `sniffCSS*` removed).
Docker: `stallonels/sniffcss` ([docs/docker.md](docs/docker.md)).
Release trigger: `git tag vX.Y.Z && git push origin vX.Y.Z` →
`.github/workflows/release.yml`. MSRV 1.99. History: [`CHANGELOG.md`](CHANGELOG.md).

---

## Anti-patterns

- Diffing different modes (`default` vs `--full`), viewports or wait strategies.
- Feeding full snapshots to the model instead of diff/check output.
- Treating an opportunity as a bug, or `unknown` contrast as pass.
- Blanket `--ignore` without checking `keepWhen`.
- Inline JSONL in MCP calls when a persisted path exists.
- `sniff init --force` on a project you do not own.
- Raising `--tolerance` to swallow real changes.
- Waiting on `element-ready` for a carousel's hidden first slide.

## Known limitations

- Link name from inner `<img alt>` not derived — verify before reporting.
- Contrast over background images is always `unknown`.
- Hidden panels remain in the AX tree (grade `AA`, `display_visible:true`).
- Actions are scoped to the main frame; `type` appends, it does not clear.
- `upload` paths resolve **inside the browser process** (mount files in containers).
- Storage-state export covers the current origin only; cookies use the whole
  browser jar (avoid `--connect`/shared MCP browsers for different accounts).
- `evidence` can echo URLs/CSP tokens — treat check reports as sensitive.
- Flutter needs a debug build; `rect` is in device coordinates; `hover`/`upload`
  are web-only.
- Server-side data/payloads and stale page caches are out of scope.

## Checklist

- [ ] `sniff init` run; `sniff prime` consulted for the full contract.
- [ ] CLI preferred over MCP when a shell is available.
- [ ] `--no-summary` used only when full JSONL is required.
- [ ] Same mode/viewport on both diff sides; stable keys across deploys.
- [ ] `sniff check` run before the AI judges; only evidence/delta sent.
- [ ] `class` respected; heuristic opportunities confirmed manually.
- [ ] Diff/check use persisted paths, not inline JSONL.
- [ ] `--ax-tree` / `--screenshot` when human-level detail is needed.
- [ ] Evidence cited in the evaluation reason.
