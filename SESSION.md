# SESSION.md

Sessão interrompida em 2026-08-14. Contexto completo em `conversations/` (se presente).

## Objetivo desta sessão

1. Adicionar `--viewport` ao tool MCP **`sniffFlutter_page`** (o `sniffCSS_page` web **já tem** — confirmado em `crates/sniff-css-mcp/src/server.rs:209-211,1044`).
2. Fazer **actions** funcionarem no backend Flutter (hoje são web-only e ignoradas silenciosamente).

## O que JÁ está pronto (validado)

- **Fixture interativa** (`crates/sniff-flutter/fixtures/app/lib/main.dart`): `FilledButton` contador (key `counter`), `OutlinedButton` que abre `AlertDialog` (key `modal`), `TextField` (key `field`), além do que já existia. `pubspec.yaml` ganhou `flutter_driver` (dev). `widget_test.dart` cobre os novos elementos (2 testes, `flutter test` → 2 passed).
- **Protocolo Flutter Driver validado empiricamente**: `ext.flutter.driver` responde no RPC direto (`{"isError":false,"response":...}`), com params `command`/`finderType`/`keyValueString`/`keyValueType`. `tap ByValueKey counter` incrementou o contador e `waitFor ByText "Counter: 1"` confirmou (via probe descartável). Sem `--timeout` no comando, funciona.
- **`sniff_core::config::Action`**: adicionados métodos `kind()`, `selector()`, `timeout_ms()`, `settle_ms()`.
- **`crates/sniff-flutter/src/driver.rs`** (novo): `FlutterDriver` (connect/tap/enter_text/wait_for/is_available), `DriverFinder` (ByValueKey/ByType/ByText) com `finder_from_spec()` (extrai `<'key'>` → ValueKey; classe → ByType; resto → ByText) + testes unitários. Compila.
- **`crates/sniff-flutter/src/action.rs`** (novo): `perform()` (Click=tap; Type=tap+enter_text; Hover/Upload → erro claro "web-only"), `unsupported()`, `target_finder()` + testes. Compila.
- **`lib.rs`**: exporta `action`, `driver`, `perform_action`, `unsupported`, `target_finder`, `DriverFinder`, `FlutterDriver`, `finder_from_spec`.
- **CLI `run_flutter`** (`crates/sniff-css/src/main.rs`): se `config.actions` não vazio → conecta `FlutterDriver`, checa `is_available()` (senão `bail!` explicando `enableFlutterDriverExtension()`), executa cada action + `sleep(settle_ms)`, e **só então** `freeze_animations()` + extract (congelar antes congelaria a transição da interação). Sem actions → fluxo antigo (freeze antes de extract). Compila.

## BLOQUEIO atual (investigar primeiro)

**O `tap` do driver trava após hot-restart.** Sequência:
1. Probe standalone direto contra o app `flutter run` fresco: `get_text`/`tap`/`waitFor` funcionam.
2. Depois que um `flutter attach --machine` faz hot-restart, o `ext.flutter.driver` tap **nunca responde** (probe deu timeout 20s; CLI `--action` travou).
3. Hipótese mais provável: `LiveWidgetController.pump` faz `binding.scheduleFrame(); await binding.endOfFrame;` (`flutter_test/lib/src/controller.dart:2503-2509`) e `endOfFrame` não completa quando o app não está produzindo frames (estado pós-hot-restart / emulador headless). Verificar:
   - `is_available()` via `getIsolate.extensionRPCs` retorna true (confirmado no log: `ext.flutter.driver` presente) — então não é falta da extensão.
   - Reproduzir numa **launch limpa** (kill do app + `flutter run --machine` novo via `setsid nohup … &`, como antes) e testar `tap` imediatamente; se funcionar, o problema é o hot-restart do attach.
   - Alternativas se o tap continuar travando: (a) verificar se `timeout` no comando ajuda (o driver tem `command.timeout`); (b) investigar se frames param; (c) considerar enviar tap e **não** esperar `endOfFrame` (mas o protocolo é RPC síncrono — o `result` só volta quando o handler termina, então sem resposta = handler preso em `endOfFrame`).
   - **Nota de ambiente**: emulador `emulator-5554` segue vivo; app atual pid 6118 VM service em `46837` (token `j6V9DbAezRs=`), mas forwards do adb acumulam (muitas linhas em `adb forward --list`); conexões antigas (ex.: porta 45845) são de processos mortos — sempre confirmar o port/token atuais antes de testar.

## O que FALTA fazer

1. **[BLOQUEIO]** Resolver o hang do driver `tap` pós-hot-restart (ver acima) — necessário para o CLI `--action` funcionar.
2. **Validar CLI end-to-end**: `sniffCSS -u flutter://emulator-5554 --project crates/sniff-flutter/fixtures/app --attach --depth 15 '--action=click:FilledButton-[<'counter'>][0]'` deve capturar o tree com `Counter: 1` (usar `--attach` com app já rodando OU `--avd`/device com app novo). Rebuild release: `cargo build -p sniff-css --release`.
3. **MCP `sniffFlutter_page`** (`crates/sniff-css-mcp/src/server.rs`):
   - Adicionar ao `SniffFlutterRequest` (linha ~301): `viewport: String` (default `""` = não alterar device; parsear com `Viewport::parse_cli` quando não vazio) e `actions: Vec<ActionInput>` (reusar `action_from_input`, linha ~1124).
   - No handler `sniff_flutter_page` (linha ~598): aplicar `sniff_flutter::ViewportGuard::apply(&device, w, h)` antes do launch e restaurar no final (manter guard vivo até após screenshot), e rodar actions via `sniff_flutter::FlutterDriver` + `perform_action` antes do `freeze_animations`/`extract` (mesma ordem do CLI). Atualizar o `Default impl` (linha ~327).
4. **Limpeza**: remover `crates/sniff-cdp/src/bin/probe_driver.rs` (untracked, artefato de teste) e `crates/sniff-cdp/src/bin/` inteiro se vazio.
5. **Docs**: em `docs/flutter.md` — `--action`/`--click`/`--type` agora suportados (requer `enableFlutterDriverExtension()` no main + `flutter_driver` em dev_dependencies; hover/upload = web-only), novo exemplo; em `docs/usage.md` — remover `--action` da lista "ignoradas no modo Flutter" e documentar o requisito; `CHANGELOG.md` — entrada Unreleased (actions no Flutter; viewport no sniffFlutter_page MCP).
6. **Gates finais**: `cargo fmt --all`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace` (antes eram 285 passed); `flutter test` no fixture (2 passed).

## Notas / armadilhas

- **Não commitar** artefatos: `crates/sniff-cdp/src/bin/`, `conversations/` (untracked legado), `sniffCSS/`.
- A flag `--viewport` do CLI flutter já está implementada e validada (MaterialApp 392.7→196.4 dp com `540x1200`, device restaurado) — essa parte do CLI **não** é o foco pendente; só o MCP.
- Os rects do extractor de flutter têm offset quebrado para filhos de flex (Column/Row reportam offset 0 — limitação do `getLayoutExplorerNode`, que só serializa `BoxParentData`, não `FlexParentData`). NÃO usar rect para coordenadas de action (por isso o driver finder é a abordagem correta). Bug de rect pré-existente, fora do escopo.
- Toolchain: PATH em `~/.zshrc` (`~/flutter/bin`, `~/Android/Sdk/platform-tools`); AVD `sniff_test`; emulador 37.1.11 com `-gpu host`; `adb root` + `setenforce 0`.
