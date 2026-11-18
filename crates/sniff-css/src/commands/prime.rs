//! Handler de `sniff prime` — protocolo estático, byte-idêntico por versão.

use serde_json::json;

use crate::cli::PrimeArgs;
use crate::output::Output;

/// Versão do binário.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Protocolo **compacto** (default): o essencial para operar.
const PRIME_COMPACT: &str = concat!(
    "sniff — computed styles for LLMs (web via CDP; Flutter via VM Service).\n",
    "CYCLE: sniff capture → sniff check → sniff diff. Capture the truth; analyze offline.\n",
    "\n",
    "WHEN:\n",
    "  capture  real computed styles/a11y from a live page. Default output = summary;\n",
    "           --no-summary = full JSONL for diff/check. v2 context ON by default:\n",
    "           __document/__env/__declared, attrs preset, 12-state matrix (3 viewports ×\n",
    "           light/dark/reduce/coarse). --emulate K=V = one profile; --minimal = no v2 facets.\n",
    "  check    offline bugs + modern opportunities (default). --rules = bugs only,\n",
    "           --modern = opportunities only, --no-modern = 0.5 behavior,\n",
    "           --no-responsive disables cross-state checks. Lines carry class/guide/evidence.\n",
    "  diff     deterministic offline delta. snapshots / init / self / prime as before.\n",
    "\n",
    "INTERPRET: class:\"bug\" + status fail/warn = measured, actionable.\n",
    "  class:\"opportunity\" = heuristic platform debt: read evidence.source + guide;\n",
    "  breakpoint-jump, dark-mode-ignored, inline-nontrivial-style and transition-absence\n",
    "  need human confirmation. contrast unknown = manual review. Facet absent = silent.\n",
    "  Never paste full JSONL to the model — diff/check first.\n",
    "\n",
    "ESSENTIAL:\n",
    "  sniff capture <URL> -s <CSS> [--depth N] [--action click:<SEL>|key:Escape]\n",
    "  sniff check <SNAP> [--rules|--modern|--no-modern] [--no-responsive]\n",
    "  sniff diff <BASE> <HEAD> [--tolerance 0.5] [--stats-only]\n",
    "  sniff init [DIR] [--force]  # --force overwrites AGENTS.md\n",
    "  INPUT: positional operand, OR --params '<json>', OR --batch file.jsonl (- = stdin).\n",
    "  TLS: --ignore-certificate-errors. AUTH: --header 'Name: Value'; --storage-state FILE.\n",
    "\n",
    "OUTPUT: stdout = data, stderr = logs. --json = machine envelope\n",
    "        {success, command, data?, error{code,message,retryable}, warnings?}.\n",
    "EXIT: 2 invalid, 3 not_found, 5 io, 70 internal.\n",
    "RULES: capture is the source of truth; never invent styles."
);

/// Protocolo **completo** (`--long`): todos os verbos e o contrato de máquina.
const PRIME_FULL: &str = concat!(
    "sniff — computed styles for LLMs.\n",
    "BACKENDS: web (Chromium/CDP, default) | flutter (Dart VM Service, `flutter://<device>`).\n",
    "CATEGORIES: box-model, layout, typography, visual, transform, animation, interaction,\n",
    "            accessibility, all.\n",
    "OUTPUT FORMATS: summary (default, token-lean) | jsonl | jsonl-flat | json.\n",
    "\n",
    "CYCLE: sniff capture (source of truth) → sniff check (bugs + opportunities) → sniff diff.\n",
    "  Capture once, then analyze offline — check/diff never touch a browser.\n",
    "\n",
    "VERBS:\n",
    "  sniff capture <URL> -s <CSS> [--depth N] [--categories LIST] [--props P]\n",
    "     [--pseudo ::before,::after] [--wait SPEC] [--action SPEC] [--click SEL]\n",
    "     [--hover SEL] [--type SEL:TXT] [--upload SEL:FILE] [--header 'Name: Value']\n",
    "     [--viewport WxH] [--viewport-matrix WxH,WxH] [--emulate K=V]\n",
    "     [--declared] [--declared-limit N] [--attrs-preset modern] [--minimal]\n",
    "     [--backend web|flutter|auto] [--device SERIAL] [--avd NAME] [--connect ENDPOINT]\n",
    "     [--ignore-certificate-errors] [--storage-state FILE] [--save-storage-state FILE]\n",
    "     [--screenshot PATH] [--fullpage-screenshot] [--persist [PATH]]\n",
    "     [--output summary|jsonl|jsonl-flat|json] [--pretty] [--full]\n",
    "     [--no-visible --no-aria --no-contrast --no-ax ...].\n",
    "  DEFAULTS: v2 context ON (__document/__env/__declared/attrs), pseudo ::before/::after,\n",
    "  12 states (3 viewports × light/dark/reduce/coarse), __actions even when empty.\n",
    "  --emulate K=V reduces to one profile; --minimal removes all v2 facets.\n",
    "  sniff check <JSONL> [--rules] [--uniform] [--modern|--no-modern] [--guide ID]\n",
    "     [--ignore CHECK] [--profile default|strict] [--responsive|--no-responsive]\n",
    "     [--tolerance 0.5]. Default: uniformity + bug rules + modern opportunities;\n",
    "     node rules run on the first matrix state only, cross-state checks use all states.\n",
    "  sniff diff <BASE> <HEAD> [--tolerance 0.5] [--ignore-props a,b] [--no-structural]\n",
    "     [--stats-only] [--no-actions].\n",
    "  sniff snapshots [--root DIR]   # default: SNIFF_SNAPSHOT_DIR or ./sniff\n",
    "  sniff init [DIR] [--force]   # AGENTS.md managed block; --force overwrites\n",
    "  sniff prime [--long]\n",
    "  sniff self setup <claude|cursor|codex|pi> | completions <bash|zsh|fish> |\n",
    "             upgrade [--dry-run] [--version TAG] [--script PATH] | version\n",
    "\n",
    "INPUT: positional operand, OR --params '<json>' (one operation), OR --batch file.jsonl\n",
    "       (one op per line; - = stdin). `-` in any input reads stdin; a required operand\n",
    "       absent + piped stdin is filled from stdin. --params is a batch of one.\n",
    "       Over sniff.batch_max=100 items, or duplicate sources, exit 2. --dry-run validates.\n",
    "\n",
    "ACTIONS: click:<selector>[:timeout[:settle]] | hover:<selector> | type:<selector>:<text> |\n",
    "         upload:<selector>:<file1,file2> | key:<name>[:timeout[:settle]]. Repeatable and\n",
    "         ORDERED; reveals post-action DOM. key:Escape dispatches a real key event.\n",
    "\n",
    "INTERPRET: class:\"bug\" + status fail/warn = measured, actionable. class:\"opportunity\"\n",
    "  = heuristic platform debt: confirm evidence.source + guide before acting.\n",
    "  breakpoint-jump, dark-mode-ignored, inline-nontrivial-style and transition-absence\n",
    "  need human confirmation. contrast unknown = manual review. Facet absent = silent.\n",
    "  --profile strict escalates opportunities to fail; --ignore CHECK suppresses one id.\n",
    "\n",
    "OUTPUT: stdout = data, stderr = logs. --json = {success, command, data?, error{code,message,retryable}, warnings?}\n",
    "EXIT: 2 invalid, 3 not_found, 5 io, 70 internal.\n",
    "RULES: capture is the source of truth; check/diff are deterministic and offline; never invent styles."
);

/// `sniff prime [--long]`.
pub(crate) fn run(args: &PrimeArgs) -> Output {
    let body = if args.long { PRIME_FULL } else { PRIME_COMPACT };
    let data = json!({ "version": VERSION, "long": args.long, "protocol": body });
    Output::new(body, data)
}
