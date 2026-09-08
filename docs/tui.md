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

---

## 8. Detalhes de Implementação da Fase 5.0B (Shell e Portão de Fronteira)

A **Fase 5.0B** construiu o esqueleto fundamental do `noteit-tui`:

### 8.1 Estrutura do Crate e Dependências
- Crate adicionado ao workspace em `noteit-tui/` com binário único `target/release/noteit-tui`.
- Dependências de produção: `ratatui` (0.30.2 com backend `crossterm`), `crossterm` (0.29.0), `signal-hook` (0.3.18) e `noteit-core` (path dependency).
- Dependências de desenvolvimento: `libc` (0.2) e `tempfile` (3.14) para os testes de pseudoterminal (PTY).
- Sem runtime assíncrono (sem `tokio`), sem rede e sem bibliotecas de interface desktop.

### 8.2 Ciclo de Vida e Restauração Segura do Terminal (`terminal.rs`)
- **Guarda RAII (`TerminalGuard`):** Instanciação segura que ativa o modo raw (`enable_raw_mode()`), entra no alternate screen (`EnterAlternateScreen`), oculta o cursor e captura o terminal crossterm. Implementa a trait `Drop` para garantir de forma infalível a desativação do modo raw (`disable_raw_mode()`), o retorno à tela padrão (`LeaveAlternateScreen`), a visibilidade do cursor (`Show`) e a liberação de buffers na saída normal ou por retorno antecipado.
- **Hook de Pânico Customizado (`install_panic_hook`):** Antes de qualquer inicialização de terminal, instala-se um panic hook global. Caso ocorra um panic em qualquer ponto do runtime da TUI, o hook restaura imediatamente o modo cozido do terminal e sai do alternate screen antes de chamar o tratador padrão de panic do Rust. Isso elimina completamente o clássico defeito de "terminal quebrado/inutilizável" após falhas inesperadas.
- **Tratamento de Sinais POSIX (`signal-hook`):** Registro assíncrono-seguro de handlers para `SIGINT` e `SIGTERM`. Ao receber qualquer sinal, uma flag booleana atômica (`SHUTDOWN_REQUESTED`) é alterada. O loop síncrono faz polling de eventos a cada 50 ms (`crossterm::event::poll`) e inspeciona a flag antes e depois de cada espera, terminando imediatamente o loop e acionando o destrutor do `TerminalGuard`.

### 8.3 Redimensionamento e Eventos (`app.rs` e `ui.rs`)
- **Redimensionamento Seguro:** Eventos `Event::Resize(w, h)` emitidos pelo crossterm são consumidos no loop e o Ratatui realiza `terminal.autoresize()` antes de cada `terminal.draw()`.
- **Layout com Guardas de Dimensão:** O renderizador em `ui.rs` verifica dimensões mínimas utilizáveis (ex.: largura ou altura degeneradas), desenhando layouts responsivos com `Constraint::Percentage` e `Constraint::Min` sem causar pânico de fatiamento de tela.
- **Comandos de Saída:** O loop encerra deterministicamente pelas teclas `q`, `Esc` e `Ctrl+C`.

### 8.4 Validações de Pré-Voo (Fail-Fast antes do Modo Raw)
- **Store Ausente:** O caminho do store é resolvido via `StorePaths::resolve()` antes de ativar o modo raw ou alternate screen. Caso `paths.notes_dir.exists()` seja falso, a TUI imprime diagnóstico claro em `stderr` (`Error: O diretório de notas não existe: <caminho>`) e finaliza com código 1, mantendo o terminal do usuário limpo.
- **Não-TTY:** Se `stdout` não for um terminal interativo (`io::stdout().is_terminal() == false`), a TUI recusa a execução em lote/pipe com código 1, orientando o usuário a utilizar a CLI (`noteit`).

### 8.5 Portão de Fronteira Mecânico (`scripts/check-tui-boundary`)
- Script de verificação contínua executado no CI e no `./scripts/check` (`tui-boundary`).
- Impede terminantemente a introdução de:
  - Bibliotecas de GUI de desktop (`gtk`, `gdk`, `webkit`, `layer-shell`, `wayland`, `niri`).
  - Runtimes assíncronos ou clientes de rede (`tokio`, `hyper`, `reqwest`, `ureq`, `curl`, etc.).
  - Desvios diretos de I/O em disco acessando arquivos Markdown do store sem transitar por `noteit-core`.

### 8.6 Testes Automatizados em Pseudoterminal (`tests/terminal_lifecycle.rs`)
- Harness de testes sobre PTY Unix (`openpty` via `libc`), medindo o estado de `termios` (`c_lflag` com flags `ICANON` e `ECHO`) antes e após a execução do binário real.
- Suíte de 9 testes automatizados cobrindo:
  1. Restauração de `termios` após saída com `q`.
  2. Restauração de `termios` após saída com `Esc`.
  3. Restauração de `termios` após interrupção por `Ctrl+C`.
  4. Restauração de `termios` e shutdown limpo ao receber sinal `SIGTERM`.
  5. Restauração de `termios` e saída de alternate screen em caso de panic controlado (`--panic-for-test`).
  6. Redimensionamento de janela via ioctl `TIOCSWINSZ` sem corrupção gráfica.
  7. Rejeição com erro explicativo antes do modo raw quando o store não existe.
  8. Rejeição de redirecionamentos em ambientes sem TTY.
  9. Execução das opções `--help` e `--version` sem inicializar terminal gráfico.

---

## 9. Detalhes de Implementação da Fase 5.0C (Modo Leitura, Navegação e Apresentação)

A **Fase 5.0C** implementou a experiência de leitura, navegação e apresentação somente-leitura da TUI:

Correção do relatório da 5.0C: `git diff d6ed712609d78b085650dc298aa452f0c86de6c4 -- noteit-tui/tests/terminal_lifecycle.rs` retornou saída vazia na verificação de 2026-09-07 (cenário a). O arquivo permanece idêntico à versão da 5.0B; a divergência dos nomes anteriormente reportados foi um erro de transcrição, sem alteração no código dos testes. Os nomes reais dos nove testes são:

1. `test_normal_exit_with_q_and_termios_restoration`
2. `test_normal_exit_with_esc_and_termios_restoration`
3. `test_ctrl_c_in_raw_mode_exits_cleanly`
4. `test_sigterm_signal_restores_terminal`
5. `test_controlled_panic_restores_terminal_via_panic_hook`
6. `test_resize_event_adapts_without_corruption`
7. `test_missing_store_fails_with_clear_message_before_raw_mode`
8. `test_non_tty_stdout_fails_with_clear_diagnostic`
9. `test_cli_flags_help_and_version`

### 9.1 Painéis de Navegação por Teclado
- **Notas Recentes (`RecentNotes`):**
  - Consome `core.list_summaries(&NoteFilter::default(), None)`.
  - Ordenação estrita por recência canônica do Note-it (`updated_at` decrescente, desempates por `mtime` e UUID). NUNCA utiliza `mtime` do sistema de arquivos como critério primário.
  - Exibe rótulo da nota, data formatada (`YYYY-MM-DD HH:MM`) e tags associadas (`#tag`).
- **Tarefas Pendentes (`PendingTasks`):**
  - Consome `core.list_tasks(TaskStateFilter::Pending, &NoteFilter::default(), None)`.
  - Reutiliza diretamente o scanner de tarefas de `noteit-core` (`task::parse_tasks`), sem duplicar expressões regulares.
  - Exibe indicador de tarefa pendente `☐`, o texto limpo da tarefa e a identificação da nota de origem `[<label>]`.
  - Pressionar `Enter` na tarefa abre a nota correspondente no painel de leitura.
- **Lixeira (`Trash`):**
  - Consome `core.list_trash()`, listando notas descartadas na pasta `trash/`.
  - Apresenta indicador `🗑️`, rótulo da nota e data de exclusão.
  - Modo estritamente somente-leitura: exibe prévia do snippet e metadados no painel de leitura, sem suporte a restauração (restauração é mutação reservada para a Fase 5.0D).
- **Atalhos de Navegação:**
  - `Tab` / `BackTab` (Shift+Tab): alternância cíclica entre painéis (`Notas Recentes` ↔ `Tarefas Pendentes` ↔ `Lixeira`).
  - `1`, `2`, `3`: seleção direta do painel desejado.
  - Setas `Up`/`Down` e teclas `k`/`j`: navegação na lista do painel ativo.
  - `Home`/`g` e `End`/`G`: salto para o início e fim da lista.
  - `Enter`, `Right` ou `l`: transição de foco para o painel de leitura.
  - `Esc`, `Left` ou `h`: retorno de foco para a lista de navegação.

### 9.2 Busca Rápida via Tecla `/`
- Acionada a qualquer momento pela tecla `/`.
- Exibe campo de consulta interativo no cabeçalho superior (`🔍 Busca Rápida no Core`).
- **Delegação ao Core:** consome diretamente `core.search_notes(&query)`. Reaproveita integralmente o motor léxico e de dobra de caixa/acentos (`search::fold`), contagem de correspondências e ranqueamento sem duplicar lógica de matching.
- Resultados atualizados dinamicamente no painel lateral à medida que o usuário digita.
- Navegação entre resultados via setas `Up`/`Down`. `Enter` abre a nota selecionada no painel de leitura; `Esc` cancela a busca e restaura o painel anterior.

### 9.3 Formatador e Renderizador de Markdown (`markdown.rs`)
Renderiza documentos Markdown para linhas estilizadas do Ratatui (`Line<'static>`):
- **Títulos (Headings):** Níveis 1 a 6 (`# ` a `###### `) estilizados com distinção visual de negrito, itálico e cores hierárquicas (amarelo, ciano, verde, branco, cinza).
- **Listas:** Marcadores de listas não ordenadas (`-`, `*`, `+`) renderizados com marcadores `• ` em ciano, preservando indentação; listas numeradas (`1.`, `2.`) com números preservados.
- **Checkboxes de Tarefas:**
  - Pendentes: `☐ ` em amarelo e texto normal.
  - Concluídas: `☑ ` em verde e texto riscado/esmaecido.
  - Metadados de conclusão (`<!-- note-it:completed_at=... -->`) removidos do texto visível via `noteit_core::task::extract_completed_at`.
- **Citações e Alertas GFM (Phase 3.5):**
  - Citações comuns (`> `) formatadas com barra vertical `│ ` em ciano/itálico.
  - Alertas GFM (`> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]`, `> [!CAUTION]`) reconhecidos em qualquer caixa e formatados com barra vertical `▍ ` com seus rótulos visíveis destacados: `[NOTA]`, `[DICA]`, `[IMPORTANTE]`, `[AVISO]`, `[CUIDADO]`.
  - Marcadores de alertas não suportados (`> [!FOO]`) são preservados como citações comuns sem perda ou corrupção de conteúdo (conforme ADR-026).
- **Blocos de Código:** Cercados por crases (```` ``` ````) ou tis (`~~~`), com cabeçalho indicando a linguagem declarada (`┌── [lang] ──`), corpo em texto simples em fonte monoespaçada e rodapé delimitador (`└───`). Zero dependência de syntax highlighting (`syntect` proibido).
- **Cálculos e Declarações Matemáticas:** Linhas iniciando com `=` ou declarações de variáveis `:=` são renderizadas como texto Markdown cru, sem avaliação de expressões matemáticas (conforme especificação da Fase 3.6/3.7).

### 9.4 Testes Automatizados com `TestBackend` (`tests/navigation_and_rendering.rs`)
- Suíte automatizada com 7 testes de integração executados contra store sintético descartável:
  1. `test_recency_ordering_follows_updated_at`: comprova que a listagem de notas segue estritamente `updated_at` decrescente.
  2. `test_panel_navigation_shortcuts`: testa alternância de painéis via `Tab`, `BackTab` e teclas numéricas `1`, `2`, `3`.
  3. `test_pending_tasks_extraction_and_opening`: valida extração de tarefas pendentes do Core e abertura da nota de origem por `Enter`.
  4. `test_trash_listing_and_preview`: valida listagem e prévia de itens da lixeira.
  5. `test_quick_search_delegates_to_core`: comprova ativação de busca com `/`, consulta ao Core e seleção por `Enter`.
  6. `test_rendered_buffer_contains_all_blocks_via_test_backend`: asserção direta do buffer do `TestBackend` validando a presença de todos os tipos de blocos renderizados (H1-H3, listas, checkboxes, blockquote, os 5 alertas GFM, código e cálculos).
  7. `test_dump_all_block_types_snapshot`: geração de dump do buffer renderizado comprovando apresentação sem display físico.

## 10. Fase 5.0D.R0 — Fechamento do Contrato de Mutação Concorrente

Subfase isolada de contrato, anterior à implementação funcional da 5.0D.
Baseline: `bf097742293f092a1cbc97e97a83243fe2f80bf5`.
Não implementa atalhos, edição, toggle, confirmação, `$EDITOR` ou recovery na TUI.
O fechamento depende dos gates locais e do CI remoto; o roadmap registra esse estado.

### 10.1 Contrato público e protocolo privado

| Operação de `WriteOperation` | Precondição | Execução no Core |
| --- | --- | --- |
| `CreateNote { draft }` | Destino ausente; não existe revision anterior | `write::create_note` → `StorageManager::create_note_atomic` → `atomic_file::create_atomic` |
| `MutateNote { selector, mutation, expected_revision }` | `Some(revision)` obrigatório para o futuro caller TUI | Contrato existente, preservado |
| `DiscardNote { selector, expected_revision }` | Revision obrigatória da nota ativa lida | `write::execute` → `ensure_revision_matches` → `StorageManager::move_note_to_trash` |
| `RestoreFromTrashAtRevision { selector, expected_revision }` | Revision obrigatória do item lido em trash | `write::execute` → `read_trash_note` → `ensure_revision_matches` → `restore_note` → `StorageManager::restore_note_from_trash` |

`RestoreFromTrash { selector }` conserva a assinatura e o comportamento dos
consumidores humanos existentes. A restauração condicionada usa uma variante
distinta: um decoder antigo não pode ignorar um campo e restaurar sem validar.
`NoteItCore::read_trash_note` usa o mesmo parser canônico e as mesmas verificações
de identidade/arquivo regular de `read_note`; `write::revision_of` calcula o token.
Não há revision fictícia para criação.

O protocolo privado passa de **2 para 3**. Isso também é necessário para criação:
embora sua assinatura seja igual, uma autoridade v2 ainda poderia usar a
persistência substitutiva. A checagem de versão existente recusa o pedido antes
de executá-lo. Não muda o schema público JSON da CLI nem o handshake MCP.
O novo resultado é `WriteOutcomeKind::NoteDiscarded`. A CLI somente completa
seus dois matches exaustivos; não ganha comando de descarte. O MCP não ganha tool.

### 10.2 Publicação atômica não é o mesmo que durabilidade

Criação segue esta sequência:

1. Criar temporário **exclusivo** (`create_new`) e privado no mesmo diretório do destino.
2. Escrever todo o documento e executar `sync_all` no arquivo: sincroniza o
   conteúdo antes de sua publicação, mas ainda não publica o nome final.
3. Executar `std::fs::hard_link(temp, destino)`: este é o ponto de commit.
   O filesystem publica o nome somente se ele estiver ausente. Um nome existente
   faz a operação falhar, preservando seus bytes. Não há `exists()` antes da
   publicação nem fallback para `rename`. Leitores do nome final nunca veem o
   arquivo parcialmente escrito.
4. Remover o nome temporário. Após a publicação, a nota já existe pelo nome final;
   falha de limpeza gera aviso, não transforma um commit em erro.
5. Sincronizar o diretório com `sync_directory_after_commit`: torna durável a
   alteração das entradas do diretório. Se essa sincronização falhar, o commit
   continua visível, mas sua sobrevivência a uma queda de energia não está
   assegurada. O caminho existente de aviso pós-commit é preservado.

A propriedade atômica é **publicação completa condicionada à ausência**, não
uma promessa de durabilidade sem sincronização do diretório. Os testes não
simulam queda de energia. O mecanismo de hard link já era usado pela restauração
da lixeira; não adiciona dependência, syscall explícita, `unsafe` ou nova plataforma.
Se o filesystem recusar hard links, a criação falha sem publicação substitutiva.

### 10.3 Descarte com desktop ativo

O cliente continua usando exclusivamente `authority::perform_at`: lease livre
produz execução local pontual; lease ocupado encaminha pelo socket privado.
O lease continua cobrindo leitura, validação e movimento; não é retido pela TUI.

O fluxo humano anterior era `request_flush` → `commit_trash` → mover arquivo →
persistir estado fechado → fechar/remover janela. Esse fluxo foi extraído para
`NoteItAppClone::commit_discard_after_flush`, compartilhado com o receiver, sem
mudar a ação humana.

No receiver, `discard_note` usa `begin_external_write` para capturar o Markdown
com o editor congelado. `document_over_live_body` reaproveita a composição de
documento já usada nas mutações externas. `validate_discard_base` verifica tanto
essa base viva quanto o documento em disco contra a revision esperada, **antes**
de flush persistente, desarme da captura, movimento ou fechamento. Não existe
`await` entre validação e commit. Com a base validada igual ao disco, não há texto
pendente para gravar: o receiver passa flush bem-sucedido ao ciclo compartilhado.

Em divergência, `ensure_revision_matches` produz `WriteError::RevisionConflict`.
O receiver chama `abort_external_write`, libera a coordenação de ciclo de vida
e devolve o erro. Não fecha a janela, não move a nota, não atualiza o pedido e
não reenvia. Na restauração, a mesma verificação ocorre antes da primitive de
restauração, preservando origem, destino e sidecar em conflito.

Como no contrato existente, o lease coordena writers Note-it. Não é um bloqueio
mandatório contra um processo externo que ignore o protocolo e escreva diretamente
no store. A precondição de ausência da criação é garantida pelo filesystem mesmo
na disputa pelo nome final; não depende da probabilidade do UUID.

### 10.4 Evidência automatizada da R0

- `atomic_file` e `storage`: publicação completa, colisão forçada imediatamente
  antes do hard link, dois publicadores concorrentes e colisão de UUID fixa.
- `tests/concurrent_lifecycle.rs`: criação via authority; descarte/restauração
  válidos byte-idênticos; revisions antigas preservam origem/destino/sidecar;
  restauração válida recusa destino ocupado; lease inacessível não é contornado;
  variantes condicionadas recusam revision ausente, nula ou vazia.
- `r016_protocol_compatibility`: criação v3 não alcança writer v2; receptor v3
  também recusa pedido v2 antes da execução.
- Receiver: texto não salvo e disco alterado são recusados antes do descarte.
- `tests/r0_desktop_discard.rs`: abre desktop real por `scripts/note-it-isolated`,
  confirma WebView carregada, efetua escrita via socket e prova descarte recusado
  mantendo estado/janela, descarte válido fechando ambos e restauração condicionada
  sem reabrir janela. Usa store descartável e barramento privado, sem TUI.

Comando para exigir a prova gráfica (sem permitir skip):

```sh
NOTE_IT_REQUIRE_DESKTOP_TEST=1 cargo test --test r0_desktop_discard -- --nocapture
```

Em CI sem Wayland, somente esse teste gráfico informa skip; os testes de Core,
receiver e protocolo continuam executando. O estágio real de frontend é
`scripts/check frontend-test`, no singular.

Validação local da R0 (2026-09-08), com zero falhas:

| Gate | Passaram | Ignorados preexistentes |
| --- | ---: | ---: |
| `core-tests` | 675 | 1 |
| `cli-tests` | 185 | 0 |
| `mcp-tests` | 226 | 1 |
| `embedding-tests` | 37 | 3 |
| `embed-tests` | 112 | 1 |
| `remote-tests` | 87 | 0 |
| `tui-tests` | 21 (9 PTY + 7 navegação + 5 markdown) | 0 |
| `workspace-tests` | 1477 | 6 |
| `frontend-test` | 1233 (59 arquivos) | 0 |
| Desktop real, execução dedicada | 1 | 0 |

Contagens por execução, não somáveis: o workspace repete testes das crates.
Os ignorados são os benchmarks, a comparação com artefato provisionado e o
smoke test pago de provider real já excluídos das suítes normais.
Os seis boundaries (`core`, `cli`, `mcp`, `embedding`, `embed`, `tui`),
`rust-format`, `ci-parity` e Clippy do workspace com todos os targets/features
também passaram. Foram adicionados 16 testes: 4 de criação, 8 de ciclo de vida
no Core, 1 de compatibilidade v2/v3, 2 do receiver e 1 com desktop real.
Nenhum arquivo de `noteit-tui`, manifesto, lockfile ou boundary foi alterado.

**FASE 5.0D AINDA NÃO INICIADA.**
