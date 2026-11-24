//! Instrumentação por função, opt-in, para medição A/B (`SNIFF_PROFILE`).
//!
//! O subscriber registra a duração de cada `tracing::span` fechado num arquivo
//! JSONL (uma linha por span): `{"function":"…","elapsed_ns":N}`. O caminho
//! vem de `SNIFF_PROFILE` — quando a variável não está definida, nenhum layer
//! é instalado e o custo do caminho normal é zero.
//!
//! Isto não altera stdout (contrato: stdout = dados); o arquivo é escrito
//! separadamente e a falha de escrita é silenciosa (perfil é diagnóstico).

use std::fmt::Write as _;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::sync::Mutex;
use std::time::Instant;

use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing::span;
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// Campos associados a um span de função, coletados na criação.
#[derive(Default)]
struct SpanFields {
    function: String,
}

impl Visit for SpanFields {
    fn record_str(&mut self, field: &Field, value: &str) {
        if field.name() == "function" {
            self.function = value.to_string();
        }
    }

    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        if field.name() == "function" {
            let _write = write!(self.function, "{value:?}");
        }
    }
}

struct Timing {
    name: String,
    started: Instant,
}

/// Layer que escreve a duração de cada span em JSONL.
pub(crate) struct ProfileLayer {
    output: Mutex<File>,
}

impl ProfileLayer {
    /// Constrói o layer a partir do caminho dado, ou `None` se não puder abrir.
    pub(crate) fn at(path: &std::path::Path) -> Option<Self> {
        let output = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(path)
            .ok()?;
        Some(Self {
            output: Mutex::new(output),
        })
    }

    fn write_row(&self, function: &str, elapsed_ns: u64) {
        let Ok(mut file) = self.output.lock() else {
            return;
        };
        let _write = writeln!(
            file,
            "{{\"function\":\"{function}\",\"elapsed_ns\":{elapsed_ns}}}"
        );
        let _flush = file.flush();
    }
}

impl<S> Layer<S> for ProfileLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let mut fields = SpanFields::default();
        attrs.record(&mut fields);
        let name = if fields.function.is_empty() {
            attrs.metadata().name().to_string()
        } else {
            fields.function
        };
        if let Some(span_ref) = ctx.span(id) {
            #[expect(
                clippy::disallowed_methods,
                reason = "perfil de performance: medir elapsed() é a finalidade do layer, não estado de domínio"
            )]
            let started = Instant::now();
            span_ref.extensions_mut().insert(Timing { name, started });
        }
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        let Some(span_ref) = ctx.span(&id) else {
            return;
        };
        let Some(timing) = span_ref.extensions_mut().remove::<Timing>() else {
            return;
        };
        let elapsed_ns = u64::try_from(timing.started.elapsed().as_nanos()).unwrap_or(u64::MAX);
        self.write_row(&timing.name, elapsed_ns);
    }
}
