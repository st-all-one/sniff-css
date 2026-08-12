# Auditoria de acessibilidade com `sniffCSS`

Workflow validado contra páginas reais (portais de câmara e governo). O contraste
é **medido** (fundo efetivo resolvido in-page), a perceptibilidade é **graduada**
(`is_user_noticeable`) e as regras `sniffCSS-check` são **determinísticas** — a IA
só interpreta evidências, não chuta.

## Facetas de acessibilidade

| Flag | Facet | O que mede |
|---|---|---|
| *(sempre)* | `aria` | Role (explícita ou implícita pelo tag), accessible name, `focusable`, `has_text`, `aria-hidden`, `disabled`, `lang` — calculados na página. |
| *(sempre)* | `is_user_noticeable` | `display_visible` (está renderizado) + `accessibility_grade` (`NONE`/`AA`/`AAA`). |
| `--contrast` | `contrast` | Ratio WCAG + AA/AAA, **com o fundo efetivo composto in-page** (transparentes subindo até o canvas; `unknown` só para fundo-imagem). |
| `--ax` | `ax` | Nó da árvore de acessibilidade do Chrome (`role`/`name`/`ignored`/`focusable`/`level`) — a verdade do browser. |
| `--ax-tree` | `__ax_tree` | Subárvore AX completa dos elementos casados (implica `--ax`). |

## Entendendo o `is_user_noticeable`

Divide o antigo `is_visible` em dois eixos ortogonais:

```json
"is_user_noticeable": {"display_visible": true, "accessibility_grade": "AA"}
```

- **`display_visible`** — o elemento está **renderizado** (`display`≠`none`,
  `visibility`≠`hidden`, `opacity`>0, tamanho≠0). **Não** depende do viewport:
  rodapé, conteúdo abaixo da dobra e skip-links continuam `true`.
- **`accessibility_grade`** —
  - `NONE` — não exposto à tecnologia assistiva: `aria-hidden="true"`,
    `hidden`/`inert`, `display:none`, `visibility:hidden`, tamanho zero.
  - `AA` — exposto à AT, porém **deficiente**: fora da tela (abaixo da dobra),
    `opacity:0`, ou **sem nome acessível** quando o role exige nome
    (link, button, img, heading, checkbox, textbox, navigation, ...).
  - `AAA` — na tela, exposto à AT e nomeado quando necessário.

> **Regra de bolso**: `display_visible:false` + `NONE` = realmente invisível.
> `display_visible:true` + `AA` = está lá (e acessível), só não está na tela
> agora — **não é falha de visibilidade**.

## Workflow de auditoria

### 1. Capturas

```bash
# Visão estrutural: landmarks, headings, links, imagens + contraste + AX
sniffCSS -u "$URL" -s "body" --depth 5 --compact --contrast --ax-tree \
  > body.jsonl

# Regiões profundas (menu, rodapé, formulários, carrossel) — contraste não
# depende da profundidade (resolvido in-page), o depth controla só o tamanho.
sniffCSS -u "$URL" -s "nav"    --depth 4 --compact --contrast > nav.jsonl
sniffCSS -u "$URL" -s "footer" --depth 6 --compact --contrast > footer.jsonl
sniffCSS -u "$URL" -s "main"   --depth 5 --compact --contrast > main.jsonl
sniffCSS -u "$URL" -s "form, #carouselExampleCaptions" --depth 3 --compact --contrast > forms.jsonl
```

### 2. Regras determinísticas

```bash
sniffCSS-check --input main.jsonl  --rules     # contrast-aa, target-size, focus-indicator, hidden-focusable, empty-alt-image
sniffCSS-check --input body.jsonl  --uniform   # o "card estranho" entre irmãos
```

`--rules` usa o facet `contrast` **medido pela engine** (fundo efetivo
resolvido): `fail` é falha real de AA, `warn` é fundo-imagem (revisão manual).

### 3. Leitura rápida dos resultados (jq)

```bash
# Falhas de contraste AA (só as reais)
sniffCSS-check --input main.jsonl --rules \
  | jq -r 'select(.check=="contrast-aa" and .status=="fail") | [.tag,.selector,.evidence]|@tsv'

# Interativos sem nome acessível (1.1.1/2.4.4/4.1.2)
jq -r '.. | objects | select(.tag=="A" or .tag=="BUTTON" or .tag=="IMG")
  | select(.aria.name==null or .aria.name=="") | select(.is_user_noticeable.accessibility_grade!="NONE")
  | [.tag,.selector,(.rect.width|tostring),(.rect.height|tostring)] | @tsv' body.jsonl

# Alvos < 24px (2.5.8)
jq -r '.. | objects | select(.tag=="A" or .tag=="BUTTON")
  | select(.is_user_noticeable.display_visible==true and (.rect.width<24 or .rect.height<24))
  | [.tag,.selector,(.aria.name//"-")] | @tsv' body.jsonl

# Hierarquia de títulos (sem H1 / pulos de nível = 1.3.1)
jq -r '.. | objects | select(.tag|test("^H[1-6]$")) | [.tag,(.aria.name//"-")] | @tsv' body.jsonl
```

## Checklist de julgamento (o que a IA deve conferir)

- [ ] Existe `<h1>`? A hierarquia não pula níveis (H2→H4 direto é falha 1.3.1).
- [ ] Landmarks presentes (`banner`/`navigation`/`main`/`contentinfo`); skip-links.
- [ ] Links, botões e imagens **sem nome** (`aria.name` vazio) → 1.1.1/2.4.4/4.1.2.
  - ⚠️ Se o `<a>` envolve `<img alt="...">`, o nome real vem do `alt` — a
    ferramenta ainda sub-reporta esse caso; confira antes de acusar.
- [ ] `contrast.aa == fail` → 1.4.3. `unknown` (fundo-imagem) → carrosséis/cards, revisar manualmente.
- [ ] Alvos `< 24px` (topbars, A+/A-, ícones) → 2.5.8.
- [ ] `accessibility_grade == NONE` em conteúdo que deveria ser lido → `aria-hidden`/`display:none` indevido.
- [ ] Conteúdo abaixo da dobra não é falha — `display_visible:true` + `AA` (presente na AX tree).

## Exemplo real (trecho de auditoria)

Rodapé de um portal (captura `footer`):

| Elemento | `display_visible` | Grade | Contraste |
|---|---|---|---|
| `footer.footer-section` | true | AA | — |
| texto copyright `#dddddd`/`#27498c` | true | AA | 6.39 pass |
| botão "Voltar" | true | AA | 4.69 pass |
| texto `#6c757d` sobre `#f8f9fa` | true | AA | **4.45 fail** |

O mesmo texto antes da correção de contraste saía como `unknown` (fundo
transparente). Agora a falha real de AA é detectada automaticamente.
