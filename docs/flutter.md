# sniffCSS — Backend Flutter/Dart

Guia completo de configuração e uso do backend **Flutter** da `sniffCSS` em
Linux. Ele captura a **árvore de widgets** de um app Flutter/Dart nativo em
emulador/device Android sobre o **Dart VM Service** (o análogo do sniff web,
que usa Chromium/CDP), e emite o **mesmo modelo JSONL** — então os mesmos
`sniffCSS-diff` / `sniffCSS-check` funcionam por path.

A referência completa de flags fica em [`docs/usage.md`](usage.md#backend-flutterdart---backend-flutter);
este guia foca em instalação, o modo padrão `auto` e o resultado.

---

## 1. Visão geral

| | Web (`--backend web`) | Flutter (`--backend flutter`) |
|---|---|---|
| Alvo | Página em Chromium via CDP | App **debug** num emulador/device Android |
| Transporte | Chrome DevTools Protocol | Dart VM Service (`ext.flutter.inspector.*`) |
| Identidade do nó | `tag`/`selector` CSS | Classe do widget (`Text`, `ElevatedButton`) |
| Estilos | Computed CSS (~400 props) | Diagnostics do widget (`getProperties`) |
| Cores | `#rrggbb` | `#rrggbb` (Flutter normalizado) |
| Contraste | WCAG derivado | WCAG derivado (mesmo código) |
| Diff/check | `sniffCSS-diff` / `sniffCSS-check` | idênticos, por path |

**Requisito fundamental:** o app precisa estar em build **debug** (ou profile).
Build `release` não expõe o VM Service e a captura falha.

---

## 2. Instalação no Linux (Ubuntu / Fedora / Arch)

### 2.1 Dependências do sistema

O mínimo para build/run Android é `git`, `curl`, `unzip` e um **JDK 17**
(os templates Android compilam para Java 17). As dependências de **Linux
desktop** (`clang/cmake/ninja/GTK`) só são necessárias se você quiser rodar o
app como alvo desktop (`flutter run -d linux`) — o backend Flutter usa Android.

**Ubuntu / Debian**
```bash
sudo apt update
sudo apt install -y git curl unzip xz-utils openjdk-17-jdk
# opcional — alvo Linux desktop:
sudo apt install -y clang cmake ninja-build pkg-config libgtk-3-dev
```

**Fedora**
```bash
sudo dnf install -y git curl unzip java-17-openjdk-devel
# opcional — alvo Linux desktop:
sudo dnf install -y clang cmake ninja-build pkgconf-pkg-config gtk3-devel
```

**Arch**
```bash
sudo pacman -S --needed git curl unzip jdk17-openjdk
# opcional — alvo Linux desktop:
sudo pacman -S --needed clang cmake ninja pkgconf gtk3
```

### 2.2 SDK Flutter

Instale no canal `stable` num diretório de sua posse (ex.: `~/flutter`):

```bash
git clone -b stable --depth 1 https://github.com/flutter/flutter.git ~/flutter
```

> Se seu `~/.gitconfig` global reescreve URLs do GitHub com credencial embutida
> (`url.https://user:TOKEN@github.com/.insteadof=...`), o remote do clone fica
> com o token gravado em `~/flutter/.git/config`. Após o clone, corrija com
> `git -C ~/flutter remote set-url origin https://github.com/flutter/flutter.git`
> e, de preferência, revogue o token exposto.

Adicione ao `PATH` (bash/zsh — coloque no `~/.bashrc`/`~/.zshrc`):

```bash
export PATH="$HOME/flutter/bin:$PATH"
```

### 2.3 Android SDK + emulador

O Flutter não instala o Android SDK sozinho. Baixe as **command-line tools** e
instale os pacotes via `sdkmanager`:

```bash
mkdir -p ~/Android/Sdk/cmdline-tools
curl -L -o /tmp/clt.zip \
  https://dl.google.com/android/repository/commandlinetools-linux-9477386_latest.zip
unzip -q /tmp/clt.zip -d ~/Android/Sdk/cmdline-tools
mv ~/Android/Sdk/cmdline-tools/cmdline-tools ~/Android/Sdk/cmdline-tools/latest

export ANDROID_HOME="$HOME/Android/Sdk"
export ANDROID_SDK_ROOT="$ANDROID_HOME"
export PATH="$ANDROID_HOME/cmdline-tools/latest/bin:$ANDROID_HOME/platform-tools:$ANDROID_HOME/emulator:$PATH"

yes | sdkmanager --licenses
sdkmanager "platform-tools" "emulator" \
  "system-images;android-33;google_apis;x86_64"
```

Crie um AVD:

```bash
avdmanager create avd -n sniff_test \
  -k "system-images;android-33;google_apis;x86_64" -d pixel_5
```

> **`google_apis`, não `google_apis_playstore`:** a imagem com Play Store é
> build de produção — `adb root` é bloqueado e, em alguns kernels, o Dart VM
> Service não consegue abrir socket (`EPERM` no bind de `127.0.0.1`). A imagem
> `google_apis` é userdebug: `adb root` + `setenforce 0` funcionam e o VM
> Service sobe normalmente. Se só houver a imagem playstore, acessível via
> `adb root` não será possível — prefira a `google_apis`.

### 2.4 Verificação

```bash
flutter doctor -v
```

Deve mostrar o Android toolchain ✓ (SDK em `~/Android/Sdk`, licenças aceitas,
emulador disponível). A ausência de Chrome/Linux toolchain **não** afeta o
backend Flutter.

Primeiro build do app (baixa Gradle/AGP/Kotlin — demora):

```bash
cd <seu-app>
flutter pub get
flutter build apk --debug
```

---

## 3. Configuração no app alvo

Para que a `sniffCSS` capture um app, ele precisa estar **rodando em debug**
com o VM Service exposto — e o app precisa conseguir **abrir sockets** no
device. Isso exige a permissão `INTERNET` no build debug:

`android/app/src/debug/AndroidManifest.xml`
```xml
<manifest xmlns:android="http://schemas.android.com/apk/res/android">
    <uses-permission android:name="android.permission.INTERNET"/>
</manifest>
```

> Sem esse arquivo (é o padrão dos templates Flutter, que o geram), o app debug
> não consegue criar sockets → o Dart VM Service nunca sobe e a captura estoura
> em *timeout* com `SocketException: ... Operation not permitted (errno = 1)`.

**Dica para o `contrast`:** o `Scaffold.backgroundColor` **não** é exposto nos
diagnostics do Flutter 3.40+. Para o contraste derivar em captura real, pinte
o fundo com um widget de superfície que expõe `color`:

```dart
Scaffold(
  body: ColoredBox(
    color: const Color(0xFF2563EB),   // → vira background-color #2563eb
    child: Center(child: ...),
  ),
)
```

Widgets com esse mapeamento: `ColoredBox`, `Container`, `Material`, `Card`,
`DecoratedBox`, `Ink`.

---

## 4. Como usar — modo padrão `auto`

O `--backend` padrão agora é **`auto`**: o próprio `--url` decide o backend e o
device. Um URL `flutter://<serial>` significa backend Flutter no device
`<serial>`; qualquer outro URL significa web.

### 4.1 Subir o emulador

```bash
emulator -avd sniff_test -no-audio -no-boot-anim -gpu host -no-snapshot &
adb wait-for-device
adb root && adb shell setenforce 0   # imagem google_apis (ver §2.3)
```

> **Sem `-no-window`** (emulador **com janela**) é o necessário para **actions**:
> o Flutter Driver só completa os comandos quando o app produz frames, e num
> emulador headless o vsync morre quando nada é apresentado — o `tap` estoura em
> *timeout*. Capturas **sem** ações funcionam no headless; ações exigem janela
> (num host sem display, use `xvfb-run` ou um device físico).
>
> `-gpu host`: em alguns hosts o renderizador de software (`swiftshader`)
> segfaulta na inicialização do emulador; `-gpu host` usa o GPU da máquina.

### 4.2 Capturar (run)

```bash
# Forma enxuta: backend e device vêm do URL; -s vira "root" se omitido.
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app --depth 10

# Forma explícita equivalente:
sniffCSS --backend flutter --device emulator-5554 \
  --project ~/projetos/app --target lib/main.dart --depth 10 -s root
```

O fluxo `run` faz: `flutter run --machine` → descobre o VM Service →
`flutter build`/instala o app → congela animações (`timeDilation`) → extrai a
árvore → imprime.

### 4.3 Anexar a um app já rodando (attach)

Com o app debug já em execução (ex.: via `flutter run` ou hot-reload), sem
recompilar:

```bash
sniffCSS -u flutter://emulator-5554 --attach --project ~/projetos/app --depth 10
```

### 4.4 Lançar AVD + rodar o app

```bash
sniffCSS -u flutter://pixel --avd pixel --project ~/projetos/app --depth 10
```

> Neste host, prefira `--device` com o emulador subido por `-gpu host` (§4.1):
> o `--avd` interno usa `swiftshader_indirect`, que crasha aqui.

### 4.5 Screenshot junto com a captura

```bash
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app --screenshot out.png
```

### 4.6 Controlar o viewport do app

O `--viewport` no backend Flutter aplica `adb shell wm size WxH` no device
**antes** de lançar o app (o Flutter relê o `MediaQuery` e relayouta, então
media queries e a geometria capturada mudam) e **restaura** o tamanho anterior
ao final — inclusive em caminhos de erro:

```bash
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app \
  --viewport 540x1200 --depth 10
```

> `540x1200` (px) na densidade 440 do AVD gera um `rect` do `MaterialApp` de
> ~196x412 dp (540/2.75). Para densidade diferente, combine com um perfil de
> AVD próprio (`hw.lcd.density`).

---

## 5. Flags opcionais do cenário Flutter

Tabela resumida (referência completa: [`docs/usage.md`](usage.md)):

| Flag | Descrição | Padrão |
|---|---|---|
| `-u, --url` | **Obrigatório.** `flutter://<serial>` (ou `flutter://<serial>/<path>`) | — |
| `-s, --selector` | Identidade do nó-raiz; `root` se omitido no backend Flutter | `root` |
| `--depth N` | Níveis de widget capturados abaixo da raiz (0 = só raiz) | `0` |
| `--project DIR` | Dir do app (com `pubspec.yaml`) | pai de `--target` |
| `--target ENTRY` | Entry do app | `lib/main.dart` |
| `--attach` | Anexar (`flutter attach`) em vez de `flutter run` | `off` |
| `--avd NAME` | Lançar este AVD | — |
| `--device SERIAL` | Serial `adb`; vence o host do URL | do URL |
| `--backend web\|flutter\|auto` | Forçar backend (vence a inferência) | `auto` |
| `--output summary\|jsonl\|jsonl-flat\|json` | Formato de saída | `summary` |
| `--no-summary` | Atalho para `--output jsonl` | — |
| `--persist` | Grava em `sniffCSS/[domain]/[UTC]-…` (mesma árvore do MCP) | `off` |
| `--screenshot PATH` | PNG do device (`adb exec-out screencap -p`) | — |
| `--viewport WxH` | Tamanho lógico do app (`adb shell wm size WxH`); restaurado ao final | device |
| `--no-visible` | Incluir widgets sem render box / fora do layout | `on` |
| `--min-width`, `--min-height` | Filtrar por tamanho do `rect` | — |
| `--exclude SEL` | Pular widgets pelo `selector` (repetível) | — |
| `--no-rect`, `--no-path` | Omitir `rect`/`path` | — |
| `--click SEL[:t[:settle]]` | Toca o widget em `SEL` antes de capturar (repetível) | — |
| `--type SEL:text` | Toca `SEL` (foca o campo) e insere `text` (repetível) | — |
| `--action spec` | Forma **ordenada** p/ fluxos mistos (repetível); `hover`/`upload` são **web-only** e falham com erro claro | — |

Flags **exclusivas do backend web** (ignoradas no modo Flutter): `--categories`,
`--props`, `--pseudo`, `--wait`, `--hover`, `--upload`, `--header`,
`--storage-state`, `--connect`, `--chrome`, `--fullpage-screenshot`.

### 5.1 Ações de interação (tap / type)

O `--click`/`--type`/`--action` do backend Flutter dirige o app pela extensão
**Flutter Driver** (`ext.flutter.driver`), localizando o widget alvo **dentro
do app** por `ValueKey` (extraído de `selector` como `<'counter'>`), por tipo
de widget ou por texto — não usa retângulos da captura (os `rect` de filhos de
`Flex` têm offset impreciso). Cada ação espera o próprio alvo, toca o centro,
e a captura roda **depois**, sobre o estado pós-interação (ex.: o
`AlertDialog` que o clique abriu):

```bash
# toca o FilledButton de key `counter` (o snapshot passa a mostrar "Counter: 1")
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app \
  --attach --depth 15 "--action=click:FilledButton-[<'counter'>][0]"

# abre o modal antes de capturar:
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app \
  --attach --depth 15 "--action=click:OutlinedButton-[<'modal'>][0]"

# foca o TextField e digita (após o tap, o campo recebe o texto):
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app \
  --attach --depth 15 "--action=type:TextField-[<'field'>][0]:hello"
```

> **Requisito:** o `main()` do app precisa chamar
> `enableFlutterDriverExtension()` (de `package:flutter_driver/driver_extension.dart`)
> e o app precisa ter `flutter_driver` em `dev_dependencies` — exatamente como o
> fixture em `crates/sniff-flutter/fixtures/app`. Sem isso, a ação falha com uma
> mensagem clara em vez de ser ignorada. `hover`/`upload` são web-only (não há
> cursor ou file input numa superfície Flutter mobile) e retornam erro.
>
> As animações são congeladas **depois** das ações (a transição do modal
> completa em tempo real antes de congelar) e o `timeDilation` é restaurado ao
> final da captura — o app nunca fica travado para a próxima execução.

---

## 6. Testar de fato num app real (checklist)

Roteiro passo a passo para alguém que vai rodar a `sniffCSS` contra **o próprio
app Flutter** pela primeira vez — do setup ao diff. Vale para emulador ou
device físico.

### 6.1 Preparar o app alvo (3 mudanças no mínimo)

```bash
cd ~/projetos/meu-app
```

1. **`pubspec.yaml`** — adicione `flutter_driver` em `dev_dependencies`:

   ```yaml
   dev_dependencies:
     flutter_test:
       sdk: flutter
     flutter_driver:
       sdk: flutter
   ```

2. **`lib/main.dart`** — chame `enableFlutterDriverExtension()` **antes** do
   `runApp` (importa `package:flutter_driver/driver_extension.dart`):

   ```dart
   import 'package:flutter/material.dart';
   import 'package:flutter_driver/driver_extension.dart';

   void main() {
     enableFlutterDriverExtension();
     runApp(const MeuApp());
   }
   ```

   > Necessário **apenas se você for usar `--action`/`--click`/`--type`**. Sem
   > ele, capturas normais funcionam e a ação falha com mensagem clara (não
   > silenciosamente).

3. **`android/app/src/debug/AndroidManifest.xml`** — permissão `INTERNET`
   (ver §3). Os templates Flutter já geram esse arquivo.

Opcional (para o `contrast` derivar): pinte o fundo com um widget de superfície
(`ColoredBox`/`Container`/`Material`), não `Scaffold.backgroundColor` (ver §3).

Depois:

```bash
flutter pub get
flutter build apk --debug   # primeiro build baixa Gradle/AGP — demora
```

### 6.2 Subir o emulador (com janela) e rodar o app

```bash
emulator -avd sniff_test -no-audio -no-boot-anim -gpu host -no-snapshot &
adb wait-for-device
adb root && adb shell setenforce 0

# no diretório do app:
flutter run --machine -d emulator-5554   # mantém aberto (a sniffCSS anexa nele)
```

> Sem a janela (`-no-window`), actions estouram em *timeout* (vsync morto) —
> ver §4.1. Se o app for rodar com `flutter run --machine` próprio da sniffCSS
> (`--backend flutter` sem `--attach`), pode pular o `flutter run` manual.

### 6.3 Smoke test — captura sem ações

```bash
sniffCSS -u flutter://emulator-5554 --attach --project ~/projetos/meu-app \
  --depth 10 --output jsonl-flat > base.jsonl
```

Esperado: uma linha JSON por widget (`tag`, `selector`, `rect`, `styles`,
`contrast`). Veja o conteúdo de um nó de texto para conferir que os diagnostics
chegam:

```bash
rg '"tag":"Text"' base.jsonl | head -1
```

Sinais de problema:

| Sintoma | Causa | Solução |
|---|---|---|
| `timed out waiting for the VM Service` | App não em debug / `--project` errado | `flutter run --machine` manual; build debug |
| `SocketException ... errno = 1` no log do app | Sem `INTERNET` no debug (ou imagem playstore) | Ver §3 |
| Só o nó raiz (`MaterialApp`) | Flutter < 3.40 | Use Flutter ≥ 3.40 |
| `contrast` todo `unknown` | Fundo não exposto nos diagnostics | Use `ColoredBox`/`Container` (ver §3) |

### 6.4 Testar ações (se o app tiver elementos interativos)

1. **Capture antes** e guarde o texto de um botão com `ValueKey`, por exemplo:

   ```bash
   # snapshot do contador antes
   sniffCSS -u flutter://emulator-5554 --attach --project ~/projetos/meu-app \
     --depth 10 --output jsonl-flat | rg -o '"data":"Counter: [0-9]+"' | head -1
   ```

2. **Rode a ação** e confira que o estado mudou:

   ```bash
   # ATENÇÃO às aspas: o selector carrega apóstrofos — aspas simples do shell
   # quebram (vira ByText, o alvo não é achado e dá timeout). Use aspas duplas.
   sniffCSS -u flutter://emulator-5554 --attach --project ~/projetos/meu-app \
     --depth 10 --output jsonl-flat \
     "--action=click:FilledButton-[<'counter'>][0]" \
     | rg -o '"data":"Counter: [0-9]+"' | head -1
   ```

3. **O fluxo completo com ações + diff** — capture o estado pós-interação como
   base, mude o app, capture o head e compare:

   ```bash
   sniffCSS -u flutter://emulator-5554 --attach --project ~/projetos/meu-app \
     --depth 10 --output jsonl-flat \
     "--action=click:OutlinedButton-[<'modal'>][0]" > base.jsonl
   # ... edite o app (hot-restart 'R' no flutter run), re-capture:
   sniffCSS -u flutter://emulator-5554 --attach --project ~/projetos/meu-app \
     --depth 10 --output jsonl-flat \
     "--action=click:OutlinedButton-[<'modal'>][0]" > head.jsonl
   sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5
   sniffCSS-check --input base.jsonl --rules
   ```

   > O `selector`/`path` dos nós (ex. `AlertDialog[0]`) é o identificador estável
   > do diff. Para ações, dê `ValueKey` aos widgets-alvo — o finder do driver
   > prefere a key e o diff fica robusto a mudanças de layout.

### 6.5 Sinais de sucesso / erros comuns de ações

| Sintoma | Causa provável | Solução |
|---|---|---|
| `flutter actions need the app to call enableFlutterDriverExtension()` | Falta a chamada no `main()` / `flutter_driver` ausente | Ver §6.1 |
| `flutter driver tap: Timeout` e o alvo **existe** | App congelado de uma captura anterior, ou emulador headless (`-no-window`) | Rode uma captura nova (restaura o `timeDilation` sozinho) e use emulador **com janela** |
| `flutter driver tap: Timeout` e o alvo **não existe** / selector quebrado | `ByText` no lugar de `ByValueKey` por causa de aspas ou de `<'...'>` errado | Use aspas duplas no shell; confira o selector no `base.jsonl` |
| Ação não surte efeito (sem erro) | Alvo coberto por um modal/barreira ainda aberto | Feche o modal (ou capture com o estado desejado já aberto) |
| Texto não aparece no snapshot do `TextField` | O `TextEditingController` não é exposto nos diagnostics do widget | Confirme o texto no app (o `--type` insere de verdade) |
| Emulador some do `adb devices` com o qemu vivo | Daemon `adb` dessincronizado | `adb kill-server && adb start-server` |

---

## 7. O resultado

### 7.1 Formato padrão (`--output summary`)

Uma linha JSON por nó (digest: `id`, `parent_id`, `tag`, `selector`/`path`,
`depth`, `rect`, `contrast` e o subconjunto `css`). Exemplo real do fixture
`sniff_flutter_fixture` (texto branco 24px/700 sobre `#2563eb`):

```json
{"id":5,"parent_id":4,"tag":"ColoredBox","selector":"ColoredBox[0]",
 "path":"ColoredBox[0]","depth":4,
 "rect":{"x":0.0,"y":0.0,"width":392.7,"height":826.9},
 "contrast":{"ratio":0.0,"aa":"unknown","aaa":"unknown"},
 "css":{"background-color":"#2563eb"}}
{"id":8,"parent_id":7,"tag":"Text-[<'greeting'>]","selector":"Text-[<'greeting'>][0]",
 "path":"Text-[<'greeting'>][0]","depth":7,
 "rect":{"x":123.6,"y":366.5,"width":145.4,"height":34.0},
 "contrast":{"ratio":5.17,"aa":"pass","aaa":"pass"},
 "css":{"color":"#ffffff","font-size":"24.0","font-weight":"700"}}
```

Os campos importantes:
- `tag` — classe do widget (`Text`, `ElevatedButton`, `ColoredBox`, …);
- `selector`/`path` — breadcrumb estável (`Column > Text[0]`) usado pelo diff;
- `rect` — tamanho do render object + offset acumulado do `parentData`;
- `styles` (no `jsonl` completo) — agrupadas em `layout`/`typography`/`visual`/
  `box-model`/`accessibility`/`custom`, com cores normalizadas para `#rrggbb`;
- `contrast` — facet WCAG derivado (`ratio`, `large`, `aa`/`aaa`, ou
  `unknown_reason` quando não há fundo resolvível);
- `accessibility.enabled` — ex.: `false` para um `ElevatedButton` desabilitado.

### 7.2 Formatos completos

- `--output jsonl` — uma linha por raiz com `children` aninhados (snapshot
  completo, inclui `styles`, `computed_style_hash`).
- `--output jsonl-flat` — uma linha **por nó** em pré-ordem com `parent_id`
  (o mais direto para alimentar `sniffCSS-diff`/`sniffCSS-check`).
- `--output json` — um único documento JSON.

### 7.3 Diff e checks (mesmo pipeline do web)

```bash
# Captura base e head em jsonl-flat, depois compara:
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app --depth 10 \
  --output jsonl-flat > base.jsonl
sniffCSS -u flutter://emulator-5554 --project ~/projetos/app --depth 10 \
  --output jsonl-flat > head.jsonl
sniffCSS-diff base.jsonl head.jsonl --tolerance 0.5
sniffCSS-check --input base.jsonl --rules
```

Os selectors estáveis (`ClassName[ordinal]`) fazem o diff casar nós entre
capturas; apenas mudanças reais aparecem como `CHANGED`/`ADDED`/`REMOVED`
(IDs de instância Dart — ex. `_MaterialAppState#ab12` — aparecem como ruído no
`jsonl` completo; use o `summary` para ver só o essencial).

---

## 8. Troubleshooting

| Sintoma | Causa provável | Solução |
|---|---|---|
| `timed out waiting for the VM Service` | App não está em debug, ou o `--project` não aponta para o app | `flutter run`/`--attach` manual para confirmar; use build debug |
| `SocketException: ... errno = 1` no log do app | Sem permissão `INTERNET` no build debug (ou imagem playstore sem `adb root`) | Adicione `src/debug/AndroidManifest.xml` (ver §3); use imagem `google_apis` + `setenforce 0` |
| Emulador morre no boot (core dump) | SwiftShader crashando no host | Inicie com `-gpu host` e use `--device` |
| App não aparece em `flutter devices` | Servidor `adb` dessincronizado | `adb kill-server && adb start-server` |
| Flutter muito antigo (evento `app.debugService` ausente) | Flutter < 3.40 | Use Flutter ≥ 3.40 (que emite `app.debugPort`) |
| `rect` zero / widgets sem geometria | Widget sem render box | Filtrar com `--no-visible` se necessário |

---

## 9. Referências

- [`docs/usage.md`](usage.md#backend-flutterdart---backend-flutter) — flags e
  exemplos (fonte única de verdade para a CLI).
- [`docs/architecture.md`](architecture.md#backend-flutter-sniff-flutter) —
  desenho interno do backend Flutter (máquina, VM Service, envelope `_extensionType`).
- [`docs/diff-checks.md`](diff-checks.md) — `sniffCSS-diff` e `sniffCSS-check`.
- [`docs/accessibility.md`](accessibility.md) — auditoria de acessibilidade.
