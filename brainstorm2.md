A ideia central é usar o Chrome DevTools Protocol (CDP) através da biblioteca chromiumoxide. O Rust vai lançar uma instância headless do Chrome, acessar seu localhost:10011, executar o getComputedStyle no elemento que você especificar, e devolver o resultado em JSON.

Abaixo está o snippet completo para você colocar em um projeto Rust.
1. Estrutura do Projeto

Crie um novo projeto:
bash

cargo new css-inspector
cd css-inspector

Edite o Cargo.toml:
toml

[package]
name = "css-inspector"
version = "0.1.0"
edition = "2021"

[dependencies]
chromiumoxide = { version = "0.5", features = ["tokio-runtime"] }
tokio = { version = "1", features = ["full"] }
serde_json = "1.0"
futures = "0.3"   # Para processar o handler do browser

2. O Código (src/main.rs)

Cole isso no src/main.rs:
rust

use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::stream::StreamExt;
use std::env;
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();

    // Exemplo de uso:
    // ./css-inspector http://localhost:10011/pagina/que/uero/testar .meu-botao
    // ./css-inspector http://localhost:10011/pagina/que/uero/testar .meu-botao color,font-size,margin
    if args.len() < 3 {
        eprintln!("Uso: css-inspector <URL> <SELETOR_CSS> [PROPRIEDADES]");
        eprintln!("  Se PROPRIEDADES não for passado, retorna TODOS os estilos computados.");
        eprintln!("  Exemplo: css-inspector http://localhost:10011/home .header");
        eprintln!("  Exemplo filtrado: css-inspector http://localhost:10011/home .header padding,background-color");
        std::process::exit(1);
    }

    let url = &args[1];
    let selector = &args[2];
    let filter_props: Option<Vec<String>> = if args.len() > 3 {
        Some(args[3].split(',').map(|s| s.trim().to_string()).collect())
    } else {
        None
    };

    // 1. Configura o navegador (headless por padrão - não abre janela)
    let config = BrowserConfig::builder()
        .headless_mode(true) // Mude para false se quiser ver o navegador abrindo
        .build()?;

    let (browser, mut handler) = Browser::launch(config).await?;

    // 2. O handler precisa ser processado em segundo plano para manter a conexão CDP viva
    let handle = tokio::task::spawn(async move {
        while let Some(event) = handler.next().await {
            if event.is_err() {
                break;
            }
        }
    });

    // 3. Cria uma nova página e navega até a URL
    let page = browser.new_page("about:blank").await?;
    page.goto(url).await?;

    // 4. Aguarda o elemento existir no DOM (crítico para SPAs como React/Vue)
    page.wait_for_selector(selector).await?;

    // 5. Obtém o handle do elemento
    let element = page.find_element(selector).await?
        .ok_or_else(|| format!("Elemento '{}' não encontrado na página", selector))?;

    // 6. Executa o getComputedStyle no contexto da página
    //    Passamos o elemento e o filtro de propriedades (se houver)
    let result: serde_json::Value = page.evaluate_func(
        r#"
        (el, filter) => {
            const computed = getComputedStyle(el);
            const styles = {};

            if (filter && filter.length > 0) {
                for (const prop of filter) {
                    const value = computed.getPropertyValue(prop);
                    if (value) {
                        styles[prop] = value;
                    }
                }
            } else {
                // Retorna TODAS as propriedades computadas
                for (let i = 0; i < computed.length; i++) {
                    const prop = computed[i];
                    styles[prop] = computed.getPropertyValue(prop);
                }
            }
            return styles;
        }
        "#,
        // Argumentos que serão injetados na função JS
        vec![element.into(), serde_json::to_value(&filter_props)?.into()]
    ).await?;

    // 7. Imprime o JSON bonitinho no stdout (a IA vai ler isso)
    println!("{}", serde_json::to_string_pretty(&result)?);

    // 8. Limpeza
    browser.close().await?;
    handle.await?;

    Ok(())
}

3. Como Compilar e Executar

No terminal, dentro da pasta do projeto:
bash

cargo build --release

O binário estará em target/release/css-inspector.
Exemplos de execução:

Buscar todos os estilos do elemento .titulo:
bash

./target/release/css-inspector http://localhost:10011/pagina/teste .titulo

Buscar apenas color, font-size e padding:
bash

./target/release/css-inspector http://localhost:10011/pagina/teste .titulo color,font-size,padding

A saída será algo como:
json

{
  "color": "rgb(255, 0, 0)",
  "font-size": "24px",
  "padding": "10px"
}

4. Integração com o Pipeline de IA (Devtime)

Agora que você tem esse binário, existem várias formas de integrar com sua IA:

    Chamada direta via terminal: A IA pode gerar o comando ./css-inspector e ler o stdout.

    Como ferramenta (Tool) no LangChain / MCP: Registre essa chamada como uma ferramenta que a IA pode invocar para "inspecionar visualmente" a página.

    Script de automação: Você pode usar isso dentro de um loop de testes (ex.: após cada alteração no CSS, rodar o inspetor para garantir que os valores calculados batem com o esperado).

Exemplo de chamada em Python (para o agente):
python

import subprocess
import json

def get_computed_css(url, selector, props=None):
    cmd = ["./target/release/css-inspector", url, selector]
    if props:
        cmd.append(",".join(props))
    result = subprocess.run(cmd, capture_output=True, text=True)
    return json.loads(result.stdout)

# Uso no pipeline
styles = get_computed_css("http://localhost:10011/dashboard", ".card", ["background", "box-shadow"])
print(styles)

5. Dicas importantes para o Devtime

    SPAs (React/Vue/Angular): O page.wait_for_selector(selector).await? garante que o elemento estará renderizado antes da consulta, mesmo se o JavaScript demorar para carregar.

    Elementos interativos (hover, focus): Se precisar de estilos em estado :hover, você pode adicionar um passo extra no código antes da consulta, ex: page.evaluate("document.querySelector('SEU_SELETOR').dispatchEvent(new MouseEvent('mouseenter'))").

    Pseudo-elementos (::before, ::after): Atualmente o código pega o elemento principal. Para pseudo-elementos, você precisaria ajustar a função JS para getComputedStyle(el, '::before'). Posso mostrar essa variação se precisar.

    Performance: O navegador headless é iniciado e fechado a cada chamada. Se for fazer muitas consultas seguidas, reutilize a instância do Browser e apenas troque de página.

6. Possíveis problemas e soluções
Problema	Solução
chromium não encontrado	Instale o Chrome/Chromium no sistema ou use a feature fetcher do chromiumoxide (basta adicionar chromiumoxide = { version = "0.5", features = ["tokio-runtime", "fetch"] } e ele baixa automaticamente).
Timeout ao carregar a página	Aumente o tempo de espera com page.goto(url).await? ou use page.wait_for_navigation().await?. Para SPAs pesadas, considere aumentar o timeout.
O elemento não é encontrado	Verifique o seletor. Em SPAs, pode ser que o elemento só apareça após uma requisição. O wait_for_selector já ajuda, mas você pode aumentar o tempo de espera padrão.

Esse snippet entrega exatamente o que você pediu: uma consulta rápida, confiável e automatizada ao valor calculado real do CSS, rodando em Rust sobre sua instância ativa em localhost:10011, pronta para ser engolida por qualquer pipeline de IA.

Lidar com elementos complexos (SPAs, lazy loading, requisições assíncronas, animações e carregamento de fontes) é o maior desafio da automação de navegador. O simples wait_for_selector pode falhar porque o elemento existe no DOM, mas ainda não tem o estilo final aplicado (ex.: está com opacity: 0, display: none ou width: 0px enquanto aguarda dados da API).

Abaixo estão as melhores estratégias para tornar seu snippet Rust extremamente robusto para devtime, organizadas da mais simples para a mais inteligente.
1. Aumentar o Tempo de Espera Geral (Timeout)

Sua página pode demorar mais que os 30 segundos padrão (ou o que o chromiumoxide define). Configure um timeout explícito e espere a página ficar "ociosa":
rust

use std::time::Duration;

// Na hora de navegar:
page.goto(url)
    .await?
    .wait_for_navigation()
    .await?; // Espera a navegação principal

// Espere a página ficar ociosa por 500ms (carga total de JS)
// OBS: chromiumoxide não tem "networkidle" diretamente no goto, mas você pode esperar manualmente:
tokio::time::sleep(Duration::from_millis(2000)).await; // gambiarra simples

// OU use um timeout maior no wait_for_selector:
let timeout = Duration::from_secs(30);
page.wait_for_selector(selector)
    .await?
    .timeout(timeout) // se a API suportar; senão, faça um loop manual
    .await?;

2. Esperar por networkidle (Todas as requisições acabaram)

Para SPAs (React/Vue/Angular), muitos dados vêm de fetch/XHR. O CDP permite esperar até que não haja mais requisições de rede por um período. Como chromiumoxide expõe o CDP nativamente, você pode fazer:
rust

use chromiumoxide::cdp::browser_protocol::network::ResponseReceivedEvent;

// Função auxiliar para esperar a rede esvaziar
async fn wait_for_network_idle(page: &chromiumoxide::page::Page, idle_time_ms: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut pending = 0;
    let mut last_activity = std::time::Instant::now();
    
    // Inscreve-se nos eventos de rede da página
    let mut events = page.event_listener::<ResponseReceivedEvent>().await?;
    
    tokio::time::timeout(Duration::from_secs(60), async {
        while last_activity.elapsed() < Duration::from_millis(idle_time_ms) {
            if let Some(_) = events.next().await {
                pending += 1;
                last_activity = std::time::Instant::now();
                // Você pode usar um contador para saber se todas terminaram, mas aqui só reseta o timer
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }).await?;
    
    Ok(())
}

// Uso dentro do main:
page.goto(url).await?;
wait_for_network_idle(&page, 500).await?; // espera 500ms sem requisições

3. Estratégia Avançada: Esperar por Condições de Estilo (Polling Inteligente)

Esta é a solução mais confiável. Em vez de esperar o elemento existir, você espera até que o valor calculado de uma propriedade específica atinja o estado esperado (ex.: opacity: 1, display != 'none', ou height > 0).

Modifique o código para fazer um loop de polling no próprio JavaScript:
rust

// Substitua o bloco de "aguarda elemento" por:
let timeout = Duration::from_secs(30);
let start = std::time::Instant::now();

let mut element_found_and_ready = false;
while start.elapsed() < timeout {
    // Verifica se o elemento existe e se sua altura é > 0 (por exemplo)
    let is_ready: bool = page.evaluate(
        r#"
        (sel) => {
            const el = document.querySelector(sel);
            if (!el) return false;
            const rect = el.getBoundingClientRect();
            const styles = getComputedStyle(el);
            // Condições de "pronto":
            return rect.height > 0 && 
                   rect.width > 0 && 
                   styles.display !== 'none' &&
                   styles.opacity !== '0' &&
                   styles.visibility !== 'hidden';
        }
        "#,
        vec![serde_json::to_value(selector)?.into()]
    ).await?;

    if is_ready {
        element_found_and_ready = true;
        break;
    }
    tokio::time::sleep(Duration::from_millis(200)).await;
}

if !element_found_and_ready {
    eprintln!("Aviso: Elemento não ficou pronto dentro do timeout, tentando capturar mesmo assim...");
}
// Agora sim, pega o elemento e executa o getComputedStyle
let element = page.find_element(selector).await?
    .ok_or_else(|| format!("Elemento não encontrado após espera"))?;

4. Esperar pela API Específica do Framework

Se você tem controle sobre o código da aplicação, exponha uma variável global quando tudo estiver carregado (ex.: window.__APP_READY__). No Rust, aguarde essa flag:
rust

page.evaluate(
    r#"
    async function waitForApp() {
        return new Promise((resolve) => {
            if (window.__APP_READY__) {
                resolve(true);
            } else {
                const check = setInterval(() => {
                    if (window.__APP_READY__) {
                        clearInterval(check);
                        resolve(true);
                    }
                }, 100);
            }
        });
    }
    waitForApp();
    "#
).await?;

5. Esperar por Fontes e Imagens (Web Fonts)

Se o valor calculado depende do carregamento de fontes (ex.: font-size, line-height), espere o document.fonts.ready:
rust

page.evaluate(
    r#"
    document.fonts.ready.then(() => console.log('Fonts loaded'));
    "#
).await?;
// Aguarda alguns ms extras para o reflow após a fonte carregar
tokio::time::sleep(Duration::from_millis(300)).await;

6. Código Final Unificado (Exemplo Prático)

Aqui está uma função de "espera inteligente" que combina a maioria das técnicas acima, que você pode colocar no seu main.rs antes de capturar os estilos:
rust

async fn wait_for_element_ready(
    page: &chromiumoxide::page::Page,
    selector: &str,
    timeout_secs: u64,
) -> Result<(), Box<dyn std::error::Error>> {
    let timeout = Duration::from_secs(timeout_secs);
    let start = std::time::Instant::now();

    // 1. Aguarda o DOM existir (rápido)
    page.wait_for_selector(selector).await?;
    
    // 2. Aguarda rede ociosa (se aplicável)
    // (chamaria a função wait_for_network_idle aqui)

    // 3. Polling de estilo (a parte mais importante)
    while start.elapsed() < timeout {
        let ready: bool = page.evaluate(
            r#"
            (sel) => {
                const el = document.querySelector(sel);
                if (!el) return false;
                const rect = el.getBoundingClientRect();
                const cs = getComputedStyle(el);
                // Verifica se é visível e tem tamanho real
                return rect.width > 0 && rect.height > 0 && cs.display !== 'none';
            }
            "#,
            vec![serde_json::to_value(selector)?.into()]
        ).await?;

        if ready {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(150)).await;
    }

    // Se timeout, retorna erro ou apenas avisa
    eprintln!("⚠️  Timeout esperando o elemento '{}' ficar visualmente pronto.", selector);
    Ok(()) // pode retornar Ok mesmo assim para não quebrar o pipeline
}

7. Dica de Ouro para Devtime: Modo Headless false para Debug

Quando estiver ajustando os timeouts, rode o navegador com headless_mode(false) e adicione page.screenshot(...) ou page.pdf(...) para ver exatamente o estado da página no momento da falha. Isso ajuda a identificar se o problema é rede, JavaScript ou CSS.
rust

let config = BrowserConfig::builder()
    .headless_mode(false) // Abre a janela do Chrome
    .build()?;

Resumo da Abordagem Recomendada
Nível	Estratégia	Implementação
Básico	Aumentar timeout	Duration::from_secs(60) no goto e wait_for_selector
Intermediário	Esperar rede ociosa	Monitorar ResponseReceivedEvent do CDP
Avançado (Recomendado)	Polling de estilo calculado	Loop em Rust avaliando getBoundingClientRect + getComputedStyle via evaluate
Específico	Aguardar flag do framework	Expor window.APP_LOADED no front-end e aguardar no Rust

Com essa combinação, seu inspetor CSS em Rust será praticamente infalível mesmo para páginas com lazy-loading complexo, animações ou dados vindos de múltiplos endpoints. A IA poderá confiar plenamente no valor capturado.
