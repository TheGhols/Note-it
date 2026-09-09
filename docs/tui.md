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
**5.0D.R0: PASS.** Gates locais e [CI da implementação](https://github.com/TheGhols/Note-it/actions/runs/34210340965)
confirmados verdes para `fc6d39a765ffe00ef038c467a64106aaeec87e06` antes deste
fechamento documental. Naquele momento, a 5.0D funcional dependia de revisão e nova autorização.

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

**Situação no fechamento da R0: FASE 5.0D AINDA NÃO INICIADA.** A revisão humana
posterior autorizou a fase funcional registrada na seção 11.

## 11. Fase 5.0D — Cliente transacional e editor externo

Baseline aprovada: `2fc1eecf1073321de5a18ab3ba956c2e94ff6008`.
O primeiro fechamento, após os gates e CI da implementação, foi invalidado pela
auditoria posterior de restauração do terminal. **5.0D: BLOCKED — regressão de
restauração confirmada**, até validar a revisão corretiva e seu CI remoto.
A seção 10 registra o encerramento histórico da R0, que permanece intocado.
A correção foi autorizada a usar `rustix 1.1.4` diretamente na TUI; nenhum contrato
do Core/R0, comando CLI, tool MCP ou função da GUI muda. A 5.0E não está iniciada.

### 11.1 Teclas e snapshot de leitura

| Contexto | Tecla | Ação |
| --- | --- | --- |
| Notas Recentes, lista | `n` | Criar nota vazia e abrir no Reader, sem prompt de título |
| Reader ativo | `Space` | Alternar a tarefa sob o cursor de leitura |
| Reader ativo | `d` | Confirmação inline `Mover para a lixeira? [y/N]` |
| Confirmação | `y` | Descartar com a revision que estava exibida antes da confirmação |
| Confirmação | `n`, `Esc`, `Enter` | Cancelar sem escrever |
| Lixeira, lista ou Reader | `r` | Restaurar o item exibido, sem confirmação |
| Reader ativo | `e` | Abrir o corpo Markdown em `$EDITOR` |

Setas/`j`/`k` movem o cursor por linhas de origem; `PgUp`/`PgDn` movem dez
linhas e `Home`/`g` voltam ao início. A linha selecionada tem fundo destacado.
Ao avançar, ela fica no topo do corpo visível; wrapping continua no Ratatui.
O renderizador anterior somente ganhou posições de origem, sem mudar seus
rótulos, parsing ou saída Markdown. O scanner `task::parse_tasks` do Core decide
se a linha é realmente uma tarefa e fornece seu `task_ref`; texto dentro de
fences não vira tarefa por semelhança visual.

`LoadedDocument` centraliza `{ id, document, revision, in_trash }`. A revision
vem de `write::revision_of` sobre o próprio documento retornado por `read_note`
ou `read_trash_note`. Conteúdo e revision são capturados juntos, não em widgets.
O `current_note_id` anterior permanece apenas como projeção para navegação.
O preview da lixeira usa o documento canônico carregado, não uma revision obtida
silenciosamente no momento de restaurar.

### 11.2 Funil de escrita e conflitos

| Ação | Operação passada a `authority::perform_at` | Precondição |
| --- | --- | --- |
| Toggle | `MutateNote` com `CompleteTask` ou `ReopenTask` | `Some(snapshot.revision)` |
| Criar | `CreateNote { draft }`, corpo vazio | Destino ausente, contrato atômico da R0 |
| Descartar | `DiscardNote` | Revision do snapshot confirmado |
| Restaurar | `RestoreFromTrashAtRevision` | Revision do snapshot da lixeira |
| Editor | `MutateNote` com `ReplaceBody` ou `ClearBody` | `Some(original_revision)`, anterior ao editor |

Nenhum desses caminhos grava no store diretamente ou mantém lease em repouso.
O `NoteItCore` da navegação permanece somente-leitura. `authority::perform_at`
escolhe lease local pontual ou encaminhamento ao desktop pelo socket privado v3;
a TUI não implementa cliente IPC nem ciclo alternativo de descarte.

Em `WriteError::RevisionConflict`, toggle/descarte/restauração mostram conflito,
recarregam conteúdo/lista e revision, sem aplicar sucesso local ou reenviar.
Criação não inventa revision para inexistência: um erro é mostrado, sem retry.
Após sucesso, a TUI relê a nota ou a lista pelo Core. No editor, a proteção dos
bytes recusados acontece **antes** de reler para exibição. Nenhuma leitura de
retorno é usada para substituir a precondição original e fazer uma escrita passar.

### 11.3 Editor, terminal e seleção de mutação

`TerminalGuard::suspend()` usa `restore()`. Na revisão corretiva da seção 12,
`resume()` primeiro restaura o snapshot original e somente depois reativa
raw/alternate screen e o cursor oculto. Construção, suspensão, Drop e panic hook
compartilham o cleanup do terminal. Ao retornar, um resize fullscreen do Ratatui invalida seu
buffer anterior e redesenha a tela, mesmo se as dimensões não mudaram.
`signal-hook` continua marcando a mesma flag; durante o editor ela é consultada
a cada 50 ms. Interrupção encerra/recolhe o filho, preserva edições significativas
no temporário e informa seu caminho também na saída do terminal.

O programa é resolvido de `$EDITOR`, com fallback **`vi`** quando ausente/vazio.
O valor é um nome/caminho de executável literal, podendo conter espaços; não é
avaliado por shell. Para argumentos, configure um script wrapper como `$EDITOR`.
`std::process::Command` recebe o caminho temporário como argumento separado.
A execução do editor usa apenas `std`; a correção da restauração do terminal
adiciona a dependência direta autorizada na seção 12.

O temporário exclusivo é `${TMPDIR:-/tmp}/noteit-<note-id>-<uuid>.md`, criado
com permissão `0600`, conteúdo bruto do corpo e `sync_all` antes do spawn.
Não contém o front matter do arquivo de armazenamento. A saída permanece em
bytes separados da visão UTF-8 usada para escolher a operação:

1. Bytes idênticos: no-op, nenhuma escrita/revision/recovery.
2. Bytes diferentes mas `canonical_content` igual: canonical no-op, mesmas garantias.
3. Forma canônica diferente e não vazia: `ReplaceBody { body: edited }`.
4. Forma canônica esvaziada: `ClearBody`, sem confirmação adicional.

**O `$EDITOR` preserva integralmente o conteúdo significativo segundo o modelo
canônico do Note-it. Terminadores `\n`/`\r` no fim do corpo não fazem parte da
representação canônica persistida e, isoladamente, não avançam revision. Em
conflito, o recovery preserva literalmente a saída do editor antes dessa canonização.**

Espaços finais continuam significativos. Um corpo já vazio seguido apenas de
terminadores é no-op; esvaziar um corpo não vazio usa `ClearBody`, não uma
tentativa de `ReplaceBody` seguida de tratamento de `InvalidInput`.
Exit status não-zero, falha do processo ou saída não UTF-8 não são persistidos;
edições significativas ficam no temporário com mensagem/caminho. Não há
conversão UTF-8 com perdas. Erro de retomada também preserva o temporário.

### 11.4 Recovery literal e política de retenção

Em conflito, criar exclusivamente:

```text
${XDG_STATE_HOME:-$HOME/.local/state}/note-it/tui-recovery/<note-id>-<uuid>.md
```

Arquivo `0600`, novos diretórios `0700`, UUID e `create_new` contra colisão;
sem sobrescrever recovery existente. Os bytes são exatamente os devolvidos
pelo editor, inclusive terminadores e arquivo de **zero bytes** quando vazio.
Não há front matter adicional, serialização, merge, canonização ou retry.
Sincroniza arquivo e diretório antes de apagar o temporário original.

Após sucesso, no-op ou recovery confirmado, remove o temporário. Se recovery
falhar, mantém o temporário e informa tanto o erro quanto seu caminho completo.
Outras falhas de escrita mantêm essa última cópia e não afirmam sucesso.
Falha de remoção pós-sucesso gera aviso, não perde o texto. Recovery e temporários
retidos não têm coleta automática nesta fase; o usuário decide quando removê-los.
Esses arquivos de trabalho são a exceção explicitamente autorizada ao I/O local:
não pertencem ao store e nunca substituem a escrita via authority.

### 11.5 Provas e gates

Testes novos em `noteit-tui/tests/transactions.rs`, `editor_process.rs` e
`desktop_concurrency.rs`. Os 9 PTY e 7 testes de navegação anteriores permanecem
byte-idênticos; os cinco testes Markdown conservam nomes e lógica.
O teste gráfico dedicado exige os binários reais e o harness inalterado:

```sh
cargo build --bin note-it
NOTE_IT_REQUIRE_TUI_DESKTOP_TEST=1 cargo test -p noteit-tui --test desktop_concurrency -- --nocapture
```

Em ambiente sem Wayland, o teste informa ausência de prova gráfica; com a variável
acima, ausência de display é falha. A execução dedicada local deve comprovar:
R1 recusada após R2 do desktop, R2 preservada/recarregada sem retry; edição válida
aplicado exatamente uma vez pelo receiver; lease liberado pela TUI ainda aberta
e desktop capaz de adquirir authority e continuar respondendo.

Gates locais finais (2026-09-08):

| Gate | Passaram | Ignorados preexistentes |
| --- | ---: | ---: |
| `core-tests` | 675 | 1 |
| `cli-tests` | 185 | 0 |
| `mcp-tests` | 226 | 1 |
| `embedding-tests` | 37 | 3 |
| `embed-tests` | 112 | 1 |
| `remote-tests` | 87 | 0 |
| `tui-tests` | 43 | 0 |
| `workspace-tests` | 1499 | 6 |
| `frontend-test` | 1233 (59 arquivos) | 0 |
| Desktop real, execução dedicada obrigatória | 1 | 0 |

Contagens por execução, não somáveis. Dos 43 resultados TUI no gate sem display,
42 executam as provas headless/PTY e o teste gráfico retorna explicitamente sem
prova gráfica; a execução dedicada acima exige Wayland e não aceita esse retorno.
O workspace local também executou o caso gráfico. São 22 testes novos: 18 de
transações/editor, 3 com processo real/PTY e 1 com os dois binários reais.
As tabelas de casos nos testes incluem os no-ops literal/canônico, espaços finais,
`ClearBody`, recovery vazio/terminadores, falha de recovery, editor não-zero,
UTF-8 inválido e writer inacessível sem bypass.

Também passaram os seis boundaries, `ci-parity`, `rust-format`, `rust-check`,
`rust-clippy` (workspace, todos os targets/features, `-D warnings`), `frontend-lint`
e `frontend-build`. O build frontend conserva o aviso de chunk maior que 500 kB.
`scripts/check-tui-boundary`, `scripts/note-it-isolated`, Core/R0, GUI, CLI, MCP,
manifestos e lockfiles não foram alterados. Os fingerprints dos dados/configuração/
estado reais (nomes, conteúdo, tamanho e mtime) permaneceram iguais.

Saída literal da execução dedicada final, além de três repetições consecutivas
verdes durante a verificação:

```text
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.24s
     Running tests/desktop_concurrency.rs (target/debug/deps/desktop_concurrency-2c1baf8c912c4380)

running 1 test
C: real TUI mutated locally; lease released while TUI remained alive
COMMAND: /home/guhols/Projetos/Note-It/scripts/note-it-isolated --root /tmp/noteit-5d-YXPZP5 -- (NOTE_IT_BINARY=/home/guhols/Projetos/Note-It/target/debug/note-it)
COMMAND: scripts/note-it-isolated --root /tmp/noteit-5d-YXPZP5 --verify
note-it-isolated: io.github.theghols.NoteIt is on the private bus for /tmp/noteit-5d-YXPZP5
DBUS_SESSION_BUS_ADDRESS=unix:path=/tmp/note-it-bus-8xSX6Y/dbus-LvOt2X1T8n,guid=7bd7e2d3f992b122541408a06aa08618

C: desktop acquired authority and mapped its window with the same TUI still alive
A: TUI sent its displayed R1; RevisionConflict; R2 bytes intact; TUI displayed R2; zero retry commits
B: valid TUI editor mutation committed exactly once by desktop receiver; private socket v3; desktop lease remained held
C: desktop accepted and confirmed its next mutation after TUI; no perpetual lease or permanent GUI block
test real_tui_and_isolated_desktop_enforce_revision_socket_and_pointwise_lease ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.59s
```

Fechamento documental após o [CI da implementação concluído com sucesso](https://github.com/TheGhols/Note-it/actions/runs/34284207606)
para `0ce2825262f28c40c8763f83b702a1368899dd37`, com ambos os jobs aprovados.
Esse fechamento foi prematuro: `00140b7c95565d1424a678282c068bc25c8a460a`
registrou conclusão antes de detectar que um editor deixando `stty raw -echo`
contaminava a referência de restauração do Crossterm. O CI anterior não cobria
esse cenário. **5.0D: BLOCKED**, aguardando prova e CI da correção.
**Fase 5.0E não iniciada.**

## 12. Revisão corretiva da 5.0D — ownership do terminal

### 12.1 Histórico e defeito

Baseline corretiva: `00140b7c95565d1424a678282c068bc25c8a460a`, confirmada
como `HEAD == origin/main`, com árvore limpa antes das alterações. A implementação
original `0ce2825262f28c40c8763f83b702a1368899dd37` e seu fechamento documental
tinham CI verde, mas os testes verificavam somente editores que restauravam o
terminal. A auditoria invalidou o fechamento: **BLOCKED — regressão confirmada**.

O Crossterm descarta sua referência de termios ao desativar raw mode. Depois de
um editor executar `stty raw -echo`, `resume()` guardava esse estado contaminado
como nova referência. Ao sair, restaurava o estado do editor, não o estado T0
anterior à primeira entrada da TUI. Não era uma falha do store nem da R0.

### 12.2 Invariante e dependência

`TerminalGuard` possui um `Arc<OriginalTerminal>` com descriptor **owned** e
snapshot `Termios` privados e imutáveis. Captura com `tcgetattr` antes de qualquer
raw mode. A seleção repete a função `tty_fd()` do Crossterm **0.29.0**, tanto no
backend libc quanto no rustix: stdin quando `isatty(stdin)`, senão abertura
read/write de `/dev/tty`. Não presume que stdout seja o terminal de termios.
O descriptor duplicado mantém uma referência ao mesmo objeto de terminal.

`rustix = { version = "=1.1.4", features = ["termios"] }` é dependência direta
somente de `noteit-tui`, com a feature padrão `std`. `cargo tree -p noteit-tui -i
rustix` confirma uma única versão **1.1.4**, já usada por Crossterm e tempfile.
O lockfile só acrescenta `"rustix"` à lista de dependências da TUI: nenhum pacote
ou versão novo. Nenhum `unsafe` novo, outra crate POSIX ou alteração no Core/R0.

Invariante: **T0 é a única autoridade de restauração durante toda a vida do guard;
nenhuma saída de editor redefine T0.**

- `suspend()`: tenta sair da alternate screen, mostrar cursor, desativar mouse,
  desativar raw do Crossterm e, por último, aplicar T0 com `tcsetattr(..., Now)`.
- `resume()`: executa essa normalização antes de permitir que Crossterm capture
  novamente sua referência e habilite raw/alternate screen.
- `restore()`/Drop: fazem a mesma restauração, mesmo se o guard estava inativo;
  um editor pode ter modificado o terminal enquanto suspenso.
- Todas as operações de cleanup são avaliadas mesmo se uma falhar. O primeiro
  erro é retornado, mas nunca impede a tentativa final de aplicar T0.
- Estados `Inactive`, `Active` e `Uncertain` distinguem transições completas de
  falhas parciais. Antes de uma transição o estado fica `Uncertain`; só muda ao
  completar todas as operações. Nova tentativa não vira falso sucesso por um
  `active = true` prematuro. Falha na construção também passa pelo Drop.

O panic hook obtém o mesmo snapshot por `Mutex<Weak<OriginalTerminal>>`, faz
upgrade para `Arc`, solta o mutex e restaura antes de chamar o hook anterior.
Não há I/O nem callback enquanto o mutex está travado, `static mut` ou sincronização
unsafe. Não existe segundo snapshot mutável. Um segundo guard simultâneo é recusado;
ao terminar a ownership, o `Weak` não mantém o terminal vivo nem contamina uma
futura inicialização.

### 12.3 Regressão independente e falhas parciais

Antes da correção, foi compilado e preservado o binário real da baseline em
`/tmp/noteit-5d-terminal-evidence-p4m9O3/noteit-tui-baseline`.
SHA-256 do executável:
`4ea8e8d2025b2ae1751f4d0d179e1f0f3fed84dfa1a25dfedda835b906f01131`.
O mesmo teste seleciona opcionalmente esse executável arquivado; por padrão usa
o binário real compilado pelo Cargo. Nenhum código da TUI simulado no teste.

```sh
NOTEIT_TUI_REGRESSION_BINARY=/tmp/noteit-5d-terminal-evidence-p4m9O3/noteit-tui-baseline cargo test -p noteit-tui --test editor_process real_tui_restores_exact_termios_after_editor_leaves_raw_no_echo -- --nocapture
```

Resultado pré-correção, exit code **101**:

```text
test real_tui_restores_exact_termios_after_editor_leaves_raw_no_echo ... FAILED
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.13s
```

A fixture cria store descartável via authority, usa XDG/TMP privados e remove o
barramento herdado. O harness captura termios do slave PTY **antes do spawn**,
abre a nota com Enter, aciona `e`, espera o editor salvar e deixar `stty raw -echo`,
e sai com `q`. Compara antes de qualquer limpeza: o Drop do harness só recolhe
o filho, não restaura termios. O snapshot compara a representação Debug completa
do rustix, incluindo bits desconhecidos, disciplina de linha, todos os caracteres
de controle e velocidades; `Termios` não implementa `PartialEq`.

| Campo | T0 antes | Baseline depois | Correção depois |
| --- | --- | --- | --- |
| Input | `ICRNL \| IXON` | `0x0` | `ICRNL \| IXON` |
| Output | `OPOST \| ONLCR` | `ONLCR` | `OPOST \| ONLCR` |
| Local | `ECHOCTL \| ECHOKE \| ISIG \| ICANON \| ECHO \| ECHOE \| ECHOK \| IEXTEN` | sem `ISIG`, `ICANON`, `ECHO` | exatamente T0 |
| Control | `CSIZE \| CREAD \| 0xf` | igual | igual |
| Disciplina e caracteres | snapshot completo do PTY | iguais | iguais |
| Velocidades entrada/saída | `38400 / 38400` | iguais | iguais |

Após a correção, a mesma regressão passa com todos os campos exatamente iguais.
Cinco testes adicionais em `editor_process.rs` cobrem:

1. Defeito literal no binário real (`stty raw -echo`).
2. Três ciclos de editor, exits **7, 0, 8**, também corrompendo INTR, ERASE,
   VMIN, VTIME e velocidade para 19200. Cada novo editor recebe T0 exato;
   após retorno a TUI está raw e após saída restaura T0.
3. Binário real com controlling TTY criado por `setsid --ctty --wait` e stdin
   redirecionado para `/dev/null`: valida o caminho real de `/dev/tty`.
4. Panic após editor contaminado, antes **e** depois de `resume()`. Subprocessos
   do próprio teste exercitam guard/hook de produção, sem switches novos no
   aplicativo. O hook anterior mede T0 antes de imprimir; não basta o Drop.
5. Stdout redirecionado, após o sizing inicial quando aplicável, para um socket
   local cujo peer já foi fechado: falhas determinísticas em inicialização,
   suspensão, retomada e Drop não pulam a restauração canônica. Segunda tentativa
   de resume continua retornando erro, não falso sucesso. Inicialização recusada
   não deixa registro de guard vivo.

As três provas de editor anteriores passam sem enfraquecimento; o helper agora
também compara termios completo na saída normal. Os 9 PTY da 5.0B, 7 testes de
navegação da 5.0C e o arquivo Markdown permanecem byte-idênticos à baseline.

Saída da suíte dedicada:

```text
PANIC: before-resume: canonical termios verified inside previous hook, before diagnostics and Drop
PANIC: after-resume: canonical termios verified inside previous hook, before diagnostics and Drop
OUTPUT FAILURE: initialize: canonical restoration survived BrokenPipe
OUTPUT FAILURE: suspend: canonical restoration survived BrokenPipe
OUTPUT FAILURE: resume: canonical restoration survived BrokenPipe
OUTPUT FAILURE: drop: canonical restoration survived BrokenPipe
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s
```

### 12.4 Validação corretiva

`scripts/check all` terminou com exit code **0**:

```text
scripts/check: all — tudo passou.
```

| Gate | Passaram | Ignorados preexistentes |
| --- | ---: | ---: |
| `core-tests` | 675 | 1 |
| `cli-tests` | 185 | 0 |
| `mcp-tests` | 226 | 1 |
| `embedding-tests` | 37 | 3 |
| `embed-tests` | 112 | 1 |
| `remote-tests` | 87 | 0 |
| `tui-tests` | 48 | 0 |
| `workspace-tests` | 1504 | 6 |
| `frontend-test` | 1233 (59 arquivos) | 0 |
| TUI ↔ desktop dedicado, display obrigatório | 1 | 0 |

`tui-tests` é headless: seus 48 resultados incluem o retorno explícito do teste
gráfico sem Wayland (47 testes efetivamente executados ali). Isso **não** é prova
gráfica; ela foi executada separadamente com `NOTE_IT_REQUIRE_TUI_DESKTOP_TEST=1`,
que falha sem display. Os testes do workspace também passaram na sessão gráfica.

Passaram ainda `ci-parity`, `rust-format`, `rust-check`, `rust-clippy`
(`--workspace --all-targets --all-features -- -D warnings`), os seis boundary gates
Core/CLI/MCP/embedding/embed/TUI, `frontend-install`, `frontend-lint` e
`frontend-build`. Nenhum gate ou warning foi suprimido. O build frontend mantém
o aviso preexistente de chunk acima de 500 kB.

A prova dedicada usou `cargo build --bin note-it` seguido de
`NOTE_IT_REQUIRE_TUI_DESKTOP_TEST=1 cargo test -p noteit-tui --test desktop_concurrency -- --nocapture`.
Trechos literais da execução corretiva:

```text
C: real TUI mutated locally; lease released while TUI remained alive
COMMAND: /home/guhols/Projetos/Note-It/scripts/note-it-isolated --root /tmp/noteit-5d-NZo5i2 -- (NOTE_IT_BINARY=/home/guhols/Projetos/Note-It/target/debug/note-it)
COMMAND: scripts/note-it-isolated --root /tmp/noteit-5d-NZo5i2 --verify
note-it-isolated: io.github.theghols.NoteIt is on the private bus for /tmp/noteit-5d-NZo5i2
C: desktop acquired authority and mapped its window with the same TUI still alive
A: TUI sent its displayed R1; RevisionConflict; R2 bytes intact; TUI displayed R2; zero retry commits
B: valid TUI editor mutation committed exactly once by desktop receiver; private socket v3; desktop lease remained held
C: desktop accepted and confirmed its next mutation after TUI; no perpetual lease or permanent GUI block
test real_tui_and_isolated_desktop_enforce_revision_socket_and_pointwise_lease ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.12s
```

O termios completo antes/depois também coincidiu nessa execução. Logs locais
preservados em `/tmp/noteit-5d-terminal-evidence-p4m9O3/`: `pre-fix.log`,
`pre-fix-final-harness.log`, `post-fix.log`, `editor-regressions.log`,
`all-gates.log`, `desktop-concurrency.log` e `rustix-features.log`.
Os fingerprints de conteúdo e de nomes/tipos/tamanhos/mtimes dos três diretórios
pessoais (`share`, `config`, `state` de `note-it`) permaneceram respectivamente
`98a859f2446e3bd2815c3e965bc50acf2366b4690f90ca6f06d8f8402a4b4db3` e
`a5a3809aece23c816463eedcd2d6240b0ab97c0712befc72cc1538ddc3429e1f`.

O primeiro CI corretivo, run
[`34324317979`](https://github.com/TheGhols/Note-it/actions/runs/34324317979),
validou as sete provas de terminal anteriores, mas expôs uma fragilidade no teste
de falha parcial: o subprocesso começava com stdout em pipe e podia bloquear no
sizing de `Terminal::new()` antes de criar seu marcador. A produção não falhou.
O teste foi tornado determinístico inicializando no PTY e redirecionando stdout
depois para um socket local cujo peer já estava fechado. A operação sob teste
continua recebendo `BrokenPipe`, sem depender de timing ou tamanho de terminal.

A revisão corretiva final de implementação/testes é
`7196a2f0f0007d01967d02a8d7f5795822c3200f`. Seu
[CI `34348671971`](https://github.com/TheGhols/Note-it/actions/runs/34348671971)
terminou `completed/success`, com os jobs Rust e frontend aprovados. Isso fecha
explicitamente o estado BLOCKED descoberto pela auditoria, sem apagar o primeiro
fechamento prematuro nem a falha de harness intermediária.

**5.0D: PASS — defeito de restauração do terminal corrigido e provado
independentemente. Fase 5.0E não iniciada.**
