//! Handler de `sniff prime` — protocolo estático, byte-idêntico por versão.

use serde_json::json;

use crate::cli::PrimeArgs;
use crate::output::Output;

/// Versão do binário.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Protocolo **compacto** (default): o essencial para operar.
const PRIME_COMPACT: &str = "\
sniff — computed styles for LLMs (web via CDP; Flutter via VM Service).
CYCLE: sniff capture → sniff check → sniff diff.
  capture the truth, check it, diff the before/after.

WHEN (use / don't):
  sniff capture   read real computed styles of a page (selectors, depth, actions).
                  NOT for offline analysis — feed its JSONL to check/diff.
  sniff check     WCAG/UX heuristics + uniformity on a captured JSONL. Offline.
  sniff diff      what changed between two captured JSONL. Offline, deterministic.
  sniff snapshots list persisted snapshots under SNIFF_SNAPSHOT_DIR.
  sniff init      write/update the managed block in the project AGENTS.md
                  (AGENTS.md only; --force overwrites the file).
  sniff self      setup/completions/upgrade/version (binary utilities).
  sniff prime     this protocol (static, cacheable). help = human surface.

ESSENTIAL:
  sniff capture -u <URL> -s <CSS> [--depth N] [--action click:<SEL>] [--output jsonl|summary|json]
  sniff check -i snap.jsonl [--rules] [--uniform]
  sniff diff base.jsonl head.jsonl [--tolerance 0.5] [--stats-only]
  sniff init [--force]
  TLS: --ignore-certificate-errors (self-signed dev servers).
  AUTH: --header 'Name: Value'; keep a login with --storage-state FILE.
  flutter://<device> in --url switches to the Flutter backend (selector defaults to `root`).

OUTPUT: stdout = data (JSONL), stderr = logs. --json = machine envelope
        {success, command, data?, error{code,message,retryable}, warnings?}.
EXIT: 2 invalid, 3 not_found, 5 io, 70 internal.
RULES: capture is the source of truth; never invent styles. Feed JSONL to check/diff.";

/// Protocolo **completo** (`--long`): todos os verbos e o contrato de máquina.
const PRIME_FULL: &str = "\
sniff — computed styles for LLMs.
BACKENDS: web (Chromium/CDP, default) | flutter (Dart VM Service, `flutter://<device>`).
CATEGORIES: box-model, layout, typography, visual, transform, animation, interaction,
            accessibility, all.
OUTPUT FORMATS: summary (default, token-lean) | jsonl | jsonl-flat | json.

CYCLE: sniff capture (source of truth) → sniff check (heuristics) → sniff diff (before/after).
  Capture once, then analyze offline — check/diff never touch a browser.

VERBS:
  sniff capture -u <URL> -s <CSS> [--depth N] [--categories LIST] [--props P] [--pseudo E]
     [--wait SPEC] [--action SPEC] [--click SEL] [--hover SEL] [--type SEL:TXT]
     [--upload SEL:FILE] [--header 'Name: Value'] [--viewport WxH]
     [--backend web|flutter|auto] [--device SERIAL] [--avd NAME] [--connect ENDPOINT]
     [--ignore-certificate-errors] [--storage-state FILE] [--save-storage-state FILE]
     [--screenshot PATH] [--fullpage-screenshot] [--persist]
     [--output summary|jsonl|jsonl-flat|json] [--pretty] [--full]
     [--no-visible --no-aria --no-contrast --no-ax ...].
  sniff check -i <JSONL> [--rules] [--uniform] [--tolerance 0.5]
     rules = contrast/target-size/focus/alt; uniform = odd one out among siblings.
  sniff diff <BASE> <HEAD> [--tolerance 0.5] [--ignore-props a,b] [--no-structural]
     [--stats-only] [--no-actions].
  sniff snapshots [--root DIR]   # default: SNIFF_SNAPSHOT_DIR or ./sniff
  sniff init [--force] [--dir DIR]   # AGENTS.md managed block; --force overwrites
  sniff prime [--long]
  sniff self setup <claude|cursor|codex|pi> | completions <bash|zsh|fish> |
             upgrade [--dry-run] [--version TAG] [--script PATH] | version

ACTIONS: click:<selector>[:timeout[:settle]] | hover:<selector> | type:<selector>:<text> |
         upload:<selector>:<file1,file2>. Repeatable and ORDERED; reveals post-action DOM.

OUTPUT: stdout = data, stderr = logs. --json = {success, command, data?, error{code,message,retryable}, warnings?}
EXIT: 2 invalid, 3 not_found, 5 io, 70 internal.
RULES: capture is the source of truth; check/diff are deterministic and offline; never invent styles.";

/// `sniff prime [--long]`.
pub(crate) fn run(args: &PrimeArgs) -> Output {
    let body = if args.long { PRIME_FULL } else { PRIME_COMPACT };
    let data = json!({ "version": VERSION, "long": args.long, "protocol": body });
    Output::new(body, data)
}
