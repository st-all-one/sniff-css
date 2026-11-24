//! Bancada A/B do `sniff` — verbos offline `diff` e `check`, sem dependências.
//!
//! ```sh
//! cargo run --release --manifest-path bench/Cargo.toml -- help
//! cargo run --release --manifest-path bench/Cargo.toml -- all \
//!   --before target/release/sniff-antigo --after target/release/sniff \
//!   --samples 9 --out bench/AB.md --json bench/AB.json
//! ```
//!
//! Perfis por função (`--before-profile`/`--after-profile`) são JSONL com
//! `{"function":"...","elapsed_ns":123}`. O produtor é a instrumentação da
//! biblioteca, ligada por `SNIFF_PROFILE=1` (implementada no código da
//! biblioteca; esta bancada apenas consome os arquivos). Nenhuma rede ou
//! Chrome é usado: os dois verbos são puramente offline.

#![forbid(unsafe_code)]

use std::collections::BTreeMap;
use std::fmt::Write as FmtWrite;
use std::fs;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

const DEFAULT_SAMPLES: usize = 9;
const SEED: u64 = 0xc0ff_ee12_3456_7890;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Verb {
    Diff,
    Check,
}

impl Verb {
    const fn name(self) -> &'static str {
        match self {
            Self::Diff => "diff",
            Self::Check => "check",
        }
    }
}

struct Config {
    mode: String,
    before: Option<PathBuf>,
    after: Option<PathBuf>,
    samples: usize,
    md_out: PathBuf,
    json_out: PathBuf,
    before_profile: Option<PathBuf>,
    after_profile: Option<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: String::from("all"),
            before: None,
            after: None,
            samples: DEFAULT_SAMPLES,
            md_out: PathBuf::from("bench/AB.md"),
            json_out: PathBuf::from("bench/AB.json"),
            before_profile: None,
            after_profile: None,
        }
    }
}

#[derive(Clone, Copy)]
struct Stats {
    min_ns: u64,
    median_ns: u64,
    p95_ns: u64,
}

struct VerbResult {
    verb: Verb,
    before: Stats,
    after: Stats,
    stdout_bytes: usize,
    stdout_fnv1a64: u64,
}

struct ProfileResult {
    function: String,
    before_median_ns: Option<u64>,
    after_median_ns: Option<u64>,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match parse_args(&args) {
        Ok(Some(config)) => match run(&config) {
            Ok((markdown, json)) => {
                if let Err(error) = emit(&config, &markdown, &json) {
                    eprintln!("erro ao escrever relatório: {error}");
                    return ExitCode::FAILURE;
                }
                println!("{markdown}");
                println!("--- JSON ---");
                println!("{json}");
                ExitCode::SUCCESS
            }
            Err(error) => {
                eprintln!("erro: {error}");
                ExitCode::FAILURE
            }
        },
        Ok(None) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("erro: {error}");
            ExitCode::from(2)
        }
    }
}

fn usage() -> &'static str {
    "uso: sniff-ab-bench [all|diff|check|help] [--before PATH] [--after PATH] \
     [--samples N] [--out bench/AB.md] [--json bench/AB.json] \
     [--before-profile PATH] [--after-profile PATH]"
}

/// Parseia argv; `Ok(None)` significa que o modo `help` já foi atendido.
fn parse_args(args: &[String]) -> Result<Option<Config>, String> {
    let mut config = Config::default();
    let mut index = 0;
    let mut saw_mode = false;
    while index < args.len() {
        let arg = args[index].as_str();
        if matches!(arg, "help" | "--help" | "-h") {
            print_help();
            return Ok(None);
        }
        if matches!(arg, "all" | "diff" | "check") && !saw_mode {
            config.mode = String::from(arg);
            saw_mode = true;
            index = index.saturating_add(1);
            continue;
        }
        if let Some(value) = value_option(arg, args, &mut index)? {
            apply_option(&mut config, arg, value)?;
            index = index.saturating_add(1);
            continue;
        }
        return Err(format!("argumento desconhecido: {arg}"));
    }
    if config.samples == 0 {
        return Err(String::from("--samples deve ser >= 1"));
    }
    if config.before.is_none() || config.after.is_none() {
        return Err(String::from("informe --before PATH e --after PATH"));
    }
    Ok(Some(config))
}

fn print_help() {
    println!("{}", usage());
    println!();
    println!(
        "modos: all (padrão), diff, check, help. Perfis são JSONL \
         de `{{\"function\":...,\"elapsed_ns\":...}}` produzidos pela \
         instrumentação da biblioteca com SNIFF_PROFILE=1."
    );
}

/// Reconhece opções com valor; avança `index` para o valor quando aplicável.
fn value_option<'a>(
    arg: &str,
    args: &'a [String],
    index: &mut usize,
) -> Result<Option<&'a str>, String> {
    if !matches!(
        arg,
        "--before"
            | "--after"
            | "--samples"
            | "--out"
            | "--json"
            | "--before-profile"
            | "--after-profile"
    ) {
        return Ok(None);
    }
    let Some(value) = args.get(index.saturating_add(1)) else {
        return Err(format!("{arg} sem valor"));
    };
    *index = index.saturating_add(1);
    Ok(Some(value))
}

fn apply_option(config: &mut Config, arg: &str, value: &str) -> Result<(), String> {
    match arg {
        "--before" => config.before = Some(PathBuf::from(value)),
        "--after" => config.after = Some(PathBuf::from(value)),
        "--samples" => {
            config.samples = value
                .parse()
                .map_err(|_| format!("--samples inválido: {value}"))?;
        }
        "--out" => config.md_out = PathBuf::from(value),
        "--json" => config.json_out = PathBuf::from(value),
        "--before-profile" => config.before_profile = Some(PathBuf::from(value)),
        "--after-profile" => config.after_profile = Some(PathBuf::from(value)),
        _ => return Err(format!("opção desconhecida: {arg}")),
    }
    Ok(())
}

fn run(config: &Config) -> Result<(String, String), String> {
    let before = config
        .before
        .as_ref()
        .ok_or_else(|| String::from("--before"))?;
    let after = config
        .after
        .as_ref()
        .ok_or_else(|| String::from("--after"))?;
    let check_snapshot = synthetic_check_snapshot();
    let verbs: &[Verb] = match config.mode.as_str() {
        "all" => &[Verb::Diff, Verb::Check],
        "diff" => &[Verb::Diff],
        "check" => &[Verb::Check],
        _ => return Err(format!("modo desconhecido: {}", config.mode)),
    };

    let mut results = Vec::with_capacity(verbs.len());
    for &verb in verbs {
        eprintln!("== {} ({} amostras) ==", verb.name(), config.samples);
        results.push(compare_verb(
            before,
            after,
            verb,
            &check_snapshot,
            config.samples,
        )?);
    }

    let profiles = compare_profiles(
        config.before_profile.as_deref(),
        config.after_profile.as_deref(),
    )?;
    Ok((
        render_markdown(config, &results, &profiles),
        render_json(config, &results, &profiles),
    ))
}

fn compare_verb(
    before: &Path,
    after: &Path,
    verb: Verb,
    snapshot: &str,
    samples: usize,
) -> Result<VerbResult, String> {
    let warm_before = run_once(before, verb, snapshot)?;
    let warm_after = run_once(after, verb, snapshot)?;
    assert_same_stdout(verb, &warm_before, &warm_after)?;

    let mut before_times = Vec::with_capacity(samples);
    let mut after_times = Vec::with_capacity(samples);
    let mut stdout_bytes = warm_before.stdout.len();
    let mut fingerprint = fnv1a64(&warm_before.stdout);
    for _ in 0..samples {
        let a = run_once(before, verb, snapshot)?;
        let b = run_once(after, verb, snapshot)?;
        assert_same_stdout(verb, &a, &b)?;
        stdout_bytes = a.stdout.len();
        fingerprint = fnv1a64(&a.stdout);
        before_times.push(a.elapsed_ns);
        after_times.push(b.elapsed_ns);
    }
    Ok(VerbResult {
        verb,
        before: stats_of(&before_times),
        after: stats_of(&after_times),
        stdout_bytes,
        stdout_fnv1a64: fingerprint,
    })
}

fn assert_same_stdout(verb: Verb, a: &RunResult, b: &RunResult) -> Result<(), String> {
    if a.stdout != b.stdout {
        return Err(format!(
            "stdout diverge entre antes/depois em `{}` ({} vs {} bytes); \
             A/B estaria medindo trabalho diferente",
            verb.name(),
            a.stdout.len(),
            b.stdout.len()
        ));
    }
    if a.stderr != b.stderr {
        return Err(format!(
            "stderr diverge entre antes/depois em `{}`; saída de diagnóstico \
             diferente pode indicar trabalho diferente",
            verb.name()
        ));
    }
    Ok(())
}

struct RunResult {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    elapsed_ns: u64,
}

// O relógio é intencional: latência é o objeto da bancada; a determinística
// exigida pelo contrato é a carga de trabalho e a ordem de saída, não o ns.
#[expect(
    clippy::disallowed_methods,
    reason = "Instant::now é a única fonte de tempo: medir latência é o objetivo da bancada"
)]
fn run_once(binary: &Path, verb: Verb, snapshot: &str) -> Result<RunResult, String> {
    let mut command = Command::new(binary);
    command.arg(verb.name());
    match verb {
        Verb::Diff => {
            let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let fixtures = root.join("../crates/sniff-css-diff/tests/fixtures");
            command.arg(fixtures.join("base.jsonl"));
            command.arg(fixtures.join("head.jsonl"));
        }
        Verb::Check => {
            command.arg("-");
            command.stdin(Stdio::piped());
        }
    }
    command.stdout(Stdio::piped());
    command.stderr(Stdio::piped());

    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| format!("spawn {} {}: {error}", binary.display(), verb.name()))?;
    if verb == Verb::Check
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin
            .write_all(snapshot.as_bytes())
            .map_err(|error| format!("escrever stdin do check: {error}"))?;
    }
    let output = child
        .wait_with_output()
        .map_err(|error| format!("esperar {} {}: {error}", binary.display(), verb.name()))?;
    let elapsed_ns = nanos(started.elapsed());
    if !output.status.success() {
        return Err(format!(
            "{} {} saiu com {:?}: {}",
            binary.display(),
            verb.name(),
            output.status.code(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(RunResult {
        stdout: output.stdout,
        stderr: output.stderr,
        elapsed_ns,
    })
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn stats_of(samples: &[u64]) -> Stats {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let count = sorted.len();
    let first = sorted.first().copied().unwrap_or(0);
    let last = sorted.last().copied().unwrap_or(0);
    let middle = count / 2;
    let median = if count % 2 == 1 {
        sorted.get(middle).copied().unwrap_or(first)
    } else {
        let low = sorted
            .get(middle.saturating_sub(1))
            .copied()
            .unwrap_or(first);
        let high = sorted.get(middle).copied().unwrap_or(last);
        low.saturating_add(high.saturating_sub(low) / 2)
    };
    let rank = count
        .saturating_mul(95)
        .div_ceil(100)
        .saturating_sub(1)
        .min(count.saturating_sub(1));
    Stats {
        min_ns: first,
        median_ns: median,
        p95_ns: sorted.get(rank).copied().unwrap_or(last),
    }
}

/// Snapshot sintético para `check`: nenhum fixture existente serve de carga
/// útil — `crates/sniff-engine/tests/golden/*.jsonl` termina com exit 5
/// (mistura layouts tree/flat) e `examples/example.jsonl` carrega mas não
/// dispara nenhuma regra. A semente é fixa e não há relógio, env ou rede.
fn synthetic_check_snapshot() -> String {
    let mut seed = SEED;
    let mut text = String::from(concat!(
        r#"{"__meta":{"schema":2,"viewport":{"width":1366,"height":768}}}"#,
        "\n"
    ));
    for id in 1..=24_u64 {
        let jitter = next_random(&mut seed);
        let height = 100_u64.saturating_add(jitter % 40);
        let height = if id % 7 == 0 { 60 } else { height };
        let width = 300_u64.saturating_add(jitter % 20);
        let y = height.saturating_mul(id.saturating_sub(1));
        let color = if id % 11 == 0 { "#999999" } else { "#212529" };
        let line = format!(
            concat!(
                r#"{{"id":{id},"tag":"DIV","selector":"div.card","path":"html/body/div[{id}]","#,
                r#""depth":0,"rect":{{"x":0,"y":{y},"width":{width},"height":{height}}},"#,
                r#""is_visible":true,"is_user_noticeable":{{"display_visible":true,"#,
                r#""accessibility_grade":"AAA"}},"aria":{{"focusable":false,"has_text":true}},"#,
                r#""styles":{{"box_model":{{"width":"{width}px","height":"{height}px"}},"#,
                r#""visual":{{"color":"{color}","#,
                r#""background-color":"#,
                "\"#ffffff\"",
                ",",
                r#""background-image":"none"}},"#,
                r#""typography":{{"font-size":"16px","font-weight":"400"}}}},"#,
                r#""children":[]}}"#
            ),
            id = id,
            y = y,
            width = width,
            height = height,
            color = color,
        );
        text.push_str(&line);
        text.push('\n');
    }
    for id in 25..=30_u64 {
        let line = format!(
            concat!(
                r#"{{"id":{id},"tag":"BUTTON","selector":"button.cta","#,
                r#""path":"html/body/button[{id}]","depth":0,"#,
                r#""rect":{{"x":0,"y":0,"width":32,"height":32}},"#,
                r#""is_visible":true,"is_user_noticeable":{{"display_visible":true}},"#,
                r#""aria":{{"focusable":true,"has_text":false}},"#,
                r#""styles":{{"box_model":{{"width":"32px","height":"32px"}},"#,
                r#""visual":{{"color":"#,
                "\"#212529\"",
                ",",
                r#""background-color":"#,
                "\"#ffffff\"",
                "}}}},",
                r#""children":[]}}"#
            ),
            id = id
        );
        text.push_str(&line);
        text.push('\n');
    }
    text
}

/// LCG determinístico (parâmetros de Knuth), sem `rand` nem tempo.
fn next_random(seed: &mut u64) -> u64 {
    *seed = seed
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    seed.checked_shr(33).unwrap_or(0)
}

fn compare_profiles(
    before_path: Option<&Path>,
    after_path: Option<&Path>,
) -> Result<Vec<ProfileResult>, String> {
    let before = match before_path {
        Some(path) => Some(load_profile(path)?),
        None => None,
    };
    let after = match after_path {
        Some(path) => Some(load_profile(path)?),
        None => None,
    };
    let mut names = std::collections::BTreeSet::new();
    if let Some(map) = &before {
        names.extend(map.keys().cloned());
    }
    if let Some(map) = &after {
        names.extend(map.keys().cloned());
    }
    Ok(names
        .into_iter()
        .map(|function| ProfileResult {
            before_median_ns: before
                .as_ref()
                .and_then(|map| map.get(&function))
                .map(|values| stats_of(values).median_ns),
            after_median_ns: after
                .as_ref()
                .and_then(|map| map.get(&function))
                .map(|values| stats_of(values).median_ns),
            function,
        })
        .collect())
}

fn load_profile(path: &Path) -> Result<BTreeMap<String, Vec<u64>>, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("ler perfil {}: {error}", path.display()))?;
    let mut map: BTreeMap<String, Vec<u64>> = BTreeMap::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let function = json_string_field(line, "function")
            .ok_or_else(|| format!("{}:{} sem campo \"function\"", path.display(), index + 1))?;
        let elapsed = json_u64_field(line, "elapsed_ns")
            .ok_or_else(|| format!("{}:{} sem campo \"elapsed_ns\"", path.display(), index + 1))?;
        map.entry(function).or_default().push(elapsed);
    }
    Ok(map)
}

/// Extrai um campo string de um objeto JSON de uma linha, sem dependências.
fn json_string_field(line: &str, key: &str) -> Option<String> {
    let bytes = line.as_bytes();
    let mut cursor = field_value_start(bytes, key)?;
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    cursor = cursor.saturating_add(1);
    let mut out = Vec::new();
    while let Some(byte) = bytes.get(cursor).copied() {
        cursor = cursor.saturating_add(1);
        match byte {
            b'"' => return String::from_utf8(out).ok(),
            b'\\' => {
                let escaped = *bytes.get(cursor)?;
                cursor = cursor.saturating_add(1);
                let values = decode_escape(bytes, &mut cursor, escaped)?;
                out.extend_from_slice(&values);
            }
            _ => out.push(byte),
        }
    }
    None
}

/// Extrai um campo inteiro não negativo de um objeto JSON de uma linha.
fn json_u64_field(line: &str, key: &str) -> Option<u64> {
    let bytes = line.as_bytes();
    let cursor = field_value_start(bytes, key)?;
    let rest = bytes.get(cursor..)?;
    let end = rest
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .unwrap_or(rest.len());
    let digits = rest.get(..end)?;
    if digits.is_empty() {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

/// Decodifica uma sequência escapada de JSON para bytes UTF-8.
fn decode_escape(bytes: &[u8], cursor: &mut usize, escaped: u8) -> Option<Vec<u8>> {
    let value: &[u8] = match escaped {
        b'"' => b"\"",
        b'\\' => b"\\",
        b'/' => b"/",
        b'n' => b"\n",
        b'r' => b"\r",
        b't' => b"\t",
        b'b' => b"\x08",
        b'f' => b"\x0c",
        b'u' => {
            let start = *cursor;
            let end = start.saturating_add(4);
            let raw = bytes.get(start..end)?;
            let hex = std::str::from_utf8(raw).ok()?;
            let code = u32::from_str_radix(hex, 16).ok()?;
            let character = char::from_u32(code)?;
            let mut buffer = [0_u8; 4];
            *cursor = end;
            return Some(character.encode_utf8(&mut buffer).as_bytes().to_vec());
        }
        _ => return None,
    };
    Some(value.to_vec())
}

/// Encontra o início do valor de `key` num objeto JSON simples.
fn field_value_start(bytes: &[u8], key: &str) -> Option<usize> {
    let needle = format!("\"{key}\"");
    let needle = needle.as_bytes();
    let position = bytes
        .windows(needle.len())
        .position(|window| window == needle)?;
    let mut cursor = position.saturating_add(needle.len());
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor = cursor.saturating_add(1);
    }
    if bytes.get(cursor) != Some(&b':') {
        return None;
    }
    cursor = cursor.saturating_add(1);
    while bytes.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor = cursor.saturating_add(1);
    }
    Some(cursor)
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn render_markdown(config: &Config, results: &[VerbResult], profiles: &[ProfileResult]) -> String {
    let mut out = String::from("# A/B `sniff` (verbos offline)\n\n");
    let before = config
        .before
        .as_ref()
        .map_or(String::new(), |p| display_path(p));
    let after = config
        .after
        .as_ref()
        .map_or(String::new(), |p| display_path(p));
    let _ = writeln!(out, "- antes: `{before}`");
    let _ = writeln!(out, "- depois: `{after}`");
    let _ = writeln!(out, "- amostras por verbo: {} (+1 warm-up)", config.samples);
    let _ = writeln!(
        out,
        "- p95: maior entre as amostras quando N <= 20 (rank `ceil(0.95*N)`)"
    );
    out.push_str("\n## Tempo por verbo\n\n");
    out.push_str(
        "| verbo | antes min (ms) | antes mediana (ms) | antes p95 (ms) | depois min (ms) | depois mediana (ms) | depois p95 (ms) | delta mediana |\n",
    );
    out.push_str("|---|---:|---:|---:|---:|---:|---:|---:|\n");
    for result in results {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            result.verb.name(),
            fmt_ms(result.before.min_ns),
            fmt_ms(result.before.median_ns),
            fmt_ms(result.before.p95_ns),
            fmt_ms(result.after.min_ns),
            fmt_ms(result.after.median_ns),
            fmt_ms(result.after.p95_ns),
            fmt_delta(result.before.median_ns, result.after.median_ns),
        );
    }
    out.push_str("\n## Equivalência de saída\n\n");
    out.push_str("| verbo | stdout antes == depois | bytes | FNV-1a 64 |\n");
    out.push_str("|---|---|---:|---|\n");
    for result in results {
        let _ = writeln!(
            out,
            "| {} | sim | {} | {:016x} |",
            result.verb.name(),
            result.stdout_bytes,
            result.stdout_fnv1a64,
        );
    }
    if !profiles.is_empty() {
        out.push_str("\n## Perfis por função (mediana, ns)\n\n");
        out.push_str("| função | antes | depois | delta |\n|---|---:|---:|---:|\n");
        for profile in profiles {
            let _ = writeln!(
                out,
                "| {} | {} | {} | {} |",
                markdown_cell(&profile.function),
                profile
                    .before_median_ns
                    .map_or_else(|| String::from("-"), fmt_u64),
                profile
                    .after_median_ns
                    .map_or_else(|| String::from("-"), fmt_u64),
                match (profile.before_median_ns, profile.after_median_ns) {
                    (Some(a), Some(b)) => fmt_delta(a, b),
                    _ => String::from("-"),
                },
            );
        }
        out.push_str(
            "\nPerfis produzidos pela instrumentação da biblioteca com `SNIFF_PROFILE=1`.\n",
        );
    }
    out
}

fn render_json(config: &Config, results: &[VerbResult], profiles: &[ProfileResult]) -> String {
    let mut out = String::from("{\n  \"schema\": 1,\n");
    let before = config
        .before
        .as_ref()
        .map_or(String::new(), |p| display_path(p));
    let after = config
        .after
        .as_ref()
        .map_or(String::new(), |p| display_path(p));
    let _ = writeln!(out, "  \"before\": {},", json_string(&before));
    let _ = writeln!(out, "  \"after\": {},", json_string(&after));
    let _ = writeln!(out, "  \"samples\": {},", config.samples);
    let _ = writeln!(out, "  \"warmup\": 1,");
    let _ = writeln!(out, "  \"stdout_identical\": {},", !results.is_empty());
    out.push_str("  \"verbs\": [\n");
    for (index, result) in results.iter().enumerate() {
        let _ = writeln!(out, "    {{\"verb\": {},", json_string(result.verb.name()));
        let _ = writeln!(out, "      \"before\": {},", json_stats(result.before));
        let _ = writeln!(out, "      \"after\": {},", json_stats(result.after));
        let _ = writeln!(
            out,
            "      \"stdout_bytes\": {},\n      \"stdout_fnv1a64\": \"{:016x}\"",
            result.stdout_bytes, result.stdout_fnv1a64,
        );
        out.push_str("    }");
        if index.saturating_add(1) != results.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("  ],\n  \"profiles\": {\n");
    let _ = writeln!(
        out,
        "    \"before\": {},",
        config
            .before_profile
            .as_ref()
            .map_or(String::from("null"), |p| json_string(&display_path(p)))
    );
    let _ = writeln!(
        out,
        "    \"after\": {},",
        config
            .after_profile
            .as_ref()
            .map_or(String::from("null"), |p| json_string(&display_path(p)))
    );
    out.push_str("    \"functions\": [\n");
    for (index, profile) in profiles.iter().enumerate() {
        let _ = writeln!(
            out,
            "      {{\"function\": {},\"before_median_ns\": {},\"after_median_ns\": {}}}{}",
            json_string(&profile.function),
            fmt_opt(profile.before_median_ns),
            fmt_opt(profile.after_median_ns),
            if index.saturating_add(1) == profiles.len() {
                ""
            } else {
                ","
            }
        );
    }
    out.push_str("    ]\n  }\n}\n");
    out
}

fn json_stats(stats: Stats) -> String {
    format!(
        "{{\"min_ns\":{},\"median_ns\":{},\"p95_ns\":{}}}",
        stats.min_ns, stats.median_ns, stats.p95_ns
    )
}

fn fmt_opt(value: Option<u64>) -> String {
    value.map_or_else(|| String::from("null"), fmt_u64)
}

fn fmt_u64(value: u64) -> String {
    value.to_string()
}

fn fmt_ms(ns: u64) -> String {
    let micros = ns / 1_000;
    if micros < 1_000 {
        format!("{micros} µs")
    } else {
        let whole = micros / 1_000;
        let fraction = micros % 1_000;
        format!("{whole}.{fraction:03} ms")
    }
}

fn fmt_delta(before: u64, after: u64) -> String {
    if before == 0 {
        return String::from("-");
    }
    let diff = i128::from(after) - i128::from(before);
    let percent = diff.saturating_mul(10_000) / i128::from(before);
    format!("{:+0.2}%", percent as f64 / 100.0)
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn markdown_cell(text: &str) -> String {
    text.replace('|', "\\|").replace('\n', " ")
}

fn json_string(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(2));
    out.push('"');
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if c < '\u{20}' => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn emit(config: &Config, markdown: &str, json: &str) -> Result<(), String> {
    write_report(&config.md_out, markdown)?;
    write_report(&config.json_out, json)
}

fn write_report(path: &Path, content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("criar {}: {error}", parent.display()))?;
    }
    fs::write(path, content).map_err(|error| format!("escrever {}: {error}", path.display()))
}
