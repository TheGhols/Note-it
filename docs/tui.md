# `noteit-tui` — Arquitetura, Contratos e Empacotamento (Fase 5.0)

Este documento estabelece a arquitetura, os contratos de concorrência, as fronteiras de dependência e o escopo da interface de terminal interativa (**TUI**) e do empacotamento do Note-it, decididos na **Fase 5.0A**.

---

## 1. Runtime de TUI e Avaliação de Candidatos

### 1.1 Contexto e Orçamento de Dependências

Até o encerramento da Fase 4:
- `noteit-core` possui 40 crates em seu grafo (`serde`, `chrono`, `dirs`, `toml`, `uuid`, etc.) com zero dependências gráficas, zero rede e sem runtime assíncrono.
- `noteit-cli` possui um grafo enxuto e determinístico: despacho imediato de comandos, 0 ms de loop de eventos, impressão de texto ou JSON e encerramento imediato.
- `noteit-mcp` é o único componente headless com runtime assíncrono (`tokio` com features mínimas `rt`, `macros`, `io-std`, `sync`), exigido pelo transporte stdio do SDK MCP oficial (`rmcp`).

Uma TUI interativa introduz, pela primeira vez no terminal do Note-it, um loop de eventos persistente (captura de teclado, redimensionamento de janela, desenho de frames e navegação).

O orçamento arquitetural impõe:
1. **Zero dependência de rede:** nenhuma crate HTTP, TLS, WebSocket ou DNS no grafo da TUI.
2. **Zero dependência gráfica:** nenhum vínculo com GTK, GDK, WebKitGTK, layer-shell, Wayland ou Niri.
3. **Sem contaminação do Core ou da CLI:** a TUI não pode inflar o binário `noteit` nem alterar o `noteit-core`.
4. **Sem necessidade forçada de runtime assíncrono complexo:** se a interface puder operar de modo síncrono ou com event polling desacoplado, evita-se o peso e a sobrecarga de um runtime multithreaded assíncrono.

### 1.2 Medição de Candidatos no Ecossistema Rust

Foram mensurados três protótipos em ambiente descartável isolado (Rust 1.98 / x86_64, perfil `release` com strip), avaliando dependências transitivas, tempo de compilação limpa, tamanho de binário e modelo de execução:

| Candidato | Dependências Normais Únicas | Pacotes no Lockfile | Tempo de Compilação (`release`) | Binário (sem strip) | Binário (`stripped`) | Modelo de Runtime |
|---|---|---|---|---|---|---|
| **`crossterm` (puro, v0.29.0)** | 32 | 37 | 7,23 s | 612 KiB | 464 KiB | Event polling síncrono (`poll(Duration)`) |
| **`ratatui` (com backend `crossterm`, v0.30.2)** | 85 | 181 | 20,23 s | 830 KiB | 636 KiB | Modo imediato síncrono (`Terminal::draw`) |
| **`cursive` (com backend `crossterm`, v0.21.1)** | 73 | 100 | 35,56 s | 1,1 MiB | 811 KiB | Modo retido com callbacks e hierarquia de views |

### 1.3 Análise e Decisão

1. **`crossterm` puro:** embora possua o menor binário (464 KiB) e apenas 32 dependências, fornece somente abstração de baixo nível do terminal (modo raw, sequências de escape ANSI, eventos de tecla). Não possui solucionador de layout, sistema de widgets, renderização diferencial de buffers ou controle de bordas. Construir uma TUI com painéis múltiplos sobre ele exigiria reinventar um motor de layout no repositório.
2. **`cursive`:** impõe um paradigma retido baseado em callbacks de eventos e mutação de árvore de views. Levou 35,56 s de compilação release (+75% em relação ao Ratatui) e gerou o maior binário stripped (811 KiB). Além disso, seu modelo de concorrência e propriedades internas exigem gerenciar referências de callbacks com tipos dinâmicos menos transparentes.
3. **`ratatui` (backend `crossterm`):**
   - **Tamanho:** binário stripped de apenas 636 KiB (apenas +172 KiB sobre o `crossterm` puro).
   - **Compilação:** 20,23 s (43% mais rápido que o `cursive`).
   - **Arquitetura:** modelo imediato declarativo (`terminal.draw(|frame| { ... })`), amplamente testável sem terminal físico.
   - **Zero Async Runtime:** opera perfeitamente em loop síncrono simples (`crossterm::event::poll(Duration::from_millis(50))`), sem requerer `tokio` ou `async-std`.
   - **Fronteira limpa:** a varredura de dependências normais do `ratatui` confirma **zero crates de rede** (zero `reqwest`, `hyper`, `tokio`, `ureq`, etc.) e **zero crates gráficas de desktop** (zero `gtk`, `webkit`, `gdk`, `wayland`).

**Decisão:** adota-se **`ratatui`** com backend **`crossterm`**.

---

## 2. Localização no Workspace: Binário Dedicado `noteit-tui`

### 2.1 Avaliação: Subcomando da CLI vs. Crate Dedicado

- **Opção A — Subcomando em `noteit-cli` (`noteit tui`):**
  A CLI existente (`noteit-cli`) é um despachante de comandos rápidos em lotes: lê argumentos, processa no Core, cospe texto ou JSON na saída padrão e termina em poucos milissegundos. Se a TUI fosse incorporada como um módulo do `noteit-cli`, todas as 85 crates de interface e widgets seriam linkadas no binário `noteit`. Uma chamada simples como `noteit listar | jq` carregaria o peso de toda a pilha de interface gráfica de terminal. Além disso, `scripts/check-cli-boundary` precisaria ser afrouxado para aceitar manipulações de terminal complexas que o CLI batch não deve fazer.
- **Opção B — Crate e binário dedicado `noteit-tui`:**
  Segue rigorosamente a fronteira estabelecida na Fase 4, onde cada binário tem uma única responsabilidade no workspace:
  - `note-it` (GUI desktop Wayland em GTK4/WebKit).
  - `noteit` (CLI rápida e headless para shell e scripts).
  - `noteit-mcp` (servidor Model Context Protocol stdio para agentes).
  - `noteit-embed` (worker isolado para embeddings remotos).
  - `noteit-tui` (interface interativa em tela cheia no terminal).

### 2.2 Decisão

A TUI residirá em seu próprio crate: **`noteit-tui`**, gerando o binário **`target/release/noteit-tui`**.
- O crate depende diretamente de `noteit-core`.
- O crate NÃO expõe biblioteca para fora e tem `publish = false`.
- Um novo gate de qualidade, **`scripts/check-tui-boundary`**, será criado para garantir de forma mecânica e contínua:
  1. Zero dependência de bibliotecas gráficas de desktop (GTK, GDK, WebKit, layer-shell, Wayland, Niri).
  2. Zero dependência de rede (nenhuma crate HTTP, TLS ou socket de internet).
  3. Zero acesso direto ao filesystem para bypass do Core (todo acesso ao store passa por `noteit-core`).

*(Nota: no futuro, se for conveniente ao usuário, `noteit-cli` poderá ter um atalho ou comando que faça `exec` de `noteit-tui`, mas nunca linkar o código da TUI dentro do executável da CLI).*

---

## 3. Superfície de Dados e Invariantes

A TUI interage com o store do usuário exclusivamente como consumidora de biblioteca do `noteit-core`.

### 3.1 Caminhos Canônicos

- **Operações de Leitura:**
  - `noteit_core::open_read_only(&store_paths)`: abre o store em modo estritamente somente leitura.
  - Listagem de notas: `core.list_notes()`.
  - Leitura de documento: `core.read_note(id)`.
  - Busca lexical e ordenação: `noteit_core::search::search_notes` e módulos de busca.
  - Filtros semânticos: `noteit_core::filter::NoteFilter`.
  - Scanner de tarefas: `noteit_core::task::scan_tasks` e `toggle_task_line`.
  - Metadados e catálogo: `noteit_core::metadata::collect_tags`, `collect_properties`.
  - Lixeira: `noteit_core::trash::list_trash`.
- **Operações de Escrita:**
  - `noteit_core::authority::perform_at(&store_paths, &operation)`: todas as mutações trafegam obrigatoriamente por essa função, encapsuladas em `WriteOperation`.

### 3.2 Invariantes Absolutos

1. **Sem invocação de subprocessos:** a TUI **nunca** executa o binário `noteit` ou `note-it` via `std::process::Command` para ler ou escrever notas.
2. **Sem parsing de saída da CLI:** a TUI **nunca** consome ou analisa stdout humano ou o schema JSON da CLI.
3. **Sem escrita direta no disco:** a TUI **nunca** grava arquivos Markdown (`.md`), cria diretórios ou manipula arquivos de lixeira por conta própria. Toda persistência atômica pertence ao Core.
4. **Sem duplicação de lógica de domínio:** parsing de front matter, regexes de tarefas, regras de normalização de busca e cálculo de digest de revisão pertencem única e exclusivamente a `noteit-core`.

---

## 4. Concorrência com GUI e CLI (O Modelo do Gravador)

### 4.1 Análise dos Dois Modelos Possíveis

1. **Modelo do Detentor Persistente de Lease (Inviável):**
   Se o processo da TUI adquirisse o lease de escrita consultivo (`flock`) de forma persistente ao abrir e o mantivesse durante toda a sessão:
   - Se a GUI (`note-it`) estivesse aberta, a TUI seria impedida de abrir ou não poderia realizar alterações, pois a GUI mantém o lease por valor em seu `AppContext` (`WriteAuthority`, ADR-039).
   - Se a TUI abrisse primeiro e retivesse o lease, uma invocação da GUI de desktop falharia imediatamente na inicialização (pois a GUI recusa iniciar se não puder deter o lease exclusivo do store).
   - A TUI teria que abrir e manter um soquete de controle IPC próprio (`noteit-core/src/control.rs`) e congelar sua interface quando recebesse comandos externos da CLI (`noteit adicionar`).
2. **Modelo do Cliente Transacional (Adotado):**
   A TUI opera exatamente como a CLI (`noteit-cli`) e o servidor MCP (`noteit-mcp`): como um **cliente transacional** que consome `noteit_core::authority::perform_at`.

### 4.2 O Fluxo de Gravação Transacional

A TUI **não retém o lease de escrita** enquanto o usuário navega, lê ou visualiza notas. O lease só é disputado no instante de uma mutação atômica:

```text
Ação na TUI (concluir tarefa, criar nota, salvar edição, lixeira)
   │
   ▼
noteit_core::authority::perform_at(&store_paths, &operation)
   │
   ├─ 1. O lease está LIVRE (nenhum GUI desktop ativo):
   │     Adquire flock temporário brevemente ──▶ Grava atômico pelo Core ──▶ Libera flock.
   │
   ├─ 2. O lease está OCUPADO (GUI desktop `note-it` ativo):
   │     Conecta ao soquete Unix privado do desktop ──▶ Envia ControlRequest ──▶
   │     Desktop congela editor, aplica mutação sobre buffer ativo, commita e devolve resposta.
   │
   └─ 3. O lease está OCUPADO mas INALCANÇÁVEL:
         Falha de modo seguro (fail-closed), zero bytes gravados, erro amigável na TUI.
```

### 4.3 Concorrência Otimista com a GUI e a CLI (`revision`)

O que acontece se a TUI estiver aberta com a Nota A na tela e, simultaneamente, o usuário alterar a Nota A na janela gráfica (ou pela CLI)?

1. Ao carregar a nota na visualização ou no modo de edição, a TUI armazena a revisão canônica inicial:
   $$R_{\text{inicial}} = \text{note.revision} = \text{SHA-256 dos bytes canônicos Markdown}$$
2. Quando a TUI solicita uma alteração (via `WriteOperation::MutateNote`), ela envia obrigatoriamente `expected_revision: R_{\text{inicial}}`.
3. Se a nota foi alterada externamente no disco ou no editor gráfico enquanto a TUI exibia o texto antigo, o Core (ou a autoridade no desktop) rejeita a mutação com `WriteError::RevisionConflict`.
4. A TUI captura o `RevisionConflict`, **não sobrescreve os dados**, avisa o usuário visualmente sobre o conflito e oferece a opção de recarregar a versão mais recente ou salvar o trabalho em uma nova nota de rascunho.
5. Em repouso, a TUI pode verificar periodicamente o `mtime` ou detectar eventos do filesystem para sinalizar se a nota visualizada sofreu alterações externas.

---

## 5. Auditoria do Empacotamento Existente (`packaging/arch/`)

A inspeção física do diretório `packaging/arch/` revelou o seguinte estado:

### 5.1 O que existe hoje

Existe apenas um arquivo: `packaging/arch/PKGBUILD`.
```bash
pkgname=note-it
pkgver=0.1.0
pkgrel=1
pkgdesc="Minimalist sticky notes for Linux Wayland"
arch=('x86_64' 'aarch64')
url="https://github.com/TheGhols/Note-it"
license=('MIT')
depends=('gtk4' 'gtk4-layer-shell' 'webkitgtk-6.0' 'glib2')
makedepends=('rust' 'cargo' 'nodejs' 'pnpm')
source=("$pkgname-$pkgver.tar.gz::$url/archive/refs/tags/v$pkgver.tar.gz")
sha256sums=('SKIP')
```

### 5.2 Defeitos e Lacunas Identificados

1. **Tag `v0.1.0` inexistente (Download 404):**
   A URL `https://github.com/TheGhols/Note-it/archive/refs/tags/v0.1.0.tar.gz` retorna `HTTP 404 Not Found`. Um `makepkg` limpo falha imediatamente no download do tarball de origem.
2. **Dependência de `pnpm` do sistema:**
   `makedepends` lista `pnpm`. No ambiente de desenvolvimento do Arch Linux, o `pnpm` pode estar instalado em nível de usuário (`~/.local/bin/pnpm`) ou gerenciado via Node/Corepack. Se não estiver instalado globalmente como pacote do Arch, `makepkg` aborta antes da compilação.
3. **Binários omitidos no pacote:**
   O `PKGBUILD` compila o workspace com `cargo build --release --locked`, mas sua função `package()` instala **exclusivamente o binário desktop**:
   - `install -Dm755 "target/release/note-it" "$pkgdir/usr/bin/note-it"`
   - `cp -r ui/dist/* "$pkgdir/usr/share/note-it/ui/dist/"`
   - Omitidos inteiramente:
     - `target/release/noteit` (a CLI principal!).
     - `target/release/noteit-mcp` (o servidor MCP oficial!).
     - `target/release/noteit-embed` (o worker daemon de embeddings remotos da Fase 4.3D!).
     - Futuro: `target/release/noteit-tui` (a TUI da Fase 5).
4. **Ausência de pacote VCS (`-git`):**
   Não há `PKGBUILD` para `note-it-git`, inviabilizando que usuários do AUR compilem as versões mais recentes a partir do branch `main` antes do corte oficial da tag `v0.1.0`.

Essas correções e extensões tornam-se parte obrigatória do escopo da Fase 5.

---

## 6. Deliberadamente Fora de Escopo da Fase 5.0

No mesmo estilo dos marcos anteriores do Note-it, os seguintes itens são deliberadamente deixados fora desta fase, com as respectivas justificativas:

1. **Editor de texto completo embutido no terminal com emulação Vim/Emacs:**
   Implementar um editor de código completo dentro da TUI (com syntax highlighting, buffer undo/redo infinito e atalhos modais) exigiria milhares de linhas de código e adicionaria um escopo enorme. Em vez disso, a TUI suportará mutações rápidas (marcar/desmarcar tarefas, editar tags, títulos e apêndices) e, para edição de corpo longo, invocará o `$EDITOR` configurado pelo usuário (suspendendo a TUI temporariamente e restaurando-a ao final, validando a `revision` antes de persistir).
2. **Modo daemon ou serviço persistente de background para a TUI:**
   A TUI é uma aplicação de primeiro plano orientada a terminal, encerrada com `q` ou `Esc`. O ciclo de vida do store e o gerenciamento de eventos continuam centralizados no processo desktop `note-it --background` ou no Core headless.
3. **Protocolos gráficos de terminal (Sixel, Kitty graphics, iTerm2):**
   A renderização gráfica de imagens anexadas (`assets/`) no terminal adiciona complexidade desnecessária e comportamento não padronizado entre diferentes emuladores de terminal. Imagens são representadas textualmente por suas referências.
4. **Empacotamento para distribuições não-Arch (Debian, Fedora, Flatpak, Snap):**
   O foco primário de empacotamento na Fase 5 permanece na plataforma de referência do Note-it: Arch Linux Wayland (AUR), mantendo os manifestos enxutos e auditáveis.
5. **Sincronização em nuvem ou compartilhamento de notas em rede:**
   Violação do princípio basilar de produto: local-first, zero telemetria, zero rede no Core.

---

## 7. Subdivisão Proposta para a Fase 5.0

Seguindo o padrão disciplinado das Fases 4.0 a 4.3:

- **Fase 5.0A — Arquitetura, medição e escopo (TUI + Empacotamento) [Esta Fase]**:
  - Medição de bibliotecas candidatas (`ratatui` + `crossterm` escolhido com base em métricas reais).
  - Definição da fronteira do crate `noteit-tui` e modelo cliente transacional de escrita.
  - Auditoria dos problemas de `packaging/arch/PKGBUILD`.
  - Documentação completa em `docs/tui.md`, `docs/roadmap.md` e `CHANGELOG.md`.
  - Gate de saída: zero `.rs` tocado, `Cargo.lock` byte-idêntico, aprovação do escopo.
- **Fase 5.0B — Crate `noteit-tui`, Shell de Terminal e Portão de Fronteira**:
  - Criação do crate `noteit-tui` no workspace e binário `noteit-tui`.
  - Configuração de dependências (`ratatui` com `crossterm`, `noteit-core`).
  - Criação de `scripts/check-tui-boundary` (proíbe GTK/WebKit/Wayland e rede).
  - Inicialização de modo raw, alternate screen e captura graciosa de sinais de encerramento (sem deixar o terminal quebrado).
  - Loop de eventos síncrono com saída por `q`/`Esc`.
  - Gate de saída: teste de pty automatizado comprovando restauração do terminal e gate de fronteira verde.
- **Fase 5.0C — Modo Leitura, Navegação e Apresentação**:
  - Painel lateral (navegação em notas recentes, tarefas pendentes, lixeira) e painel de visualização.
  - Integração com leitura do Core (`open_read_only`), busca com `/` e filtros.
  - Renderização de Markdown no terminal (cabeçalhos, listas, caixas de seleção, blocos).
  - Gate de saída: testes com stores sintéticos em pseudoterminal comprovando navegação por teclas sem corrupção de layout.
- **Fase 5.0D — Modo Edição, Mutação e Concorrência Transacional**:
  - Alternância rápida de tarefas (`[ ]` <-> `[x]`) diretamente pela TUI.
  - Criação e exclusão de notas via `noteit_core::authority::perform_at`.
  - Spawn do `$EDITOR` externo para edições completas de corpo com suspensão/retorno do terminal.
  - Verificação rigorosa de concorrência com a GUI: teste de `RevisionConflict` e envio por soquete privado quando o desktop estiver ativo.
  - Gate de saída: testes com harness isolado (`scripts/note-it-isolated`) cobrindo mutações com o daemon de desktop rodando em paralelo.
- **Fase 5.0E — Empacotamento Completo e Distribuição**:
  - Atualização do `packaging/arch/PKGBUILD` para empacotar os 5 binários: `note-it`, `noteit`, `noteit-mcp`, `noteit-embed`, `noteit-tui`.
  - Criação de `packaging/arch/PKGBUILD-git` para desenvolvimento no trunk.
  - Atualização de `scripts/build.sh` para verificar os 5 executáveis.
  - Validação de build em chroot limpo (`makechrootpkg` ou mock local).
  - Gate de saída: pacote `.pkg.tar.zst` inspecionado contendo todos os binários e assets sem conflitos.
- **Fase 5.0R — Auditoria de Segurança, Regressão e Fechamento**:
  - Prova de que todos os 6 gates de fronteira passam ilesos.
  - Prova de zero regressão no Core, na CLI, no MCP e na GUI.
  - CI remoto integralmente verde.
