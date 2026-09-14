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

1. **Editor de texto completo embutido no terminal com emulação Vim/Emacs:** *(A recusa do editor embutido foi revogada pelo roadmap na Fase 5.0D.2, que entrega um editor v1 não-modal, sem realce de sintaxe e com desfazer de limite explícito — ver seção 14. Modos, realce e histórico ilimitado continuam fora.)*
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
Renderiza documentos Markdown para linhas estilizadas do Ratatui (`Line<'static>`). O registro abaixo descreve a 5.0C; a varredura inline foi reescrita na Fase 5.0D.1 e vive hoje em `inline.rs` — ver seção 13:
- **Títulos (Headings):** Níveis 1 a 6 (`# ` a `###### `) estilizados com distinção visual de negrito, itálico e cores hierárquicas (amarelo, ciano, verde, branco, cinza). *(Substituído na Fase 5.0D.1: os seis níveis passaram a ter cores próprias em `Color::Rgb` — ver seção 13.5.)*
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

## 13. Fase 5.0D.1 — Fidelidade de Markdown e compatibilidade com notas da GUI

### 13.1 Defeito: o leitor mostrava o arquivo, não a nota

Uma nota é Markdown, e Markdown diz duas coisas ao mesmo tempo: as palavras e
como elas estão vestidas. O leitor da 5.0C mostrava as duas. O diagnóstico
visual, feito sobre notas reais escritas pelo editor gráfico, encontrou sete
vazamentos e uma hierarquia incompleta:

| O que aparecia na tela | Causa no renderer da 5.0C |
| --- | --- |
| `<span data-note-it-color="#DC2626" style="color:#DC2626">` | `parse_inlines` não reconhecia HTML nenhum; `<` era um caractere comum. |
| `<mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A">` | idem. |
| `<u>…</u>` | idem — não havia sublinhado no vocabulário do renderer. |
| `<!-- comentário -->` | comentários só eram removidos dentro de tarefas, por `extract_completed_at`. |
| `&nbsp;` | não havia decodificação de entidades. |
| `~~riscado~~` | riscado não era um delimitador reconhecido. |
| `*negrito com italico*` sobrando de `***…***` | negrito e itálico eram dois `find` independentes: o de `**` consumia dois asteriscos de três e deixava o par restante na tela. |
| `**tarefa em negrito**` no painel de tarefas | `ui.rs` desenhava `task.text` — Markdown cru — como um `Span` literal. |
| H4 branco, H5 cinza, H6 cinza escuro | a paleta tinha três cores e depois acabava. |

O parser anterior era uma sequência de `find` sobre um `Vec<char>`: sem pilha,
sem noção de aninhamento e sem regra de pareamento. Era por isso que `***x***`
quebrava, que um `<span>` dentro de um `<mark>` não tinha como existir, e que
`3 * 4 * 5` corria o risco de virar itálico. Ampliar aquele desenho com mais
substituições globais teria multiplicado os casos sem resolver a composição, que
é o requisito central: cor, marca-texto e ênfase precisam conviver.

### 13.2 Arquitetura: reconhecimento de bloco separado da varredura inline

A correção separa as duas responsabilidades em dois módulos do mesmo diretório —
`noteit-tui/src/*.rs`, portanto ambos continuam sob a varredura de
`scripts/check-tui-boundary`:

- **`markdown.rs` — reconhecimento de bloco.** Decide o *tipo* de cada linha
  armazenada (cerca, comentário de bloco, citação, alerta, título, régua,
  tarefa, lista, lista numerada, parágrafo) e a moldura em que ela é desenhada.
  Não interpreta nada dentro da linha.
- **`inline.rs` — projeção inline.** Uma varredura por linha, com três
  colaboradores explícitos: um **scanner de tokens**, que reconhece exatamente
  as construções que o serializador do próprio Note-it escreve; uma **pilha de
  tags**, para que `</span>` restaure o estilo que estava aberto quando o
  `<span>` abriu, e não o estilo nenhum; e um **emissor**, que agrupa trechos de
  estilo igual em um único `Span` e é o único ponto por onde texto vira
  apresentação — razão pela qual é também o único ponto onde controles de
  terminal são removidos.

A ênfase recorre (um delimitador só é delimitador se tiver par, então sua
extensão é conhecida antes do conteúdo ser lido) e é limitada a 32 níveis; as
tags não recorrem, então uma nota com duzentos `<span>` aninhados custa duzentas
entradas de `Vec` e nenhuma pilha de chamadas.

As regras de pareamento de ênfase, escape, code span, entidade e link são
deliberadamente as mesmas de `noteit_core::visible_text`. As duas projeções
respondem perguntas diferentes — "quais são as palavras?" para rótulo e busca,
"quais palavras e com que estilo?" para o leitor — e ficariam incoerentes se
discordassem sobre o que era um delimitador. É por isso que `= 100 * 2.5`,
`3 * 4 * 5`, `note_it_config` e `~/Downloads` continuam intactos nos dois lados.

**Nenhuma dependência foi adicionada.** `Cargo.lock` permaneceu byte-idêntico e
`cargo tree -p noteit-tui` não mudou.

### 13.3 Subconjunto HTML admitido

Três elementos, e nada mais:

| Forma armazenada | Efeito no leitor |
| --- | --- |
| `<span data-note-it-color="#RRGGBB">` | `Style::fg(Color::Rgb(..))` |
| `<span style="color:#RRGGBB">` | idem, como alternativa quando o atributo canônico falta ou é inválido |
| `<span data-note-it-font-size="NN">` | abre e fecha sem efeito — um terminal não tem corpo de fonte; o texto permanece |
| `<mark data-note-it-highlight="#RRGGBB">` | `Style::bg(..)` mais `fg(#1E293B)` |
| `<mark style="background-color:#RRGGBB">` | idem |
| `<mark>` sem atributo | destaque com o primeiro amarelo da paleta (`#FDE68A`) — a grafia das notas anteriores ao atributo |
| `<u>` … `</u>` | `Modifier::UNDERLINED` |
| `<!-- … -->` | removido inteiro, inclusive `note-it:completed_at` |
| `&amp;` `&lt;` `&gt;` `&quot;` `&apos;` `&nbsp;` e referências numéricas | decodificados **uma vez** |

O atributo canônico `data-note-it-*` tem precedência; o `style` que o mesmo
serializador escreve ao lado é aceito como alternativa, lido **por nome exato de
propriedade** e validado como cor. Não há interpretação de CSS: `position`,
`z-index`, `expression(…)` ou qualquer outra declaração ao lado é ignorada, não
avaliada. Cor é `#RGB` ou `#RRGGBB` e mais nada — `red`, `rgb(…)`, `var(…)` e
`url(javascript:…)` não são cores que o Note-it grava, portanto não são cores
que o leitor lê.

O `#1E293B` sobre o destaque não é invenção da TUI: é o `HIGHLIGHT_TEXT_COLOR` de
`ui/src/ui/palettes.ts`, que o editor gráfico escreve no mesmo `style` inline do
fundo porque os destaques são pálidos de propósito e o texto claro da nota
sumiria neles. O leitor faz o mesmo pelo mesmo motivo, e o resultado é a mesma
cascata: `<span cor><mark>` fica com a cor do destaque no texto, `<mark><span
cor>` deixa a cor interna vencer. Ambos os casos preservam o fundo.

### 13.4 Regras de segurança

- **HTML desconhecido é removido, nunca executado nem impresso.** Uma tag bem
  formada de um elemento que o Note-it não escreve — `<script>`, `<div>`,
  `<img>`, `<b>` — desaparece; o que ela envolvia permanece como texto. Um `<`
  que não abre tag nenhuma (`um < dois`, `<https://exemplo.com>`) continua o
  caractere que é.
- **Atributos jamais são executados.** `onclick`, `onerror`, `src` e afins não
  são lidos: só quatro nomes de atributo têm significado, e o resto do elemento
  é atravessado apenas para descobrir onde a tag termina — com consciência de
  aspas, para que um `>` citado não a encerre cedo demais.
- **Nenhuma URL é resolvida.** Um link mostra suas palavras sublinhadas; o
  destino não vai à tela e nada o segue. A renderização não abre rede, arquivo
  nem subprocesso: recebe `&str` e devolve `Span`.
- **Controles de terminal ficam inertes.** As faixas C0 e C1
  (`U+0000`–`U+001F`, `U+007F`–`U+009F`) não sobrevivem à passagem pelo emissor,
  inclusive quando chegam por referência numérica (`&#27;`); a tabulação vira os
  espaços que representa em vez de mover o cursor. Um `ESC [ 3 1 m` armazenado
  são cinco caracteres do arquivo de alguém, nunca uma instrução para este
  terminal.
- **Fechamento fora de ordem fecha o que dá.** `</span>` fecha o `<span>` mais
  interno e tudo aberto dentro dele; um fechamento sem nada a fechar é
  descartado, jamais impresso. Uma tag aberta e nunca fechada termina no limite
  da construção que a contém.
- **Nada some em silêncio.** Um delimitador sem par continua o caractere que é.
  Um `<!--` que ninguém fechou não é um comentário: apenas os quatro caracteres
  saem, e as palavras depois deles sobrevivem em vez de serem engolidas até o
  fim do arquivo. A busca pelo `-->` de um comentário de bloco para na primeira
  cerca de código, porque um `-->` dentro de um bloco pertence ao código.
- **Dentro de uma cerca nada é interpretado.** Todo caractere é o caractere que
  alguém digitou; só a inertização se aplica.

### 13.5 Hierarquia H1–H6

Seis níveis, seis cores próprias, todas em `Color::Rgb`:

| Nível | Cor | Modificadores |
| --- | --- | --- |
| H1 | `#FFCC66` | negrito + sublinhado |
| H2 | `#5FD3F3` | negrito |
| H3 | `#7FD98C` | negrito |
| H4 | `#E8975A` | negrito |
| H5 | `#C39BF0` | negrito |
| H6 | `#93A7C4` | negrito + itálico |

A cor nunca é o único portador da identidade: o marcador `#`…`######` continua
na tela e os modificadores diferem nas duas pontas, então um terminal de paleta
pobre — que aproxima o RGB para a entrada mais próxima que tiver — ainda mostra
seis níveis distinguíveis. A semântica textual e o nível estrutural do título
não mudaram.

### 13.6 Superfícies corrigidas fora do leitor

Duas telas exibiam Markdown cru pelo mesmo motivo e foram corrigidas com o mesmo
renderer:

- **Painel de Tarefas Pendentes.** `TaskEntry::text` é uma linha da nota, e agora
  é lida como uma: o painel mostra `tarefa em negrito`, nunca
  `**tarefa em negrito**`.
- **Prévia da lixeira.** Mostrava `note.content` inteiro dentro de um único
  `Span`; passou a usar `markdown::render_markdown`, porque uma nota na lixeira
  continua uma nota.

Rótulos de nota e trechos de busca já passavam por `noteit_core::visible_text` e
não precisaram de nada.

### 13.7 Provas

**Regressão primeiro.** `noteit-tui/tests/markdown_fidelity.rs` foi escrito
contra a baseline `32cbda6` e falhou em **24 dos 27** testes iniciais, cada um
pela razão esperada — os três que passavam já eram comportamentos corretos
(cerca literal, ênfase dentro de título sem vazamento, metadados de tarefa
ocultos). Depois da correção o arquivo cresceu para **30 testes** e passa
inteiro.

Contagens finais, `cargo test -p noteit-tui`:

| Alvo | Testes |
| --- | --- |
| unitários de `src/lib.rs` (`inline.rs` + `markdown.rs`) | 12 |
| `tests/markdown_fidelity.rs` | 30 |
| `tests/navigation_and_rendering.rs` | 7 |
| `tests/transactions.rs` | 18 |
| `tests/terminal_lifecycle.rs` | 9 |
| `tests/editor_process.rs` | 8 |
| `tests/desktop_concurrency.rs` | 1 |
| **total** | **85** |

O que é provado, e como:

- **Estilo, não só texto.** As asserções de cor, fundo e modificador inspecionam
  o `Style` dos `Span`s e as células do `TestBackend` (`cell.fg`, `cell.bg`,
  `cell.modifier`), nunca apenas `Line::to_string()`.
- **Títulos.** Os seis níveis têm `fg` mutuamente distintos e H1 difere de H6
  também em modificador; cor e marca-texto são exercitados **em cada um dos seis
  níveis**.
- **Composição.** Fechamento restaurando o estilo anterior; `span` contendo
  `mark`; `mark` contendo `span`; cor combinada com negrito, itálico, riscado,
  sublinhado e código.
- **Blocos.** Parágrafo, lista, lista numerada, tarefa pendente, tarefa
  concluída, citação e os cinco alertas GFM, todos com cor e ênfase dentro, mais
  o bloco cercado permanecendo literal.
- **Entradas hostis.** Tag não fechada, fechamento órfão, fechamento fora de
  ordem, cor inválida, atributo inesperado, `style` com propriedades extras,
  `url(javascript:…)`, `<script>`, atributo de evento, `<img onerror>`,
  sequências ANSI/OSC/C1, entidade numérica que decodifica para `ESC`, Unicode
  combinado, emoji com ZWJ, 20 000 caracteres e 200 spans aninhados. Nenhuma
  causa panic, execução ou desaparecimento de texto.
- **Cursor.** Um comentário de bloco continua ocupando as linhas que armazena —
  em branco, nunca ausentes — para que a numeração que o cursor de leitura
  endereça continue honesta.
- **Buffer desenhado.** Três testes `TestBackend` provam que o buffer final
  contém as palavras e não contém `<span`, `<mark`, `<u>`, `<!--`, `-->`,
  `&nbsp;`, `data-note-it-*`, `background-color`, `note-it:completed_at`, `**`
  nem `~~`.

**Prova em terminal real.** O binário foi executado em pseudoterminal
(`script -qc`, 150×45, `TERM=xterm-256color`) contra um store descartável, com
`DISPLAY`, `WAYLAND_DISPLAY` e `DBUS_SESSION_BUS_ADDRESS` removidos do ambiente e
os quatro `XDG_*` apontados para o diretório temporário. O fluxo ANSI capturado
mostra `38;2;255;204;102` … `38;2;147;167;196` para os seis títulos,
`48;2;253;230;138` com `38;2;30;41;59` no marca-texto, `\e[1m`, `\e[1m\e[3m`,
`\e[9m` e `\e[4m` nas ênfases, `U+00A0` no lugar de `&nbsp;` — e nenhuma das
grafias de armazenamento.

**Varredura sobre as formas reais.** Um teste descartável renderizou as 42 notas
do store do usuário em modo estritamente somente-leitura — 502 linhas
apresentadas, **zero vazamentos** fora de blocos cercados de código, onde a
marcação literal é o comportamento correto. O arquivo foi removido depois da
verificação e nenhum conteúdo pessoal foi copiado para o repositório: as
fixturas versionadas são sintéticas e reproduzem apenas as *formas* observadas.

### 13.8 Gates executados

| Comando | Resultado |
| --- | --- |
| `cargo test -p noteit-tui` (sem display e sem barramento) | 85 testes, todos aprovados |
| `cargo fmt -p noteit-tui -- --check` | limpo (formatados apenas os arquivos alterados) |
| `cargo check --workspace` | aprovado |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | aprovado, ~80 s |
| `scripts/check-tui-boundary` | aprovado |
| `scripts/check rust` (gate canônico, 18 estágios) | **tudo passou**, ~503 s |
| `git diff --check` | limpo |
| `git diff --stat Cargo.lock` | vazio — nenhuma dependência adicionada |

### 13.9 Integridade do store real

`~/.local/share/note-it`, `~/.config/note-it` e `~/.local/state/note-it` foram
inventariados por tipo, tamanho, `mtime` e caminho antes e depois de todo o
trabalho, inclusive depois de `scripts/check rust` — que executa o harness de
isolamento e, com display, abre um daemon real do Note-it. Os dois inventários
são idênticos, com o mesmo SHA-256
(`3cc2f66847f3c681a7d2c3d71d1cc1cb28fe1f4bbdc2adc6b3d9f2c6d01cc2ba`, 242
entradas). Nenhuma escrita, nenhuma remoção, nenhum `mtime` movido. O harness
usa `$WORK/ambient-home` como stand-in de `$HOME` e um barramento privado; o
`$HOME` real nunca é alvo.

### 13.10 Limitações conhecidas

- **Corpo de fonte não tem representação.** `data-note-it-font-size` é
  reconhecido e consumido, mas um terminal tem uma célula só: o texto aparece,
  o tamanho não.
- **Cor é `Color::Rgb`.** Em um terminal sem truecolor a aproximação é feita pelo
  próprio emulador. Os títulos sobrevivem a isso pelo marcador `#` e pelos
  modificadores; uma cor de nota aproximada continua uma cor, apenas menos exata.
- **Texto colorido dentro de um marca-texto perde a cor**, exatamente como no
  editor gráfico, onde o `color` inline do `<mark>` vence o do `<span>` externo.
  É fidelidade deliberada, não correção de contraste.
- **A seleção do cursor de leitura ainda pinta o fundo da linha** e um
  marca-texto continua vencendo essa pintura na sua extensão. A legibilidade da
  seleção sobre cores semânticas é escopo declarado da **Fase 5.0D.3**.
- **O rótulo `(Fase 5.0B / 5.0C)` no cabeçalho continua obsoleto** — também
  escopo declarado da 5.0D.3, e por isso não tocado aqui.
- **Um comentário que começa no meio de uma linha e continua na seguinte** não é
  tratado como bloco: o marcador sai e o texto permanece. O serializador do
  Note-it escreve comentários em bloco próprio, então a forma não ocorre em
  notas que ele produziu.

**5.0D.1: PASS — nenhum HTML canônico, delimitador reconhecido, comentário ou
entidade alcança a tela; seis níveis de título distinguíveis; store real
inalterado. Fases 5.0D.2, 5.0D.3 e 5.0E não iniciadas.**

## 14. Fase 5.0D.2 — Editor nativo no painel direito

### 14.1 Contrato e o que mudou

O roadmap pede que o `$EDITOR` deixe de ser o fluxo principal: abrir uma nota
para trabalhar entra em edição direta, sem uma segunda tecla `e`. A exclusão
histórica registrada na seção 6, item 1 — "editor de texto completo embutido com
emulação Vim/Emacs" — foi **explicitamente revogada** pelo roadmap para o editor
v1 desta fase. Não há contradição de escopo: o que a seção 6 recusou (modos,
realce de sintaxe, desfazer ilimitado) continua fora; o que a 5.0D.2 entrega é um
editor não-modal, sem realce e com desfazer de limite explícito.

A regra de navegação passa a ser:

| Onde | Tecla | Resultado |
| --- | --- | --- |
| Lista | `↑` `↓` | seleciona, e o painel direito **pré-visualiza a nota renderizada** |
| Lista, busca ou tarefas | `Enter` / `→` / `l` | abre a nota **editando**, sem segunda tecla |
| Lista | `n` | cria a nota e já abre editando |
| Lixeira | `Enter` | apenas leitura; uma nota descartada não é editável |
| Editor | `Esc` | sai para a leitura (perguntando, se houver pendências) |
| Leitura | `Enter` / `i` | volta a editar |
| Leitura | `Esc` | volta à lista |
| Leitura | `e` | `$EDITOR` externo, inalterado desde a 5.0D |

Selecionar não é abrir. É o que mantém o leitor — e com ele toda a fidelidade
de Markdown da Fase 5.0D.1 — na frente de quem está navegando, enquanto quem
decidiu trabalhar na nota recebe o cursor de texto imediatamente.

### 14.2 Arquitetura

Três peças, nenhuma nova dependência:

- **`draft.rs` — o modelo.** Linhas de texto, um cursor, uma âncora de seleção e
  um histórico limitado. Não conhece Markdown, store, revision nem terminal, o
  que permite testar suas regras sem nenhum dos três. Uma [`Position`] conta
  **caracteres**: bytes deixariam um cursor cair dentro de um `ç` e colunas de
  exibição amarrariam o modelo a uma fonte. Toda operação é total — nenhum
  índice parte uma sequência UTF-8, então nenhuma entrada causa pânico.
- **`app.rs` — a máquina de estados.** `Focus` ganha `Editor`. O rascunho existe
  exatamente enquanto o foco é `Editor`, e é isso que reduz "sair, trocar de
  nota/painel, buscar ou descartar com alterações pendentes" a **uma única
  porta**.
- **`ui.rs` — o painel.** Desenha a fonte da nota, a seleção e o cursor como
  células do próprio buffer, o que também os torna verificáveis por
  `TestBackend`. Borda âmbar contra a ciano da leitura; título `Edição: <rótulo>`
  com `●` enquanto houver pendência; atalhos no rodapé.

O editor **não interpreta Markdown**. O leitor da 5.0D.1 continua a única
projeção renderizada que existe na TUI; não há uma segunda interpretação para
discordar dela. O que se edita é o que o arquivo guarda.

### 14.3 Teclado do editor

| Tecla | Ação |
| --- | --- |
| qualquer caractere | insere |
| `Enter` | quebra de linha |
| `Tab` | quatro espaços |
| `Backspace` / `Delete` | remove, ou junta linhas nas bordas |
| `←` `→` `↑` `↓` `Home` `End` `PgUp` `PgDn` | navega |
| `Shift` + as mesmas | estende a seleção |
| `Ctrl+A` | seleciona tudo |
| `Ctrl+Z` / `Ctrl+Y` (ou `Ctrl+Shift+Z`) | desfazer / refazer |
| `Ctrl+S` | salvar |
| `Esc` | sair da edição |
| `Ctrl+C` | pedir para encerrar |

`Tab`, `/`, `d` e `q` navegam em outros lugares da aplicação; aqui são texto.
Um `Tab` vira quatro espaços porque um corpo Markdown indenta com espaços, e
guardar uma tabulação cuja largura ninguém combina seria guardar uma surpresa.

### 14.4 Salvamento e conflito

O funil de escrita é o da seção 11.2, sem exceção nova:

| Ação | Operação | Precondição |
| --- | --- | --- |
| `Ctrl+S` | `MutateNote` com `ReplaceBody` ou `ClearBody` | `Some(revision lida ao abrir o painel)` |

A escolha da mutação usa **`editor::mutation_for`**, exatamente a função com que
o editor externo decide. "Pendente" significa, portanto, uma só coisa nesta
aplicação: uma alteração que o modelo canônico persistiria. Um corpo que difere
apenas por terminadores de linha não é pendente, não pergunta nada ao sair e não
escreve nada — a mesma resposta que o `$EDITOR` já dava.

Em `WriteError::RevisionConflict` **nada é sobrescrito e nada é jogado fora**: o
rascunho continua na tela e o painel faz uma pergunta com três saídas:

- `[p] preservar rascunho` — grava os bytes do rascunho em
  `${XDG_STATE_HOME}/note-it/tui-recovery/<note-id>-<uuid>.md` e relê a nota;
- `[r] reler a nota` — descarte consciente do rascunho, relendo o que está no
  store;
- `[Esc] manter no editor` — não escreve nada e devolve o rascunho intacto.

A preservação usa `editor::preserve_draft`, extraída de `EditorSession::recover`
para que os dois editores tenham **uma só** implementação de recovery, com as
mesmas garantias já documentadas na seção 11.4: arquivo `0600`, diretórios
`0700`, UUID e `create_new` contra colisão, arquivo e diretório sincronizados,
bytes literais sem front matter nem canonização.

### 14.5 Alterações pendentes nunca somem em silêncio

O rascunho só existe com o foco no editor, e do editor só se sai por `Esc`,
`Ctrl+C` ou sinal. **Uma tecla e um sinal não são a mesma coisa**, e a diferença
decide qual garantia se aplica: numa há alguém na frente da tela para responder,
na outra não há.

- **`Esc`** com pendências pergunta `[s] salvar  [d] descartar  [Esc] continuar
  editando`. Sem pendências, sai direto para a leitura.
- **`Ctrl+C` é uma tecla, não um sinal.** Em raw mode o crossterm desabilita
  `ISIG`, então `Ctrl+C` chega como evento de teclado — é uma pessoa pedindo
  para sair, com tela para perguntar e alguém para responder. Com pendências,
  faz **a mesma pergunta** que o `Esc`: `[s]` salva e encerra, `[d]` descarta
  conscientemente e encerra, `[Esc]` continua editando e cancela a saída. Sem
  nada pendente, encerra na hora, como sempre fez. Se salvar esbarrar num
  conflito, o pedido de sair não sobrevive a ele: a pergunta do conflito assume,
  nada é sobrescrito e ninguém sai sem decidir.
- **`SIGINT`, `SIGTERM` e `SIGHUP` entregues de fora** não podem ser
  perguntados: um sinal não responde. O rascunho pendente é então **preservado**
  no diretório de recovery e seu caminho é impresso no terminal já restaurado.
  É a leitura honesta de "não pode perder texto silenciosamente" quando a
  confirmação é fisicamente impossível.

  A distinção foi estabelecida na revisão R2 desta fase. Até ela, `Ctrl+C` de
  teclado desviava para o caminho dos sinais e encerrava na hora, deixando
  apenas um recovery — o texto não se perdia, mas a saída acontecia sem a
  confirmação que o contrato exige de toda saída iniciada pela pessoa.

  `SIGHUP` foi acrescentado à mesma flag na revisão R1 desta fase. Antes ele não
  era tratado, e sua disposição padrão matava o processo na hora — levando junto
  a restauração do terminal e qualquer rascunho não salvo. É o sinal que uma
  pessoa realmente encontra: fechar a janela do terminal ou cair a sessão ssh.
  A revisão também moveu a preservação para **fora** do `?` do laço de eventos,
  porque o companheiro habitual de um `SIGHUP` é justamente a escrita de quadro
  que descobre que o terminal sumiu; qual dos dois o processo percebe primeiro
  não pode decidir se o texto sobrevive.

- **`SIGTSTP`/`SIGCONT`** não são tratados, e não devem ser: parar um processo
  não o encerra, então o rascunho continua exatamente onde estava quando ele
  volta. Uma suspensão não é uma forma de perder texto.
- **Trocar de nota, de painel, buscar ou descartar** são estruturalmente
  inalcançáveis com pendências, porque as teclas que fariam isso são texto
  dentro do editor. É uma garantia mais forte que uma confirmação, e há teste
  para ela.

### 14.6 Terminal

Esta fase não altera `terminal.rs`, o ciclo do `$EDITOR`, `suspend`/`resume`, o
panic hook ou o snapshot T0 da revisão corretiva da seção 12. O editor nativo é
in-process: não suspende o terminal, não lança processos e não toca em raw mode.
As nove regressões de PTY da 5.0D continuam byte-idênticas em asserções, e as
novas provas de terminal real confirmam restauração exata (`assert_restored`) e
retorno a modo cozido depois de editar, cancelar e ser interrompido.

### 14.7 Provas

**Regressão antes da correção.** `noteit-tui/tests/native_editor.rs` foi escrito
contra a baseline `86290a0` e falhou em **29 dos 31** testes iniciais. Os dois
que passavam eram guardas de preservação — a pré-visualização renderizada da
5.0D.1 e "a lixeira não é editável" — e continuam passando. Nenhuma falha foi
por erro de compilação: o arquivo usa apenas a superfície pública que já existia
(`App`, `handle_key`, `draw`, `core`), de modo que a baseline é comportamental.

Contagens finais de `cargo test -p noteit-tui` — **144 testes** (133 no fechamento inicial, mais 4 de sinal na revisão R1 e 7 de saída por `Ctrl+C` na revisão R2, ambas na seção 14.5):

| Alvo | Testes | |
| --- | --- | --- |
| unitários de `src/lib.rs` | 26 | 14 de `draft.rs` são novos |
| `tests/native_editor.rs` | 45 | novo (34 + 4 da R1 + 7 da R2) |
| `tests/markdown_fidelity.rs` | 30 | 5.0D.1, inalterado |
| `tests/transactions.rs` | 18 | 1 expectativa de foco atualizada |
| `tests/terminal_lifecycle.rs` | 9 | inalterado |
| `tests/editor_process.rs` | 8 | só a sequência de teclas mudou |
| `tests/navigation_and_rendering.rs` | 7 | 2 expectativas de foco atualizadas |
| `tests/desktop_concurrency.rs` | 1 | só a sequência de teclas mudou |

O que é provado: entrada em edição sem segunda tecla; identidade e atalhos
visíveis; `Esc` em dois degraus; inserção e remoção Unicode e multilinha; junção
de linhas; seleção por `Shift`+setas e `Ctrl+A`, visível e substituível;
desfazer/refazer com agrupamento por tipo de edição e limite respeitado;
salvamento com a revision lida, salvamentos encadeados, no-op canônico sem
escrita; as três saídas do conflito, com o arquivo de recovery conferido byte a
byte; pendência perguntando antes de sair, e teclas de navegação sendo texto;
nota vazia, nota mais alta que o painel, linha mais larga que o painel, terminal
estreito, redimensionamento em ambas as direções, `PgUp`/`PgDn` parando nas
bordas; conteúdo hostil inerte; lixeira não editável.

Em terminal real (PTY): edição com acentuação salva no store; colagem
multilinha; terminal estreito (44×14); cancelamento sem escrita; `Ctrl+C` com
pendência preservando o rascunho e informando o caminho; e o `$EDITOR` externo
continuando disponível como ação alternativa. Todos terminam com
`assert_restored()` e o PTY de volta ao modo cozido.

**Por que as provas visuais em PTY afirmam pelo store.** O Ratatui repinta
apenas as células que a tecla mudou, então uma palavra digitada tecla a tecla
nunca atravessa o fluxo inteira. Esperar por ela seria uma corrida. As provas de
terminal real afirmam pelo que o store passa a conter e pelo prompt que só
aparece se o texto registrou; os glifos desenhados são afirmados com
`TestBackend`, que desenha um quadro completo.

### 14.8 Gates

| Comando | Resultado |
| --- | --- |
| `cargo test -p noteit-tui --test native_editor` | 34 aprovados |
| `cargo test -p noteit-tui` | 144 aprovados (133 no fechamento inicial; R1 acrescentou 4 e R2 acrescentou 7) |
| `cargo fmt --all -- --check` | limpo |
| `cargo check --workspace` | aprovado |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | aprovado |
| `scripts/check-tui-boundary` | aprovado |
| `scripts/check rust` (18 estágios) | **tudo passou**, ~166 s |
| `git diff --check` | limpo |
| `git diff --stat Cargo.lock` | vazio — nenhuma dependência adicionada |

### 14.9 Limitações conhecidas

- **A edição é por escalar Unicode, não por grafema.** Um acento combinante, uma
  bandeira ou um emoji de família nunca é corrompido, cortado ao meio ou
  reordenado — apenas exige mais de um `Backspace`. É o que permite ao editor
  não ter tabela de segmentação nem dependência.
- **A rolagem horizontal conta caracteres, não colunas de exibição.** Em uma
  linha com caracteres largos (CJK, emoji) o limiar de rolagem é aproximado; o
  cursor continua sobre o caractere certo.
- **Sem coluna “pegajosa”:** subir e descer por uma linha curta e voltar não
  restaura a coluna original, ela fica presa ao comprimento da linha curta.
- **Sem realce de sintaxe, sem modos, sem busca dentro do editor.**
- **Uma tabulação existente em uma nota externa é desenhada como `→`** (uma
  célula), para que a coluna que o cursor informa e a que se vê sejam a mesma.
- **O rótulo `(Fase 5.0B / 5.0C)` no cabeçalho continua obsoleto** e a
  legibilidade da seleção do leitor sobre cores semânticas continua pendente:
  ambos são escopo declarado da **Fase 5.0D.3** e não foram tocados.

**5.0D.2: PASS — edição direta no painel direito, salvamento transacional com a
revision lida, conflito sem sobrescrita e com três saídas, pendências nunca
perdidas em silêncio, e a fidelidade de Markdown da 5.0D.1 intacta na leitura.
Fases 5.0D.3 e 5.0E não iniciadas.**

## 15. Fase 5.0D.3 — polimento, mouse e formatação inline

A interface usa o título durável **NOTE-IT — Interface de Terminal**, sem número
de fase. Em menos de 60 colunas exibe somente o painel focado; abaixo de 35×6
mostra um estado mínimo determinístico. A matriz automatizada cobre 40×12,
80×24 e 140×34, inclusive editor, menu de formatação, Unicode e texto longo.
O leitor mantém wrapping sem aparar espaços e a seleção usa `REVERSED`,
preservando foreground, background e modificadores semânticos.

O mouse é capturado somente durante a tela alternativa. Clique nas quatro
regiões semânticas do cabeçalho abre Notas Recentes, Tarefas Pendentes, Lixeira
ou Busca; clique numa linha visível usa a mesma seleção/abertura da navegação
por teclado. A roda atua no painel sob o ponteiro. Uma troca iniciada por mouse
com rascunho pendente passa pela mesma pergunta salvar/descartar/continuar.
Teclado permanece disponível (`1`–`3`, `/`, setas, `j`/`k`, `Enter`). Seleção
de texto com mouse não faz parte desta fase.

No editor, **Alt+F** ou clique no comando do rodapé abre o menu de formatação,
navegável por setas/Enter e clique. Com seleção, a ação é aplicada imediatamente
à faixa já selecionada. Sem seleção, a mesma ação configura o estilo transitório
da digitação seguinte: cor do texto e marca-texto podem ser combinados, trocados
ou limpos independentemente, e o título do editor indica, de forma compacta, o
estado ativo. Alterar esse estado sozinho não suja o rascunho nem cria histórico;
cancelar o menu é uma operação nula. A
paleta vem de `ui/src/ui/palettes.ts`: texto Cinza `#64748B`, Vermelho
`#DC2626`, Laranja `#C2410C`, Amarelo `#A16207`, Verde `#15803D`, Azul
`#2563EB`, Roxo `#7C3AED` e Rosa `#DB2777`; marca-texto Amarelo `#FDE68A`
(padrão), Verde `#BBF7D0`, Azul `#BFDBFE`, Rosa `#FBCFE8` e Roxo `#DDD6FE`.
Limpar cor e limpar marca-texto são ações próprias. A persistência usa
`<span data-note-it-color="…" style="color:…">` e
`<mark data-note-it-highlight="…" style="background-color:…">`.

Formatar é mutação do `Draft`: preserva Unicode e múltiplas linhas, cria um
passo de undo/redo, marca pendência e só chega ao store por `Ctrl+S`,
`authority::perform_at` e a revision originalmente lida. Conflitos continuam
sem sobrescrita. A limpeza remove apenas wrappers canônicos completos dentro da
seleção; HTML desconhecido ou tag incompleta permanece intacto. Seleção futura
por mouse e demais comandos rich-text ficam adiados.

Na correção 5.0D.3.R1, a digitação estilizada agrupa caracteres adjacentes com
semântica idêntica em um único wrapper canônico e mantém cor e marca-texto em
aninhamento compatível com a GUI e o leitor. Backspace, Delete, Enter, Unicode e
undo/redo passam pelo mesmo `Draft`; limites de tag não são atravessados por uma
remoção destrutiva.

O rodapé é contextual e escolhe variantes inteiras por largura, sem cortar um
atalho no meio. A matriz cobre 40, 50, 60, 70, 80, 100, 120 e 140 colunas;
Salvar, Formatar e Sair continuam descobríveis no editor em larguras práticas.
Avisos transitórios são limpos na próxima ação significativa, troca de foco,
nota ou painel, conclusão/cancelamento da formatação e interação com tarefa.

Em Tarefas Pendentes, o texto é apresentado pelo mesmo renderer inline seguro
do leitor: delimitadores Markdown e wrappers canônicos não vazam. `Space`
alterna a tarefa selecionada; clicar exatamente em `☐`/`☑` executa a mesma ação,
enquanto clicar no restante da linha abre a nota. A mutação continua usando o
`TaskRef` e a revision carregada pela autoridade transacional do Core; conflito
preserva a edição externa e produz aviso determinístico. O editor nativo segue
mostrando o Markdown/HTML fonte, pois ali ele é o conteúdo editável. Nenhuma
parte da Fase 5.0E foi iniciada.

### 15.1 Fechamento e descoberta arquitetural da R2

A 5.0D.3/R1 está encerrada dentro do escopo aprovado. Hoje a TUI oferece edição
nativa da **fonte Markdown/HTML canônica**, navegação por mouse, formatação de
uma seleção, cor e marca-texto para digitação futura com reset e indicador
visível, rodapé contextual responsivo e tarefas renderizadas com checkbox
acionável por mouse ou `Space`. O editor não deve ser descrito como visual: nele,
os delimitadores Markdown e os wrappers canônicos de cor e marca-texto continuam
visíveis e editáveis.

A revisão arquitetural 5.0D.3.R2 não invalidou mouse, formatação, tarefas nem
responsividade. Ela estabeleceu que ocultar markup enquanto se edita não é um
polimento seguro sobre o `Draft` atual: cursor e seleção são indexados na fonte
persistida, enquanto o renderer do leitor descarta delimitadores sem produzir um
mapeamento reversível. Esconder trechos por regex ou ajustar offsets como texto
solto poderia corromper conteúdo. Por isso, permanecem inexistentes na TUI:

- edição visual/WYSIWYM verdadeira;
- mapeamento bidirecional fonte↔visual para cursor, seleção e mutações;
- alternância entre editor Visual e editor Markdown;
- avaliação matemática com a mesma semântica do editor gráfico;
- extração e apresentação semântica de flashcards equivalente à GUI.

Esses itens foram separados em fases futuras: a 5.0D.4A define e aprova primeiro
a arquitetura sem perdas; a 5.0D.4B só então implementa os modos Visual e
Markdown; e a 5.0D.5 trata a paridade semântica de matemática e flashcards sem
criar parsers divergentes e ingênuos. Até lá, a fonte canônica permanece
inteiramente acessível no editor atual, e o Core, o formato persistido e os
fluxos transacionais não mudam.

## Relatório de arquitetura — Fase 5.0D.4A

### 1. Estado inicial

O gate foi conferido em 2026-09-10 antes de qualquer alteração. A branch era
`main`, o working tree estava limpo e `HEAD` e `origin/main` apontavam para
`04532f19af1d06da377575468a5610eee2e9b2be` (`feat(tui): complete phase
5.0D.3 polish`). A Fase 5.0D.3 consta como concluída em `docs/roadmap.md`. O CI
remoto do mesmo SHA, run `34519476358`, estava `completed/success`. Portanto o
gate permitiu esta análise.

### 2. Arquitetura atual

O fluxo atual é:

```text
NoteDocument.content + revision
          |
          +--> leitor: markdown.rs -> inline.rs -> Lines/Spans Ratatui
          |
          +--> editor: Draft(lines, Position escalar, seleção, snapshots)
                           |
                           +--> mutation_for -> ReplaceBody/ClearBody
                                   |
                                   +--> authority::perform_at(expected_revision)
```

`Draft` é corretamente ignorante de Markdown e possui a única cópia mutável do
corpo durante a edição. Suas posições são `(linha, coluna)` em valores escalares
Unicode; o texto é reconstruído sem perder quebras internas. O histórico tem no
máximo 200 snapshots integrais e agrupa inserções e remoções consecutivas.
Formatação de cor e marca-texto, entretanto, hoje altera diretamente substrings
da fonte e usa buscas locais de wrappers; isso é adequado apenas no editor raw.

`app.rs` mantém o snapshot carregado com sua revision, detecta pendência pela
mesma canonicalização do Core, salva uma única vez pela autoridade e nunca faz
retry após conflito. Sinais preservam os bytes do rascunho em `tui-recovery`.
Esses contratos não pertencem à projeção visual e não serão mudados.

`markdown.rs` reconhece blocos linha a linha: fences, comentários, citações,
callouts, headings, separadores, tarefas e listas. `inline.rs` faz uma varredura
segura e limitada para ênfase, links, código inline, entidades e o pequeno
subconjunto HTML da GUI. Ela produz apresentação, não tokens com intervalos; em
particular descarta delimitadores, destinos, comentários e tags. O vetor
`sources` de `RenderedMarkdown` só associa uma linha renderizada a uma linha da
fonte, portanto não é um source map editável.

A GUI usa Tiptap/ProseMirror e `@tiptap/markdown`. Extensões próprias serializam
underline como `<u>`, cor e tamanho como `<span data-note-it-...>`, highlight
como `<mark data-note-it-highlight=...>`, além de tarefas, callouts, comentários,
código, imagens, matemática e flashcards. A árvore da GUI é normalizada e sua
serialização pode mudar uma grafia equivalente; logo ela informa o vocabulário
canônico suportado, mas não pode servir como round-trip lossless da TUI.

O Core persiste `NoteDocument.content` como Markdown canônico, preserva campos
desconhecidos do front matter, condiciona mutações a `NoteRevision` e expõe
`visible_text` para busca/apresentação. Esse helper também é deliberadamente
lossy e não é base de edição. Não há parser Markdown Rust nem rope diretamente
declarado pelo crate TUI. `unicode-segmentation` e `unicode-width` existem no
lock por dependências transitivas, mas não são API direta disponível sem uma
decisão posterior de dependência — proibida nesta fase.

### 3. Problema estrutural

Uma posição visual é uma fronteira entre unidades semânticas; uma posição da
fonte é uma fronteira de bytes/escalares que também atravessa delimitadores e
atributos invisíveis. A relação não é bijetiva: em `**abc**`, várias fronteiras
da fonte colapsam na fronteira visual anterior a `a` ou posterior a `c`.
Entidades têm ainda cardinalidade diferente (`&amp;` vira `&`). Assim, ocultar
caracteres e reaproveitar offsets do `Draft` tornaria cursor, seleção e remoção
ambíguos e poderia cortar sintaxe invisível.

### 4. Invariantes obrigatórias

A arquitetura adota como contratos verificáveis:

1. `Draft.text()` é a única fonte mutável e a única candidata a persistência.
2. `VisualDocument` é transitório, derivado e descartável; nunca é salvo.
3. Todo segmento projetado aponta para intervalos de bytes UTF-8 válidos da
   revisão exata da fonte da qual foi derivado.
4. Todo caret visual editável tem uma afinidade determinística com uma fronteira
   da fonte; posições sem resposta única não são editáveis.
5. Tokens invisíveis existem explicitamente no mapa, inclusive com largura
   visual zero.
6. Uma edição substitui apenas intervalos explicitamente possuídos pelo nó; bytes
   desconhecidos fora deles são copiados literalmente.
7. Inserção, Backspace, Delete e Enter são operações semânticas, não aritmética
   cega sobre offsets.
8. Seleções multilinha são decompostas por fronteiras de bloco e só executadas
   se todo o intervalo for mutável sem atravessar região protegida.
9. Formatação produz um patch de fonte validado e normalizado, nunca wrappers
   por caractere.
10. Cada comando visual é uma transação única no histórico existente.
11. o modo Markdown raw continua integral e sempre pode representar a fonte.
12. alternar modo só reprojeta e mapeia a seleção; não cria patch nem pendência.
13. revision, conflito, autoridade, canonicalização, recovery e sinais continuam
    exatamente acima do editor.
14. Core, formato de storage e front matter não mudam.
15. falha ou ambiguidade é fail-closed: preserva a fonte e oferece modo Markdown.

### 5. Alternativas avaliadas

| Critério | A — tokens/AST lossless | B — rope/piece table anotada | C — árvore incremental | D — WYSIWYM controlado |
|---|---|---|---|---|
| Complexidade inicial | alta: lexer, ranges e regras de patch | muito alta: buffer e metadados móveis | muito alta: gramática incremental e adaptação | média se apoiado em A e escopo pequeno |
| Memória | fonte + nós/ranges, linear | fonte fragmentada + árvore + anotações | árvore, cache e fonte, linear com constante maior | fonte + projeção apenas do subconjunto |
| Atualização por tecla | reparse de bloco/linha; full parse como fallback | patches baratos, anotações difíceis de reparar | incremental natural, porém integração complexa | reparse local nos nós editáveis; opacos não mudam |
| Cursor/seleção | ranges e carets explícitos resolvem | âncoras estáveis ajudam, semântica ainda necessária | offsets reversíveis se a gramática os preservar | simples e seguro dentro do subconjunto; bloqueado fora |
| Inserção/remoção | resolvida por ownership de delimitadores | eficiente, mas o rope sozinho não define limites | resolvida pela árvore se sem erros ambíguos | regras explícitas por construção suportada |
| Formatação/multilinha | patches estruturais e validação | exige camada estrutural adicional | transformações da árvore mais serializer lossless | somente combinações comprovadas; expansão gradual |
| Unicode | pode mapear bytes, escalares, grafemas e células | rope escolhido pode impor unidade inconveniente | depende da biblioteca | grafema no visual, byte range na fonte |
| Desconhecido/malformado | tokens `Opaque` preservam bytes | bytes preservados, comportamento semântico ausente | error nodes variam por parser | atômico/source-visible/raw, sempre preservado |
| Fences/código | blocos atômicos ou conteúdo literal mapeado | possíveis, ainda requer parser | bons se a gramática é robusta | inicialmente source-visible/atômico |
| HTML/tags Note-it | lexer dedicado pode preservar tags exatas | anotações complexas aninhadas | HTML embutido costuma ser o caso difícil | tags canônicas suportadas; demais opacas |
| Aninhamento, tarefas e callouts | representáveis com ownership aninhado | viáveis com segunda árvore | bons quando a gramática cobre extensões | liberados construção a construção |
| Matemática/flashcards | decorações anexas a ranges | anotações naturais, mas caras de manter | queries na árvore | decoração não persistida sobre ranges |
| Undo/redo | patch/snapshot da fonte | operações do piece table | operações de árvore precisam serializer | comandos viram um patch/snapshot da fonte |
| Preservação da fonte | forte se o serializer só toca ranges | muito forte para bytes não tocados | incerta se a árvore normaliza ao serializar | forte, pois regiões não suportadas não são reescritas |
| Compatibilidade com Draft | boa como camada acima | exigiria trocar seu armazenamento | exigiria novo modelo já no início | ótima; `Draft` permanece dono da fonte |
| Testabilidade | excelente por propriedades de ranges | boa, porém estado interno amplo | depende da biblioteca/gramática | excelente matriz por capability e fail-closed |
| Adoção incremental | boa | baixa | média | melhor: começa estreito sem prometer WYSIWYG total |

Opção A isolada tenderia a liberar cedo demais operações sobre toda a árvore.
Opção B otimiza edição antes de resolver a semântica e duplicaria o papel atual
do `Draft`. Opção C não pode ser aprovada sem selecionar e provar um parser que
preserve ranges, extensões e erros; nenhum existe hoje na TUI. A decisão é a
**Opção D implementada sobre a infraestrutura lossless da Opção A**: WYSIWYM
controlado, com capability explícita por nó. Isso é uma única arquitetura, não
duas fontes de verdade.

### 6. Arquitetura recomendada

```text
App / revision / recovery
          |
       Draft  <----- histórico de estados da fonte
   (fonte canônica)
          |
   LosslessProjector ----> VisualDocument (cache derivado)
          |                     |
   tokens + ranges          blocos, runs, carets,
   inclusive trivia        estilos e regiões opacas
                                |
                         VisualCommand
                                |
                    EditPlanner -> SourcePatch
                                |
                    validar ranges/UTF-8/ownership
                                |
                         aplicar no Draft
```

`VisualDocument` guarda a identidade/geração do texto que projetou. Um comando
com geração antiga é rejeitado e reprojetado. `EditPlanner` é o único componente
capaz de converter intenção visual em patch; o renderer nunca modifica fonte.
Após qualquer patch, a fonte do `Draft` muda primeiro e a projeção afetada é
recriada. Não existe sincronização bidirecional entre dois documentos.

Capabilities mínimas:

| Construção | Política inicial no Visual |
|---|---|
| texto, parágrafo | totalmente editável |
| heading H1–H6 | texto editável; nível é atributo de bloco |
| bold, italic, underline, strike | editável e formatável quando balanceado/canônico |
| cor e highlight canônicos | editáveis; atributos validados; aninhamento conhecido |
| links | rótulo editável; destino é metadado atômico, alterável só por comando próprio futuro ou raw |
| entidades reconhecidas | visual e atômica como um grafema; editar substitui a entidade inteira por texto escapado canônico |
| inline code | visual, conteúdo literal editável; delimitadores pertencem ao nó; formatação interna proibida |
| fenced code | source-visible inicialmente; bloco atômico para seleção visual externa |
| listas simples | conteúdo editável; marcador é atributo/prefixo atômico; transformação estrutural só quando especificada |
| tarefas | conteúdo editável; checkbox atômico; metadado `note-it:` protegido |
| blockquote/callout | visual, mas prefixos/marker atômicos; Enter estrutural somente após testes próprios |
| tabela, se encontrada | source-visible/read-only até haver gramática e regras de células |
| HTML desconhecido/malformado, comentários | região opaca e source-visible; raw obrigatório para alterar |
| matemática e delimitadores de flashcard | texto comum nesta fase; apenas âncoras para decoração futura |

Quando um documento contiver regiões opacas, o restante continua editável. Uma
seleção que as atravesse é recusada com ação para ir ao modo Markdown; não se
faz edição parcial silenciosa.

### 7. Modelo de source map

Todos os offsets de fonte são **bytes**, sempre em fronteiras UTF-8. O modelo
conceitual é:

```text
VisualDocument {
  source_generation,
  source_len,
  blocks: [VisualBlock],
  segments: [Segment],
  carets: [CaretMap],
  decorations: [TransientDecoration]
}

Segment {
  id,
  parent,
  source: ByteRange,
  visual: GraphemeRange,
  role: Text | OpenSyntax | CloseSyntax | BlockPrefix | Metadata |
        Entity | Opaque | LineBreak,
  semantic: Plain | Heading(level) | Strong | Emphasis | Underline |
            Strike | Color(hex) | Highlight(hex) | Link | InlineCode | ...,
  capability: EditableText | Atomic | Protected | SourceVisible,
  ownership: node_id,
  style_stack,
}

CaretMap {
  visual_boundary,
  left_source_boundary,
  right_source_boundary,
  default_affinity: Left | Right,
  allowed_operations,
}
```

`OpenSyntax`, `CloseSyntax`, `BlockPrefix` e `Metadata` têm range visual vazio,
mas range de fonte não vazio. Texto pode ter mapeamento interno por fronteiras
de grafemas. Entidades são um segmento visual de um grafema para vários bytes.
Uma quebra visual causada apenas por wrapping tem range de fonte vazio e não é
um caret lógico; `\n` real é `LineBreak`.

Ranges são semiabertos. Após um patch `[start,end) -> replacement`, todos os
ranges anteriores à edição permanecem; os posteriores deslocam pelo delta e o
menor bloco delimitado que contém a edição é reparseado. Se seu novo limite não
for demonstrável, reprojeta-se o documento todo. Nenhum range velho é usado após
incrementar `source_generation`.

Em `**abc**`, o open `0..2` e close `5..7` são invisíveis; o texto `2..5` ocupa
visual `0..3`. O caret visual inicial tem afinidade interna/right e resolve em
fonte 2; o final tem afinidade interna/left e resolve em 5. Inserir no início ou
fim insere dentro do strong. Backspace antes de `a` e Delete depois de `c` não
tocam delimitadores: operam no vizinho visual exterior ou não fazem nada nos
limites do documento. Backspace depois de `a` remove somente `a`; Delete antes
de `c` remove somente `c`. Se o conteúdo ficar vazio, a operação estrutural
remove o par completo `****`, produzindo texto vazio, em um patch único.

### 8. Cursor

O cursor visual é `(block_id, grapheme_boundary, affinity)`, nunca célula do
terminal nem offset cru. Setas esquerda/direita percorrem grafemas; cima/baixo
usam a coluna visual desejada medida em células apenas para escolher outro
caret, sem armazenar a célula como posição textual. Home/End usam o conteúdo
visual do bloco. Carets não são criados dentro de syntax/protected/opaque.

Ao reparsear, o cursor é restaurado por uma âncora: primeiro a fronteira da fonte
resultante do patch, depois contexto `(node kind, bytes vizinhos, afinidade)`.
Se ela desapareceu, escolhe-se deterministicamente o caret editável mais próximo,
preferindo o lado indicado. Nunca se clampa para dentro de tag.

### 9. Seleção

Anchor e head usam carets visuais e preservam direção. A tradução produz uma
lista ordenada de `OwnedSlice`, não simplesmente o intervalo entre o menor e o
maior offset. Delimitadores totalmente contidos podem ser incluídos apenas pela
operação estrutural que os possui; delimitadores de ancestrais não são apagados
por uma seleção parcial. Uma seleção multilinha inclui `LineBreak` reais e
prefixos apenas segundo regras dos blocos envolvidos.

Antes de mutar, o planner prova: mesma geração, ranges válidos e ordenados, toda
a área coberta por capabilities compatíveis e resultado parseável no subconjunto
pretendido. Ao atravessar `Protected`, `Opaque` ou `SourceVisible`, recusa tudo.
Copiar pode retornar texto visual; uma ação futura explícita pode copiar fonte.

### 10. Inserção / remoção

Inserção resolve o caret com afinidade. Texto digitado é escapado apenas quando
o contexto exige (`&`, `<`, `>` dentro dos wrappers HTML canônicos); Markdown
comum continua texto literal conforme a regra local. Colagem é um comando único
e pode cair para inserção raw escapada se contiver sintaxe não representável.

Backspace e Delete escolhem primeiro o grafema visual adjacente. Cada grafema
aponta para seu range de fonte inteiro, logo um emoji ZWJ ou `&amp;` é removido de
uma vez. Ao esvaziar uma mark, remove-se também o par de delimitadores que ela
possui. Nunca se remove metade de tag, destino de link, fence ou metadado. Join
de blocos só existe quando há regra explícita para o par; caso contrário a tecla
é recusada ou requer raw.

Exemplo aninhado:

```html
<mark data-note-it-highlight="#FDE68A" style="background-color:#FDE68A"><span data-note-it-color="#DC2626" style="color:#DC2626">abc</span></mark>
```

Há nós `Highlight(Color(Text("abc")))`. Os carets antes de `a`, entre `a/b` e
depois de `c` mapeiam, respectivamente, logo após a abertura de `span`, entre os
bytes de `a`/`b` e imediatamente antes de `</span>`; as tags de `mark` continuam
ancestrais invisíveis.

- Inserir `X` em qualquer desses carets insere dentro de ambos os estilos:
  `Xabc`, `aXbc` ou `abcX`, sem criar wrapper novo.
- Backspace antes de `a` não entra nas tags e atua no conteúdo visual anterior;
  inexistindo-o, não faz nada. Entre `a/b` remove `a`. Depois de `c` remove `c`.
- Delete antes de `a` remove `a`; entre `a/b` remove `b`; depois de `c` atua no
  próximo conteúdo visual ou não faz nada. Nunca apaga `</span></mark>`.
- Selecionar `ab` gera o slice exato dos dois caracteres, mantendo os quatro
  wrappers fora da seleção.
- Trocar a cor foreground de `ab` divide/coalesce o nó de cor e resulta
  conceitualmente em um `span` novo para `ab` e no span vermelho para `c`, ambos
  dentro do mesmo `mark`; a serialização usa wrappers canônicos máximos, nunca
  um por caractere. Se a nova cor for vermelha, é no-op.
- Limpar highlight com `ab` selecionado divide o highlight: `ab` permanece no
  span de cor sem `mark`; `c` permanece no trecho vermelho destacado. Os runs
  adjacentes semanticamente iguais são coalescidos.
- Enter entre `a/b` divide o bloco em dois. Inline marks não atravessam a quebra:
  fecha `span`/`mark` antes do `\n` e reabre wrappers canônicos no segundo bloco,
  produzindo dois trechos bem formados. É uma transação/undo. Se o contexto de
  bloco não admitir split, a operação é recusada, nunca escrita como tag partida.

### 11. Formatação

O estilo efetivo do caret é derivado da pilha de ancestrais. `active_style` é
estado explícito para digitação futura e começa como o estilo efetivo ao mover o
cursor, salvo override consciente do usuário. Seleção homogênea mostra seu
estilo; seleção mista mostra estado misto. Alterar apenas `active_style` não suja
nem cria histórico.

Aplicar/limpar bold, italic, underline, strike, cor ou highlight calcula runs
semânticos máximos, divide apenas nas fronteiras da seleção, remove wrappers
vazios e coalesce vizinhos com a mesma pilha. Ordem canônica de marks para novo
texto deve ser única (por exemplo highlight externo, cor interna, depois marks
Markdown), mas wrappers preexistentes fora do patch mantêm seus bytes e sua
ordem. Não se normaliza o documento inteiro. Inline code rejeita formatação
interna. Link preserva destino. Uma mudança produz um patch composto aplicado
do fim para o começo como um único checkpoint.

### 12. Blocos Markdown

Em `# Meu título`, `# ` é `BlockPrefix` invisível possuído por
`Heading(level=1)`; não é caret e o conteúdo começa em fonte byte 2. O nível é
atributo projetado. Pode haver uma ação estrutural futura para mudar nível, mas
digitação comum não edita o marcador.

- Enter no início: cria um parágrafo vazio antes e mantém `# Meu título` como
  heading; o caret vai ao novo parágrafo.
- Enter no meio: divide em `# Meu` e `# título`; ambos são headings do mesmo
  nível, regra determinística escolhida para não perder o atributo.
- Enter no fim: cria um parágrafo vazio abaixo, não outro heading, seguindo a
  expectativa de encerramento do título.

Parágrafo vira heading somente por comando estrutural explícito ou pela fonte
raw; typing de `# ` no meio de texto não reclassifica magicamente nesta primeira
entrega. Listas, tarefas, quotes e callouts terão contratos próprios de Enter e
join antes de ganhar `EditableText`; até lá podem ser visuais-atômicos ou
source-visible conforme a tabela da seção 6.

### 13. Sintaxe desconhecida/malformada

`<custom-widget foo="bar">hello</custom-widget>` vira um único `Opaque` com os
bytes exatos. A política inicial é mostrá-lo source-visible com estilo de região
protegida; não mostrar apenas `hello`, pois isso sugeriria falsamente que o
wrapper pode ser preservado durante qualquer edição interna. Não há caret dentro
dele e Raw Markdown é obrigatório para alterá-lo. Edições antes/depois deslocam
seu range, mas copiam seus bytes literalmente.

Uma extensão futura só poderá exibir `hello` e manter wrapper opaco se provar
matching lossless, oferecer carets apenas num child explicitamente seguro e
definir escaping e split; isso não é premissa da 5.0D.4B. HTML malformado,
comentários, tags desconhecidas e nesting excedente seguem fail-closed. Fonte
inválida nunca é descartada, reparada ou normalizada implicitamente.

### 14. Unicode

Bytes continuam sendo a unidade de ranges da fonte e posições escalares podem
continuar dentro do `Draft` raw para preservar os testes 5.0D.2. O editor Visual
deve usar **grapheme clusters estendidos** como unidade de movimento, seleção e
remoção. Isso impede separar acento combinante, emoji ZWJ e bandeiras. CJK e emoji
podem ocupar duas células; células servem apenas ao layout via largura Unicode.

A migração é por adaptadores: `SourceOffset(byte)` converte para posições do
`Draft`; `GraphemeIndex` indexa cada run visual; `DisplayColumn` é calculado no
renderer. Primeiro, testes com segmentação implementada/provada sem mudar Draft;
depois, uma dependência direta só poderá ser proposta em 5.0D.4B se os gates de
licença, MSRV e lock forem aceitos. Não se deve escrever segmentador próprio.

### 15. Undo/redo

Na adoção inicial, o histórico mantém **snapshots da fonte** porque é a unidade
que já prova round-trip/recovery e limita-se a 200 passos. Cada `VisualCommand`
gera uma transação, mesmo que contenha múltiplos patches. A projeção nunca entra
no histórico; após undo/redo é recriada da fonte e o cursor é restaurado por
âncora.

Operações estruturais ou projetadas sozinhas seriam frágeis diante de parser
novo. Patches de fonte reduziriam memória, mas exigiriam inversão e retenção dos
bytes removidos. Para 1 MB, 200 snapshots podem chegar a cerca de 200 MB mais
overhead; portanto 5.0D.4B deve medir e pode evoluir internamente para patches
reversíveis com checkpoint periódico, sem alterar o contrato observável nem o
limite. Nunca se guarda apenas a string visual.

### 16. Transição Visual ↔ Markdown

Visual → Markdown: mapeia anchor/head pelos `CaretMap` e afinidades para offsets
da fonte atual, converte-os às posições escalares do Draft e troca somente o
modo. Uma seleção que cobre entidade ou nó atômico seleciona seu range fonte
inteiro. Não há patch, canonicalização ou novo histórico.

Markdown → Visual: captura offsets e afinidade raw, reprojeta a fonte literal e
busca o caret visual cujo range contém/limita cada offset. Offset dentro de
sintaxe invisível vai para a fronteira visual externa segundo afinidade; dentro
de `Opaque/SourceVisible` permanece selecionável nesse bloco source-visible. Se
a fonte raw introduziu sintaxe inválida, ela vira texto literal ou `Opaque`; não
é descartada nem corrigida.

Se alguma parte não puder ser projetada, o documento visual abre com a região
source-visible protegida e um aviso. Se nem os limites de bloco puderem ser
determinados com segurança (por exemplo fence não fechado abrangendo o resto),
todo o trecho afetado é um bloco raw, e o modo Markdown permanece disponível.
Alternar repetidamente sem editar deve manter `Draft.text()` byte-idêntico,
selection round-trip definida e zero pendência.

### 17. Integração futura de matemática

`TransientDecoration { kind, anchor: SourceRange/NodeId, placement, payload,
generation }` pertence ao `VisualDocument`, nunca ao `Draft`. Linhas `= 10 + 20`
e `preco := 100` continuam segmentos de texto editáveis e persistem literalmente.
Na 5.0D.5, um analisador poderá anexar resultado ao fim da linha como decoração
zero-source, não selecionável e sem caret. Editar a linha invalida a decoração;
resultado assíncrono de geração antiga é descartado. Copiar, salvar, undo e
source map ignoram a decoração.

### 18. Integração futura de flashcards

`Pergunta :: Resposta` e `Termo ::: Definição` continuam fonte comum nesta fase.
O projetor reserva semantic annotations não proprietárias sobre ranges: pergunta,
delimitador e resposta podem ser extraídos sem reescrever bytes. Em uma futura
visualização, `::`/`:::` pode ser uma decoração/segmento atômico ainda ancorado
ao range literal; Backspace/Delete não o atravessam sem comando explícito. A
extração de cards inline e em bloco lê a mesma geração da fonte e não muda o
documento. Sintaxe ambígua fica texto comum/raw. Nenhum resultado de estudo ou
ID transitório entra no Markdown por essa arquitetura.

### 19. Performance

| Entrada | Estratégia aceitável inicial | Limite esperado |
|---|---|---|
| 1 KB | parse integral por tecla | irrelevante em prática |
| 100 KB | reparse do bloco/linha; integral como fallback medido | alvo interativo; evitar clone extra por frame |
| 1 MB | indexação linear inicial e invalidação localizada | parse integral por tecla não é aceitável |
| 20.000 linhas | índice de inícios de linha + blocos; viewport renderiza janela | navegação não deve varrer tudo a cada frame |
| linha única muito longa | runs/chunks e cálculo de largura só na janela | nunca criar uma célula/cache pesado por coluna invisível |

O parser lossless inicial pode ser O(n) ao abrir/trocar modo. Por tecla, invalida
do início do bloco seguro anterior ao primeiro limite de bloco estável posterior;
fences, HTML multilinha ou alteração de delimitador podem ampliar a janela. Um
fallback O(n) é correto, mas deve ser raro e instrumentável. A arquitetura não
exige rope agora; se medições mostrarem movimentação de `String` patológica, o
storage interno do Draft poderá mudar sem criar segunda fonte canônica.

### 20. Estratégia incremental de migração

1. Introduzir tipos puros `SourceOffset`, `SourceRange`, `VisualPosition`, tokens
   lossless, capabilities e projeção, sem ligar à UI nem ao Core.
2. Provar parse/round-trip e source maps para texto, parágrafo e headings; regiões
   restantes ficam `SourceVisible`.
3. Criar `VisualDocument` transitório **acima** de `Draft`; Raw continua sendo o
   editor 5.0D.2 sem mudanças comportamentais.
4. Ligar renderer visual e transição de modos sem mutação.
5. Liberar edição de plain text e headings; depois marks balanceadas e HTML
   canônico, uma capability por vez, cada qual com testes de boundary.
6. Só depois avaliar listas/tarefas/callouts e code; desconhecido continua raw.

Não se substitui `Draft` na 5.0D.4B. Ele pode ganhar uma API pública de patch
transacional e conversão segura de offsets, mas seus testes existentes de fonte,
cursor, seleção, history e round-trip permanecem válidos. `app.rs` continua
obtendo `draft.text()` para pending/save/recovery. Uma substituição por rope só
pode ser decisão posterior baseada em benchmark e com a mesma interface.

### 21. Estratégia de testes da 5.0D.4B

A pirâmide proposta:

1. Unitários do lexer/projetor: todo token cobre ranges contíguos, ordenados,
   sem overlap/gap; concatenação dos slices reproduz a fonte byte a byte.
2. Unitários do mapa: todo caret editável mapeia ida/volta com afinidade; zero-
   width nunca recebe cursor; ranges sempre caem em fronteira UTF-8/grafema.
3. Property tests: fontes arbitrárias UTF-8, `project(source).source == source`,
   monotonicidade de offsets e `visual -> source -> visual` nas posições válidas.
   Se o projeto não quiser adicionar framework, gerador determinístico em teste;
   fuzz target só após decisão explícita de tooling.
4. Tabelas de mutação: insert/delete/backspace/replace/Enter em cada boundary,
   nested marks, entidades, links, seleção multilinha e regiões protegidas.
5. Round-trip Visual↔Raw com cursor/seleção e zero alteração/pending/history.
6. Unicode: NFC/NFD, combining marks, skin tone, ZWJ, bandeiras, CJK, emoji de
   largura dupla, variação e linha muito longa.
7. Malformados/adversariais: tags/fences/comments sem fechamento, nesting alto,
   controle terminal, HTML desconhecido e delimitadores aleatórios; sempre
   fail-closed e bytes preservados.
8. `TestBackend`: estilo, cursor físico, seleção, wrapping, scroll e blocos
   source-visible em larguras da matriz existente.
9. Integração App/Core isolada: salvar uma única fonte, no-op de modo, conflito,
   undo/redo, recovery e sinais, reutilizando os testes 5.0D.2/5.0D.3.
10. PTY real: alternância de modo, acento composto/emoji, edição nested,
    terminal estreito/redimensionamento, Ctrl+C/SIGINT/SIGTERM/SIGHUP e restauração
    exata de termios.

Invariantes machine-checked obrigatórias: cobertura exata da fonte; nenhuma
posição no meio de UTF-8/grafema/tag; mapa monotônico; patches disjuntos e dentro
do ownership; fonte externa a patches byte-idêntica; operação recusada não muda
fonte/history/cursor; uma ação igual a um undo; reparse independente da viewport;
modo não muda conteúdo; save continua revisionado; decoração nunca serializa.

### 22. Riscos

- **Parser próprio divergir da GUI:** limitar o vocabulário, fixtures cruzadas
  da serialização GUI e fallback raw; não prometer CommonMark completo.
- **Ambiguidade em delimitadores:** capability fail-closed e token `Opaque`.
- **Normalização excessiva:** patch mínimo e proibição de resserializar a árvore
  inteira; comparar bytes fora do patch em testes.
- **Explosão de casos de boundary:** matriz explícita por construção e property
  tests antes de habilitá-la.
- **Unicode/células:** tipos separados para byte, grapheme e display column.
- **Snapshots grandes:** benchmark e eventual patch history, sem reduzir safety.
- **Reparse O(n):** invalidação por bloco e budget/telemetria de teste.
- **Cursor após reparse:** âncoras com afinidade e testes de estabilidade.
- **HTML hostil:** jamais executar; source-visible e rendering inert continuam.
- **Falsa aparência de suporte:** sinalizar visualmente regiões protegidas e
  oferecer atalho claro para Markdown.

### 23. Arquivos/módulos previstos para 5.0D.4B

Sem implementação agora. A revisão independente pode autorizar, no máximo:

- novos módulos TUI `visual.rs` (documento/capabilities), `source_map.rs`
  (tipos/mapeamento), `projection.rs` (lexer/projetor lossless) e
  `visual_edit.rs` (planner/patches);
- ajustes controlados em `draft.rs` para patch transacional/conversão de offset,
  `app.rs` para modo/cache/comandos e `ui.rs` para render/cursor;
- possível refatoração compartilhada de reconhecimento em `markdown.rs`,
  `inline.rs` e `formatting.rs`, mantendo o renderer atual e suas fixtures;
- novos testes unitários e integrações em `noteit-tui/tests/`.

Ficam explicitamente fora: alterações no Core, schema/migration, storage, GUI,
matemática 5.0D.5, semântica de flashcards 5.0D.5 e qualquer 5.0E. Dependência
nova exige decisão separada durante a autorização da implementação.

### 24. Git

Esta fase alterou somente esta documentação de arquitetura. Nenhum código de
produção, Core, schema, migration ou dependência foi alterado. Não houve commit,
push, tag nem release; 5.0D.4B, 5.0D.5 e 5.0E não foram iniciadas.

### 25. Veredito

A arquitetura está suficientemente especificada para revisão independente: há
uma única fonte mutável, source map concreto, semântica de limites, fallback
lossless, migração incremental e critérios machine-checkable. A aprovação não
autoriza implementação automaticamente.

**5.0D.4A APPROVED DESIGN — arquitetura suficientemente definida para revisão independente antes de autorizar a implementação 5.0D.4B.**

## 26. Fase 5.0D.4A.R1 — correção normativa após revisão independente

### 26.1 Status e precedência

A revisão independente bloqueou o desenho original. Esta R1 conserva sua decisão
fundamental, mas substitui qualquer regra anterior incompatível pelos contratos
desta seção. O fluxo normativo passa a ser:

```text
Draft source
  -> lossless projection
  -> immutable VisualDocument
  -> VisualCommand
  -> plan(command, visual_document, draft_generation)
  -> Result<SourceTransaction, Refusal>
  -> Draft
```

`Draft.text()` é a única fonte mutável e a única candidata a persistência.
`VisualDocument` é um valor derivado, imutável e válido para uma única
`Generation`; widgets recebem somente acesso de leitura e nem widget nem
`EditPlanner` podem alterá-lo. Não há edição otimista autoritativa da projeção.
Somente o `Draft` aplica `SourceTransaction`.

Depois de uma transação bem-sucedida, nesta ordem: o `Draft` muda; sua generation
incrementa; o `VisualDocument` anterior fica stale; a projeção afetada é
reconstruída; cursor e seleção são restaurados por âncoras de fonte e contexto.
Comando, slot ou documento de generation antiga é recusado sem mudar estado.

### 26.2 Representação física, semântica e visual

As três camadas são tipos distintos; nenhuma pode desempenhar o papel da outra.

#### Lexeme

`Lexeme` é a partição física da fonte:

```text
Lexeme {
  id: LexemeId,                 // estável somente dentro da Generation
  generation: Generation,
  source: SourceRange,
  kind: LexemeKind,
  owner: Option<NodeId>
}
```

Lexemes são ordenados, contíguos, não sobrepostos e cobrem exatamente
`0..source_len`. A propriedade lossless normativa é:

```text
concat(source[lexeme.source] para cada Lexeme em ordem) == source.as_bytes()
```

Somente `Lexeme` participa dessa propriedade. Todo byte pertence exatamente a
um lexeme, inclusive texto visível, delimitador Markdown, open/close tag HTML,
nome/valor/separador de atributo, entidade, whitespace, tab, line ending,
escape, comentário, prefixo de bloco, metadata protegida, sintaxe desconhecida
e fonte opaca. Um token HTML pode ter lexemes internos de atributo sem deixar de
pertencer a um node protegido; a partição continua plana e sem duplicação.

Âncoras virtuais de source range vazio podem existir para layout/decoration,
mas não são `Lexeme`, não possuem bytes e não participam da cobertura física.

#### Node

`Node` é a árvore semântica. Nodes podem ter ranges de cobertura aninhados ou
sobrepostos por ancestralidade; nunca duplicam nem armazenam texto persistido.
Podem possuir lexemes e filhos. O vocabulário inclui `Paragraph`, `Heading`,
`Strong`, `Emphasis`, `Strike`, `Underline`, `Color`, `Highlight`, `Link`,
`InlineCode`, `ListItem`, `Task`, `Blockquote`, `Callout` e `Opaque`.

Sobreposição entre nodes só é válida por relação ancestral ou por annotations
não proprietárias explicitamente declaradas. Dois nodes proprietários irmãos
não podem possuir o mesmo lexeme. O ancestor mais restritivo domina todas as
capabilities dos descendentes.

#### ProjectionRun

`ProjectionRun` é saída visual derivada:

```text
ProjectionRun {
  lexemes: [LexemeId],
  semantic_path: [NodeId],
  graphemes: derived visible sequence,
  style: derived style,
  protection: derived capability summary
}
```

Ele referencia identidades, não possui fonte persistida e não pode ser editado.
Wrapping pode dividir um run para desenho, mas não muda graphemes, source map ou
nodes. A projeção é independente da viewport.

### 26.3 Tipos e line endings

São tipos normativamente incompatíveis:

- `Generation(u64)` identifica uma versão do `Draft`;
- `SourceOffset` é byte UTF-8 em uma generation;
- `SourceRange` é `[start,end)`, com ambos os limites em `is_char_boundary`;
- `ScalarPosition` é `(linha, coluna escalar)` compatível com o Draft raw;
- `GraphemeIndex` é índice EGC dentro de um run/bloco identificado;
- `DisplayColumn` é largura de célula terminal, somente para layout;
- `BlockId`, `NodeId` e `CaretSlotId` são identidades da generation.

Não haverá conversão implícita/genérica entre esses tipos nem `From<usize>`.
Funções nomeadas e validadas fazem `ScalarPosition -> SourceOffset`,
`SourceOffset -> ScalarPosition`, `SourceOffset -> CaretSlot` e
`GraphemeIndex -> CaretSlot`. A ponte do Draft soma os bytes das linhas anteriores
mais `\n` e localiza a fronteira escalar com `char_indices`; a volta valida UTF-8
antes de contar escalares. Nenhuma API aceita offset sem generation.

Line endings são lexemes físicos. `LF` e `CRLF` permanecem byte-idênticos e não
são normalizados pela projeção, transição de modo ou patch não relacionado. Para
layout, ambos representam uma quebra lógica; para source map, `LF` ocupa um byte
e `CRLF` dois. Um patch que cria nova quebra usa a convenção local determinística:
o line ending do bloco atual; se o documento ainda não possui quebra, `LF`.
Misturas preexistentes são preservadas. A canonicalização final já existente no
Core para terminadores finais continua fora da projeção e não é redesenhada.

### 26.4 CaretSlot e RawBookmark

Afinidade binária deixa de ser o modelo. Uma fronteira visual contém zero, um ou
N slots ordenados:

```text
CaretSlot {
  id: CaretSlotId,
  generation: Generation,
  visual_boundary: (BlockId, GraphemeIndex),
  source_offset: SourceOffset,
  context_path: [NodeId],
  insertion_context: SemanticStyle,
  stickiness: Before | InsideStart | InsideEnd | After,
  capabilities: OperationCapabilities
}
```

A ordenação é do contexto exterior para o interior na abertura e do interior
para o exterior no fechamento. Left/Right navega entre fronteiras de grapheme e
seleciona o **slot canônico de texto**: o slot mais interno que contém o run
visível adjacente na direção do movimento. Isso mantém o estilo ao digitar no
começo/fim de um run. Um comando estrutural pode escolher explicitamente outro
slot; operação comum nunca adivinha profundidade pela coluna de display.

Em `<mark ...><span ...>abc</span></mark>`, antes de `a` existem, na mesma célula:

1. antes de `<mark>`, path vazio;
2. depois do open de `mark`, antes de `<span>`, path `[Highlight]`;
3. depois do open de `span`, antes de `a`, path `[Highlight, Color]`.

O terceiro é o slot canônico ao navegar para `a`; inserção herda highlight e
color. Backspace/Delete primeiro escolhem o grapheme visual adjacente e usam seu
ownership; formatting usa os nodes cobertos pela seleção, não a display column.

`RawBookmark` preserva posições que Visual não pode expor:

```text
RawBookmark {
  generation: Generation,
  anchor: SourceOffset,
  head: SourceOffset,
  direction,
  snapped_visual_anchor: Option<CaretSlotId>,
  snapped_visual_head: Option<CaretSlotId>
}
```

Markdown -> Visual guarda offsets raw exatos, inclusive dentro de delimitador,
tag, atributo, entidade, destino de URL, metadata ou opaque. O cursor visual faz
snap para o slot legal determinístico mais próximo, preferindo o lado da direção
de navegação e depois o anterior em empate. Visual -> Markdown sem mutação e na
mesma generation restaura anchor/head raw exatos. Toda mutação do Draft invalida
o bookmark; depois dela a volta usa os slots canônicos correntes. A mesma regra
vale para seleção. Trocar modo não toca Draft, history, pending nem active style,
e nunca normaliza a fonte.

### 26.5 Capabilities e gramática inicial

Capabilities são por operação:

```text
Insert | DeleteInside | DeleteBoundary | ReplaceSelection | Format |
Split | Join | ChangeAttribute | ToggleAtomicState
```

`Recognized`, `Projected`, `Atomic`, `Protected`, `SourceVisible` e `Opaque` são
classificações separadas. O perfil inicial é conservador:

Toda capability ausente da matriz da subfase corrente é normativamente negada.
Não existe concessão implícita por reconhecimento, projeção, tipo de node,
capability da GUI nem comportamento do editor raw. Uma capability concedida a
um node também não habilita outra operação ou seus descendentes. Em particular,
editar o texto de um Heading em B.4 não habilita Strong, Emphasis, Color,
Highlight nem outro Format dentro dele; cada mark só passa a operar depois de
seu gate B.5/B.6 e de permissão explícita para aquele contexto.

| Construção | Reconhecida/projetada | Operações visuais iniciais |
|---|---|---|
| texto de parágrafo | sim/sim | Insert, DeleteInside, ReplaceSelection, Split e Join |
| heading ATX H1-H6 | sim/sim; prefixo Protected | texto: Insert/DeleteInside/ReplaceSelection em B.4; Split/Join conforme §26.9; Format somente após o gate B.5/B.6 da capability inline e permissão explícita no contexto |
| `*abc*` emphasis | após prova B.5 | inicialmente SourceVisible/Protected; depois Insert/DeleteInside/Replace/Format |
| `**abc**` strong | após prova B.5 | idem |
| `***abc***` strong+emphasis | reconhecido só pela regra abaixo | inicialmente SourceVisible/Protected; habilitação própria B.5 |
| `~~abc~~` strike | vocabulário conhecido | SourceVisible/Protected até B.5 |
| `<u>abc</u>` underline canônico | vocabulário GUI verificado | SourceVisible/Protected até B.5 |
| inline code | reconhecido | SourceVisible/Protected; sem Split/Format até contrato de delimiter próprio |
| link | reconhecido somente se grammar provar label/destino | SourceVisible/Protected até B.7; destino sempre Protected |
| color/highlight HTML canônicos | reconhecidos | SourceVisible/Protected até B.6 |
| font size canônico | vocabulário GUI verificado | SourceVisible/Protected; sem ChangeAttribute nesta fase |
| imagem | vocabulário GUI verificado | Atomic, Protected, SourceVisible; nenhuma edição nesta fase |
| comentário | reconhecido | Protected e SourceVisible |
| fenced code | reconhecido | bloco Protected e SourceVisible |
| lista/task | reconhecida | SourceVisible/Protected até contratos B.7; checkbox sem toggle inicial |
| metadata de conclusão de task | reconhecida | invisível na leitura, mas Protected/SourceVisible no editor seguro |
| blockquote/callout | reconhecido | SourceVisible/Protected até B.7 |
| HTML desconhecido/malformado | não semântico | Opaque, Protected e SourceVisible |

Reconhecimento da GUI prova vocabulário, nunca editabilidade ou round-trip. Não
se declara `<strong>` como sintaxe canônica: `<u><strong>abc</strong></u>` fica
Opaque/SourceVisible até evidência e contrato próprios.

#### Delimiter runs suportados

O subconjunto de B.5 só reconhece runs balanceados na mesma linha, fora de code,
HTML, escape e opaque, sem whitespace logo dentro dos delimitadores:

- `*abc*` -> `Emphasis`;
- `**abc**` -> `Strong`;
- `***abc***` -> um node composto com ownership único do run e semantic path
  `[Strong, Emphasis]`; não são dois pares editáveis independentes;
- `\*` e `\**` permanecem texto escapado, com backslash lexeme;
- marks adjacentes são nodes irmãos, mesmo quando semanticamente iguais;
- nesting só é habilitado quando a tokenização interna produz árvore balanceada
  e ownership único;
- `****`, runs vazios, width acima de três, mismatch, unbalanced e casos cuja
  precedência não seja decidida por estas regras ficam literais SourceVisible.

Isso não pretende implementar CommonMark completo. Ambiguidade falha fechada.

Links exigem label com escapes/nesting reconhecido e destino com parênteses
balanceados e escapes, por isso `[a **bold** label](https://example.com/a_(b))`
tem label semanticamente editável somente em B.7 e destino Protected. O parser
atual de `inline.rs`, que termina no primeiro `)`, não satisfaz esse contrato.

### 26.6 Planejamento de seleção e SourceTransaction

Seleção visual resolve primeiro anchor/head em slots e produz `SelectionPlan`
com slices de conteúdo, nodes proprietários, delimitadores relacionados e
rewrite envelope candidato. Planejamento não muda estado. Só depois um comando
produz todos os patches:

O intervalo visual de uma seleção não vazia é semiaberto `[start,end)`, em ordem
de EGCs do `VisualDocument`; `start = min(anchor,head)` e
`end = max(anchor,head)`, preservando a direção separadamente. Ambos precisam ser
fronteiras EGC válidas da mesma `Generation`. EGC que termina exatamente em
`start` e EGC que começa exatamente em `end` não estão selecionados. Os
`CaretSlotId` exatos de anchor/head são preservados para contexto, mas não mudam
quais EGCs pertencem ao intervalo.

Para decisão mutante, cada EGC selecionado fornece seu `semantic_path` de marks
inline e sua protection efetiva. O resultado é calculado nesta ordem, sem
discrição do planner:

1. qualquer EGC/range selecionado Protected, Opaque, SourceVisible ou metadata
   sem ownership -> `Refusal`;
2. boundary que não é EGC, generation stale ou ownership inconsistente ->
   `Refusal`;
3. todos os EGCs selecionados têm exatamente o mesmo inline-mark path -> operação
   permitida somente se cada node/contexto concede explicitamente a capability;
4. caso especial whole-leaf-node: seleção coincide exatamente com o visual range
   de um único mark sem boundary de mark descendente e a capability possui
   contrato explícito de cleanup de seus open/close lexemes -> permitido;
5. qualquer outro conjunto de paths, inclusive plain+mark, marks adjacentes,
   tipos diferentes, parte de nested stack ou outer mark contendo descendant
   marcado -> `Refusal`.

Um endpoint que apenas toca uma fronteira não conta como cruzamento: valem
somente os EGCs em `[start,end)`. Seleção vazia (`start == end`) não é
`DeleteSelection`, `ReplaceSelection` nem `FormatSelection`; vira comando de
caret e usa o `CaretSlot` escolhido e sua capability. Não há caret dentro de
grapheme multi-code-point, portanto esta álgebra nunca seleciona parte de EGC.

| Caso na arquitetura inicial | Delete/Replace | Format | Copy visual |
|---|---|---|---|
| somente plain text, capability concedida | permite | somente mark cujo gate foi concedido | permite |
| parte de um único mark, path constante e capability concedida | permite | permite segundo capability | permite |
| exatamente um mark leaf inteiro, cleanup concedido | permite com cleanup atômico | permite segundo capability | permite |
| começa fora e termina dentro de mark | recusa | recusa | permite |
| começa dentro e termina fora de mark | recusa | recusa | permite |
| plain + mark inteiro + plain | recusa | recusa | permite |
| dois marks adjacentes, iguais ou diferentes | recusa | recusa | permite |
| outer mark com apenas parte de descendant nested | recusa | recusa | permite |
| outer mark inteiro contendo descendant mark | recusa sem capability futura de subtree | recusa | permite |
| endpoint toca boundary, mas EGCs selecionados têm um único path | aplica a regra desse path | aplica a regra desse path | permite |
| seleção contém Protected/Opaque/SourceVisible/raw | recusa | recusa | permite para toda seleção EGC válida, copiando a projeção exata |
| caret vazio exatamente em boundary com N slots | não é seleção; comando usa slot atual | altera active override somente se concedido | texto vazio |

```text
SourceTransaction {
  generation: Generation,
  patches: [SourcePatch],        // ordenados, disjuntos, UTF-8 válidos
  resulting_cursor_anchor,
  resulting_selection_anchor,
  history_group: OneCommand
}
```

- `DeleteSelection`: remove conteúdo visível e somente cleanup estrutural
  obrigatório (por exemplo wrapper que ficaria sem conteúdo).
- `ReplaceSelection`: plano validado de delete mais slot determinístico de
  inserção e estilo herdado do head na direção da seleção; é uma transação só.
- `FormatSelection`: mantém conteúdo e só transforma wrappers dentro do envelope.
- `CopyVisualSelection`: para toda seleção EGC válida da generation corrente,
  concatena exatamente os graphemes dos `ProjectionRun` em `[start,end)` e
  sucede; não exige ownership. Em SourceVisible copia os caracteres efetivamente
  projetados, inclusive spelling raw visível; lexeme invisível contribui zero
  grapheme. Somente generation/boundary inválida produz `Refusal`.
- cópia raw/source é um comando futuro explícito.

Uma seleção mutante que cruza parcialmente uma fronteira de mark inline é sempre
`Refusal` na arquitetura inicial. Isso inclui começar fora e terminar dentro,
começar dentro e terminar fora, entrar/sair de apenas parte de uma pilha nested
ou reunir contextos sem selecionar boundaries completos compatíveis. A regra
vale para `DeleteSelection`, `ReplaceSelection`, `FormatSelection` e qualquer
comando mutante futuro baseado nesta seleção. Não há exceção dependente da
capacidade do planner: uma transformação para uma classe exata de boundary só
pode ser introduzida por capability posterior, normativa e separadamente
autorizada. `CopyVisualSelection` continua permitido porque não muta fonte.

Seleção integralmente contida em um único contexto semântico editável pode ser
mutada quando a capability correspondente está ativa. Selecionar todo o conteúdo
visual de um único mark também pode ser mutado quando seu contrato possui ambos
os delimitadores e aplica cleanup dentro do rewrite envelope. Na ausência de
regra explícita para múltiplos nodes irmãos completos, a mutação é recusada.
`ReplaceSelection` aceita herança pelo slot canônico do head somente depois que
essas regras aprovarem a seleção; nunca usa união de estilos.

Exemplos normativos: em `x **ab**`, selecionar `x a` recusa Delete, Replace e
Format; em `**ab** x`, selecionar `b x` produz a mesma recusa. Copy sucede nos
dois casos. Em `**a *bc* d**`, seleção que contém conteúdo apenas Strong e entra
ou sai parcialmente de Emphasis também recusa. Em `**abc**`, selecionar apenas
`b` é permitido quando Strong estiver autorizado; selecionar todo `abc` pode
remover conteúdo e delimitadores possuídos em uma SourceTransaction.

Se qualquer mutação intersecta `Protected`, `Opaque`, `SourceVisible`, metadata
sem ownership comprovado ou boundary parcial proibido, a operação inteira é
`Refusal`. Refusal preserva Draft, history, cursor, seleção e active style
byte/valor-idênticos.

Todo patch é validado antes do checkpoint. O Draft aplica do maior offset para o
menor como uma operação atômica; falha de qualquer validação aplica zero patches.

### 26.7 Rewrite envelope e ordem canônica

A precedência normativa é:

1. preservar todo byte fora do rewrite envelope aprovado;
2. escolher o menor ancestral semântico que contenha todos e somente os nodes
   que precisam de mudança estrutural;
3. normalizar somente dentro dele;
4. nunca ampliá-lo só para embelezar/coalescer sintaxe equivalente;
5. equivalência fora do envelope não autoriza reescrita.

Para apply/clear de mark, o envelope é o menor conjunto de runs intersectados e
seus delimitadores possuídos. Para change/clear color ou highlight, é o menor
node da espécie que intersecta a seleção mais os splits indispensáveis. Apagar o
último grapheme usa o node vazio e seus open/close lexemes como envelope de
cleanup. Um sibling não intersectado permanece fora.

Assim, em `<span red>a</span><span red>b</span>`, editar apenas o primeiro node
mantém o segundo byte-idêntico. “Wrapper máximo” significa máximo **dentro do
envelope**, nunca global. Adjacent equal marks fora dele não são coalescidas.

Para fonte recém-gerada ou integralmente reconstruída dentro do envelope, a ordem
externa -> interna é: `Highlight`, `Color`, `Underline`, `Strike`, `Strong`,
`Emphasis`. Essa ordem é convenção Note-it da TUI para fonte nova, não afirmação
de CommonMark. InlineCode é exclusivo e não combina com essas marks. Fonte
preexistente fora do envelope conserva ordem/grafia. Seleção com ordem mista é
reescrita só se um único envelope local balanceado for provado; caso contrário a
formatação é recusada.

### 26.8 Active typing style e undo/redo

`effective_style` vem do `context_path` do slot. `active_style_override` é estado
transitório explícito escolhido pelo usuário; não é fonte nem parte do histórico.

- digitar sem override usa effective style;
- selecionar formato sem seleção define/substitui o override;
- Left/Right dentro do mesmo run preserva override; cruzar para outro semantic
  path, clicar outro caret, trocar bloco ou entrar em SourceVisible limpa-o;
- troca Visual/Markdown suspende o override e o restaura apenas se volta na mesma
  generation e mesmo context path; mutação raw o invalida;
- typing-over-selection herda o slot canônico do head, salvo override explícito;
- seleção mista mostra `Mixed`; uma ação explícita aplica/remover mark por
  envelope, mas `Mixed` sozinho não muda override;
- ação semanticamente no-op não cria patch, history ou pending.

Undo/redo restaura `EditorSnapshot`: fonte, `ScalarPosition` raw de cursor,
seleção raw e âncoras de source/context visual. O modo de edição não faz parte do
history e permanece o modo corrente; a projeção é refeita e slots são resolvidos.
O active override também não faz parte: após undo/redo ele é limpo e o estilo é
recalculado do slot. Isso evita estado parcialmente histórico.

Um `VisualCommand` produz exatamente um entry, ainda que tenha vários patches.
Além de no máximo 200 entradas, history tem budget configurável de bytes; ao
exceder qualquer limite, remove snapshots mais antigos completos. O estado atual
e o passo imediatamente anterior, quando houver memória para a operação, não são
removidos no meio de uma transação. B.3 deve medir notas grandes e propor o valor
numérico antes de produção; nenhuma implementação pode manter sem limite
`200 * note_size`.

### 26.9 Split, Join e boundaries

| Construção inicial | Enter início | Enter meio | Enter fim | Join/Backspace/Delete de boundary |
|---|---|---|---|---|
| parágrafo | parágrafo vazio antes | dois parágrafos | parágrafo vazio depois | une dois parágrafos; uma transação |
| heading | parágrafo antes; heading fica | dois headings do mesmo nível | parágrafo depois | só B.4: heading+paragraph e heading+heading por regras testadas |
| mark inline habilitada | slot externo no extremo não herda; slot interno herda | fecha antes da quebra e reabre no segundo bloco | slot interno fecha e cria próximo bloco sem mark; externo não toca mark | não remove delimitador isolado; cleanup por ownership |
| inline code | sem Split | sem Split | sem Split | SourceVisible/Protected |
| lista/task/quote/callout | sem Split inicial | sem Split inicial | sem Split inicial | SourceVisible/Protected até B.7 |
| Opaque/SourceVisible | nenhum Enter visual | nenhum | nenhum | nenhum |

Em parágrafo, styles ativos no caret podem ser reabertos no segundo parágrafo
somente para marks habilitadas e balanceadas. Marks Markdown não atravessam line
ending: a transaction insere closers, line ending e openers canônicos. Em heading
meio, ambos mantêm o nível. Heading início/fim segue a regra acima. DeleteInside
nunca implica DeleteBoundary; boundary exige capability própria.

### 26.10 Unknown e malformed

Fail-closed é normativo:

#### Algoritmo normativo de limite HTML

O reconhecimento e os limites dependem somente dos bytes da mesma `Generation`.
Offsets abaixo são `SourceOffset` UTF-8; nenhuma decisão consulta renderer,
viewport, estilo visual ou heurística de linha.

1. Um candidato HTML começa no byte `<`. `<!--` inicia comentário. Fora desse
   caso, open/close tag exige nome começando por letra ASCII e continuando apenas
   com ASCII alfanumérico, `-`, `:`, `_` ou `.`. Close tag lexical é exatamente
   `</`, nome, zero ou mais whitespace ASCII e `>`; qualquer outro tail torna o
   close malformed. `<` cujo byte seguinte não seja letra ASCII, `/` seguido de
   letra ASCII ou `!` de `<!--` é texto literal.
2. O scanner de tag percorre bytes nos estados `Unquoted`, `SingleQuoted` e
   `DoubleQuoted`. Aspa abre/fecha somente seu próprio estado; `>` encerra o tag
   somente em `Unquoted`. Portanto `>` dentro de atributo quoted não encerra o
   token. Line ending não encerra token. Se um candidato não alcança `>` antes
   de EOF, o span protegido é `[candidate_start, source_len)`.
3. Nome de tag é comparado por ASCII lowercase. Open tag é autocontido se o
   último byte não whitespace ASCII anterior ao `>` for `/` em `Unquoted`, ou
   se o nome pertencer ao allowlist de void elements `area`,
   `base`, `br`, `col`, `embed`, `hr`, `img`, `input`, `link`, `meta`, `param`,
   `source`, `track`, `wbr`. Esses dois casos não exigem close.
4. Para open tag não autocontido, a busca do close é document-bounded, do fim do
   opener até EOF, com o mesmo lexer quote-aware. Uma pilha contém todo open tag
   não void/autocontido encontrado. Close tag só desempilha quando seu nome
   ASCII-lowercase é exatamente o nome no topo. Comentários completos são um
   token e seu conteúdo não participa da pilha. Texto, Markdown, delimitadores,
   fences e sequências `<...>` que não sejam tags lexicais não têm significado
   para matching HTML.
5. O close do candidato inicial é provado somente quando a pilha bem formada
   volta a ficar vazia. O span do elemento é então `[candidate_start,
   matching_close_end)`, incluindo o `>` final. Para elemento desconhecido, esse
   span inteiro é um único ancestor Opaque. Para elemento canônico, só a grammar
   canônica própria pode atribuir semântica/editabilidade dentro do mesmo span.
6. Close divergente do topo, close órfão, atributo quoted não terminado, opener
   interno não terminado ou EOF com pilha não vazia torna o candidato inicial
   não fechado: seu span é `[candidate_start, source_len)`. Um close órfão fora
   de qualquer candidato inicia por si uma região malformed
   `[orphan_close_start, source_len)`.
7. Dentro de região Opaque nenhum byte é reclassificado como Markdown, mark,
   descendant editável ou novo bloco. Lexemes ainda particionam todos os bytes,
   mas as capabilities efetivas de todos os descendentes são Protected.

O algoritmo é total: dado o mesmo byte string e `Generation`, produz o mesmo
início e o mesmo fim. Scanning é sempre document-bounded; line ending nunca é
condição de parada.

| Fonte UTF-8 | Span Opaque/Protected normativo | Razão |
|---|---|---|
| `<x>abc</x>` | `[0,10)` | unknown balanceado single-line, incluindo close |
| `<x>a\nb</x>` | `[0,10)` | unknown balanceado multiline; newline não encerra |
| `<x>a\n# h` | `[0,8)` = `[0,source_len)` | open sem close; Markdown posterior não é semântico |
| `<x><y>z</y></x>` | `[0,15)` | nested bem formado; close exterior esvazia a pilha |
| `<x><y></x>` | `[0,10)` = `[0,source_len)` | close diverge do topo; toda a região falha fechada |
| `<x a=">">ok` | `[0,11)` = `[0,source_len)` | `>` quoted não fecha opener; não há `</x>` |
| `<x>**b**</x>` | `[0,12)` | Markdown-looking permanece bytes internos opacos |
| `pré <x>ç **b**</x> pós` | `[5,20)` | offsets são bytes; Unicode externo/interno é preservado |
| `<span data-note-it-color="#DC2626">x` | `[0,source_len)` | canonical opener sem close provado |

Nos dois primeiros casos o span termina exatamente no byte após `>` do close.
Nos casos não fechados termina exatamente em `source_len`, isto é, EOF.

- HTML desconhecido balanceado: do open ao close correspondente é um único
  ancestor `Opaque`; descendente canônico não recupera editabilidade;
- HTML desconhecido ou canônico com open tag reconhecido lexicalmente, mas sem
  close correspondente provado pela grammar lossless: um único ancestor
  Opaque/SourceVisible/Protected começa no primeiro byte do open tag e segue até
  EOF. Não existe limite por linha, inferência de provável linha única nem
  comportamento do renderer capaz de encurtar a região; descendente canônico
  não recupera editabilidade;
- HTML canônico malformado: aplica a mesma regra até EOF, sem reparo, close
  sintético ou normalização;
- budget de nesting do projetor: 32. Ao tentar abrir o nível 33, o ancestor que
  começou a região não resolvida até seu close comprovado, ou EOF, vira Opaque;
- comentário `<!--` sem `-->`: Protected/SourceVisible até EOF;
- fence sem fechamento: Protected/SourceVisible da abertura até EOF;
- entidade só decodifica o allowlist provado (`amp`, `lt`, `gt`, `quot`, `apos`,
  `nbsp` inicialmente); desconhecida permanece texto literal e editável somente
  como seus caracteres visíveis;
- `<`/`>` que não formam tag pela grammar são texto literal, nunca ocultados.

Princípio comum: para construção suportada cuja grammar admite continuação em
mais de uma linha e exige fechamento, a ausência de close provado fixa EOF como
único limite seguro. Isso também rege comentário e fence não terminados. Em
contraste, caracteres `<` e `>` que não formam open tag conforme a grammar
lexical permanecem texto literal e não tornam o restante opaco.

Logo, `<custom-widget>hello\nparagraph` sem `</custom-widget>` e
`<span data-note-it-color="#DC2626">hello\nparagraph` sem `</span>` são
Opaque/SourceVisible/Protected desde `<` até EOF; `paragraph` não é editável.
Não há reparo ou close sintético. Já um custom element com close correspondente
provado é Opaque somente do open ao close exterior, ainda dominando descendentes
canônicos. O texto `2 < 3 and 4 > 1` não contém open tag lexical e permanece
literal.

Em `<custom-widget foo="bar"><span data-note-it-color="#DC2626">hello</span></custom-widget>`,
os lexemes cobrem tudo, o node Opaque exterior possui a região e domina o Color:
todo o trecho é source-visible sem slot editável interno.

### 26.11 Exemplos normativos

Para `**abc**`:

```text
Lexemes: StrongOpen[0..2], Text[2..5], StrongClose[5..7]
Node: Strong coverage[0..7], owns open/close, child Text
ProjectionRun: "abc", path[Strong], lexeme Text
```

Antes de `a` há slot externo offset 0 e interno offset 2; após `c`, interno
offset 5 e externo offset 7. Navegação pelo texto escolhe internos. Ao apagar o
último grapheme, o envelope cobre open, conteúdo e close; uma única transaction
substitui `[0,7)` por vazio. `****` já existente não é Strong vazio: é literal
SourceVisible.

`***abc***` usa um node composto de ownership único somente após B.5 provar a
regra; até lá é SourceVisible. `&amp;` é um lexeme fonte de cinco bytes, um
grapheme visual e carets somente antes/depois; Delete remove os cinco bytes, e
nenhuma edição preserva uma metade.

No nested mark/color do §26.4 existem os três slots enumerados. Selecionar `ab`
escolhe somente lexeme Text correspondente. Mudar color cria splits dentro do
menor Color envelope; limpar highlight inclui somente o Highlight necessário.

No link `[a **bold** label](https://example.com/a_(b))`, label e destino têm
owners distintos; destination e delimitadores são Protected. Antes de B.7, todo
link é SourceVisible. Depois, label pode receber carets e destination continua
sem slots visuais.

`e` + U+0301 e `👨‍👩‍👧‍👦` são cada um um EGC: podem ter vários escalares/bytes,
mas apenas carets nas pontas e remoção integral. Não há normalização NFC/NFD.
CRLF é lexeme de dois bytes e permanece CRLF. Cursor raw dentro de um atributo
HTML vira `RawBookmark` exato e caret visual snapped; voltar sem mutação restaura
o byte exato.

Uma seleção que começa fora e termina dentro de Strong pode ser copiada
visualmente, mas Delete/Replace/Format são sempre `Refusal` nesta arquitetura.
O mesmo vale no sentido dentro -> fora e para entrada/saída parcial de mark
nested. Seleção inteiramente dentro de Strong pode ser mutada depois de B.5;
selecionar seu conteúdo visual inteiro pode incluir cleanup dos delimitadores
possuídos em uma transação. Wrappers de color iguais adjacentes não são unidos
se o segundo estiver fora do rewrite envelope.

Fence ou comentário não terminado são SourceVisible/Protected até EOF, sem caret
visual interno e com round-trip físico exato.

### 26.12 Dependência Unicode

Cursor Visual requer implementação correta de extended grapheme clusters; criar
algoritmo próprio é proibido. B.1 não expõe cursor de grapheme e não precisa de
dependência nova. B.2 pode propor dependências diretas `unicode-segmentation` e,
se Ratatui não expuser o necessário, `unicode-width`, somente com autorização
explícita contendo versões exatas, licença, compatibilidade MSRV, impacto no lock
e justificativa. A presença transitiva atual não é autorização nem API estável.

### 26.13 Gates de performance

B.1 pode reprojetar integralmente por correção, pois não é interativo. Antes de
ampliar edição interativa, benchmarks devem registrar tempo, alocações/memória,
bytes/nodes reprocessados e número de fallbacks integrais para:

- documentos de 1 KB, 100 KB e 1 MB;
- 20.000 linhas;
- uma linha de 100.000 caracteres;
- séries de insert/delete no início, meio e fim;
- alternância repetida Visual/Markdown;
- 200 undo/redo;
- nesting no limite e inputs adversariais;
- quantidade de full projections durante uma série de typing.

É proibido: full projection incondicional por tecla em documento grande; source
map por célula terminal em linha enorme; prefix scan repetido O(n²); nesting sem
limite; history sem budget. Invalidação local deve registrar seu safe block e
ampliação; fallback integral é correto em ambiguity, mas mensurado. Valores
numéricos de latência/memória só serão aceitos após baseline reprodutível em B.P.

Performance é gate recorrente, não fechamento adiado. Ao concluir B.1, o
relatório registra baseline informativo de projeção para 1 KB, 100 KB, 1 MB,
20.000 linhas, linha de 100.000 caracteres e nesting adversarial; full projection
continua permitida porque não há edição interativa. Antes de autorizar B.3, um
gate obrigatório demonstra que a arquitetura interativa proposta não depende
de full projection incondicional por tecla em notas grandes, prefix rescanning
O(n²), mapa por célula ou history sem limite. B.3 não começa se qualquer desses
modos de falha permanecer.

Antes de ampliar B.5, B.6 ou B.7, as medições se repetem após cada aumento
material de complexidade. Cada relatório registra tamanho, operação, tempo,
memória quando mensurável, bytes/nodes reprocessados e contagem de fallbacks para
projeção integral. B.P permanece fechamento agregado de performance, regressão
e production readiness; **B.P não substitui nenhum gate anterior**.

### 26.14 Sequência obrigatória da 5.0D.4B

Sem alterar o roadmap, a implementação deve ser autorizada separadamente:

1. **5.0D.4B.1 — Lossless projection foundation:** tipos de source/generation,
   Lexemes, Nodes, cobertura, grammar e opaque; round-trip exato; nenhuma UI
   visual editável e nenhuma dependência de grapheme.
2. **5.0D.4B.P0 — Baseline de projeção:** imediatamente depois de B.1, registra
   os cenários informativos do §26.13. Não bloqueia B.2, que continua read-only.
3. **5.0D.4B.2 — Read-only visual source map:** EGC, CaretSlots, paths,
   RawBookmarks, VisualDocument imutável e transição de modos; nenhuma mutação
   visual. É o primeiro gate que pode pedir dependência Unicode.
4. **5.0D.4B.P1 — Gate pré-interativo:** obrigatório depois de B.2 e antes de
   autorizar B.3; precisa excluir os quatro modos de falha proibidos do §26.13.
5. **5.0D.4B.3 — Minimal visual editing:** plain text/parágrafo,
   SourceTransaction, insert/delete/replace e integração atômica com history.
6. **5.0D.4B.4 — Heading/block boundaries:** heading, Enter, Split/Join e
   Backspace/Delete de boundary.
7. **5.0D.4B.P2 — Gate pré-inline:** mede B.3/B.4 e deve passar antes de B.5.
8. **5.0D.4B.5 — Markdown inline capabilities:** Strong, Emphasis, Strike,
   Underline e InlineCode individualmente; nenhum é liberado em lote.
9. **5.0D.4B.P3 — Gate pré-HTML:** mede o aumento de B.5 e deve passar antes de
   B.6.
10. **5.0D.4B.6 — Canonical HTML formatting:** Color, Highlight, envelopes,
    ordem canônica, splits e coalescing local.
11. **5.0D.4B.P4 — Gate pré-blocos estruturados:** mede B.6 e deve passar antes
    de B.7.
12. **5.0D.4B.7 — Structured blocks:** links, listas, tasks, blockquotes e
    callouts apenas com contratos próprios.
13. **5.0D.4B.P — Performance closure:** depois de B.7, consolida resultados,
    budget, invalidação incremental e regressões; não substitui P0-P4.
14. **5.0D.4B.R — Adversarial/regression closure:** properties, malformed,
    Unicode, TestBackend, PTY, conflito, recovery, sinais e terminal.

Cada P1-P4 é pré-condição formal da etapa seguinte indicada: essa etapa não pode
ser autorizada, iniciada nem considerada conforme enquanto o gate estiver
pendente ou falhar. Uma subfase concluída não autoriza automaticamente a
seguinte, e cada checkpoint também exige autorização separada.

### 26.15 Contrato de testes corrigido

5.0D.4B deve provar por máquina:

1. ranges físicos de Lexeme são ordenados;
2. ranges físicos de Lexeme nunca se sobrepõem;
3. ranges físicos de Lexeme não têm gaps;
4. Lexemes cobrem exatamente `0..source_len`;
5. concatenar seus slices reconstrói os bytes originais exatos;
6. todo limite de SourceRange é uma fronteira UTF-8 válida;
7. todo CaretSlot visual cai em uma fronteira EGC;
8. uma fronteira visual aceita zero, um ou N slots ordenados;
9. nenhum slot editável existe dentro de Protected/Opaque;
10. ancestor desconhecido impede editabilidade de todo descendente;
11. patches de SourceTransaction são generation-correct, UTF-8 válidos,
    ordenados e disjuntos;
12. bytes fora do rewrite envelope são idênticos;
13. Refusal preserva Draft, history, cursor, seleção e active style;
14. um comando cria exatamente uma transação de undo;
15. troca de modo sem edit não muda source, history ou pending;
16. RawBookmark restaura offsets e direção exatos na mesma generation;
17. remoção de grapheme nunca corta um cluster;
18. CRLF, LF, whitespace, escapes e grafia da fonte não são normalizados;
19. decorations nunca serializam;
20. VisualDocument, CaretSlot e SourceTransaction stale são recusados;
21. a projeção independe da viewport;
22. nenhuma mutação visual bypassa Draft;
23. o parser apresentacional atual nunca é usado como prova de editabilidade;
24. open tag HTML canônico/desconhecido sem close provado protege do open até EOF;
25. texto `2 < 3 and 4 > 1` não cria região Opaque;
26. seleção mutante que cruza parcialmente boundary de mark tem resultado único:
    `Refusal`, sem mudança de Draft, history, cursor, seleção ou active style;
27. a ausência de uma capability na matriz da subfase equivale a deny.

Fixtures obrigatórias incluem delimiter runs, nesting, entities, links com
parênteses balanceados, unknown/malformed HTML, fences/comments não terminados,
NFC/NFD, ZWJ, flags, CJK, line endings mistos e limites de tamanho. Property
tests geram UTF-8 e verificam cobertura/round-trip; TestBackend e PTY entram
somente quando há UI.

As tabelas R2 obrigatórias incluem:

- `<custom-widget>hello\nparagraph` e o `span` canônico equivalente sem close:
  Opaque começa no `<`, termina em EOF e a segunda linha não é editável;
- `2 < 3 and 4 > 1`: todo o texto é literal, sem Opaque;
- em `x **ab**`, seleção visual `x a`: Delete, Replace e Format recusam; Copy
  sucede;
- em `**ab** x`, seleção visual `b x`: a mesma matriz de recusa/cópia;
- em `**abc**`, seleção `b`: mutação apenas com Strong autorizada; seleção de
  todo `abc`: cleanup integral apenas com contrato Strong ativo.
- HTML quote-aware e matching: single/multiline balanceado, `>` quoted, nesting
  bem formado, close divergente, Unicode antes/dentro/depois e Markdown-looking
  interno devem produzir exatamente os spans da tabela do §26.10;
- seleção deve cobrir todas as linhas da tabela do §26.6, inclusive marks
  adjacentes iguais/diferentes, outer+nested, boundary apenas tocado, caret vazio
  e EGC multi-code-point.

### 26.16 Separação do parser legado e fronteiras preservadas

`markdown.rs`, `inline.rs` e as buscas textuais atuais de `formatting.rs` são
renderer apresentacional/helpers do editor raw. Não são fundação autorizada do
projector lossless. A incapacidade atual de `inline.rs` balancear parênteses no
destino de link é evidência concreta. Primitivas só poderão ser compartilhadas
depois de equivalência e losslessness provadas, nunca por semelhança de saída.

Persistência permanece fora do desenho: `Draft.text()`, `mutation_for`,
`ReplaceBody`/`ClearBody`, canonical no-op, revision original,
`authority::perform_at`, ausência de retry, conflict, recovery, sinais e terminal
lifecycle não mudam. Core, GUI, schemas, migrations e formato persistido ficam
intocados. Raw Markdown é fallback integral permanente.

### 26.17 Mapeamento dos bloqueios da revisão

| Bloqueio independente | Correção normativa R1 |
|---|---|
| ranges físicos vs. semânticos | Lexeme/Node/ProjectionRun separados (§26.2) |
| afinidade binária | N CaretSlots com context path (§26.4) |
| cursor raw perdido | RawBookmark por generation (§26.4) |
| unidades misturadas/CRLF | newtypes, conversões nomeadas e line-ending policy (§26.3) |
| grammar/capability ampla | matriz conservadora por operação (§26.5) |
| delimiter runs | subconjunto normativo/fail-closed (§26.5) |
| OwnedSlice insuficiente | SelectionPlan e comandos separados (§26.6) |
| patch mínimo vs. coalescing | rewrite envelope e precedência (§26.7) |
| ordem de marks | ordem completa para fonte nova (§26.7) |
| active style | lifecycle e recomputação (§26.8) |
| Enter/Join | matriz por construção (§26.9) |
| malformed/unknown | safe limits e ancestor dominance (§26.10) |
| sintaxe GUI omitida | classificação explícita Protected/SourceVisible (§26.5) |
| Unicode | gate de dependência em B.2 (§26.12) |
| undo/memória | snapshot lógico e budget por bytes (§26.8) |
| performance vaga | cenários/métricas/proibições (§26.13) |
| dual authority implícita | VisualDocument formalmente imutável (§26.1) |
| renderer usado como parser | rejeição explícita (§26.16) |
| 5.0D.4B ampla | nove gates separados (§26.14) |

### 26.18 Status da arquitetura

Esta R1 corrige documentação somente. Não implementa nenhuma subfase, não aprova
a própria arquitetura e não autoriza código. O veredito cabe a nova revisão
independente.

**5.0D.4A.R1 READY FOR INDEPENDENT RE-REVIEW**

## Correção final de arquitetura — Fase 5.0D.4A.R2

Esta R2 é estreita e normativa. Ela substitui apenas as quatro ambiguidades
remanescentes: HTML aberto sem close agora protege invariavelmente até EOF;
seleção mutante que cruza parcialmente mark agora recusa invariavelmente;
capability não concedida é deny; e performance passa a checkpoints P0-P4
intercalados antes da edição interativa e de cada ampliação, mantendo B.P como
fechamento agregado.

Permanecem inalterados todos os demais contratos aceitos da R1: fonte única no
Draft, VisualDocument imutável por generation, Lexeme/Node/ProjectionRun,
cobertura byte-exact, tipos de posição, RawBookmark, N CaretSlots, stale
rejection, delimiter grammar, rewrite envelope, ordem canônica, active style,
Split/Join, Unicode, undo/history budget, proibição do parser legado,
revision/authority/conflict/recovery/terminal, fallback raw e fronteiras futuras
de matemática e flashcards.

Esta correção não implementa B.1 nem qualquer etapa posterior e não aprova a
própria arquitetura. O veredito permanece reservado à revisão independente.

**5.0D.4A.R2 READY FOR FINAL INDEPENDENT RE-REVIEW**

## 27. Fase 5.0D.4A.R3 — correção normativa após revisão independente final

### 27.1 Status e precedência

A revisão independente final bloqueou a R2 com 3 BLOCKERs, 21 MAJORs e 13 MINORs.
Ela confirmou, por rastreamento manual linha a linha, que as nove linhas da tabela
normativa do §26.10 são reproduzidas exatamente pelos passos 1–7 como escritos, e
que a disciplina de pilha do passo 4 é consistente com o passo 6. Os defeitos estão
nos casos que a tabela não alcança e em contratos vizinhos.

Esta R3 conserva toda a arquitetura aceita da R1/R2 — fonte única no `Draft`,
`VisualDocument` imutável por generation, `Lexeme`/`Node`/`ProjectionRun`, cobertura
byte-exact, tipos de posição, `RawBookmark`, N `CaretSlot`s, stale rejection,
rewrite envelope, ordem canônica, active style, Split/Join, Unicode, budget de
history, proibição do parser legado, revision/authority/conflict/recovery/terminal,
fallback raw e fronteiras de matemática e flashcards. Ela **substitui** os contratos
listados nas seções seguintes; onde R3 e R1/R2 divergirem, R3 prevalece.

O histórico R1 e R2 permanece no documento como registro. As seções 1–25 do
relatório original (a partir de "Relatório de arquitetura — Fase 5.0D.4A") são
**históricas** sempre que a R1, a R2 ou esta R3 restatem o mesmo contrato; em
particular o esboço `Segment`/`CaretMap` da seção 7, a seleção por `OwnedSlice` da
seção 9 e o modelo de afinidade binária da seção 14 estão superados e não são
implementáveis. Uma referência a "a tabela da seção 6" naquele relatório significa a
seção 6 **do relatório**, não a seção 6 deste documento.

### 27.2 B1 — `Generation` é monotônica por sessão

A R2 amarrava o incremento a um único evento, a `SourceTransaction` bem-sucedida.
`app.rs::reseat_draft` constrói um `Draft` novo após cada save, conflito, recovery ou
recarga; um contador por instância reinicia e um `VisualDocument` em cache passa a ser
"generation-correto" contra bytes de outra revisão. Undo que restaurasse uma
generation armazenada produz a mesma falha em quatro teclas.

> `Generation` é estritamente crescente e monotônica durante toda a sessão do
> editor, independentemente da origem da mudança. Incrementa em **toda** mutação
> dos bytes do `Draft`: `SourceTransaction`, tecla do editor raw, undo, redo,
> `insert_str`/paste, e substituição do `Draft` por um novo (reseat após save,
> após conflito, após recovery ou após recarga da nota). Undo e redo **nunca**
> restauram uma generation anterior: avançam o contador como qualquer outra
> mutação. O contador pertence à sessão, não à instância de `Draft`; construir um
> novo `Draft` não o reinicia. Nenhum `EditorSnapshot` armazena uma generation.

### 27.3 B2 — precedência de contexto antes de qualquer candidato HTML

Nada na R2 dizia se um fence ou um code span suprime a **iniciação** de um candidato
HTML, e o passo 4 apontava para o lado errado ao retirar dos fences todo significado
para matching. Consequência literal: uma nota com um bloco ```` ```html ```` contendo
`<div class="card">`, ou um parágrafo com `` `<div>` ``, torna Opaque/Protected todo
byte daquele `<` até EOF — em B.1, antes de qualquer capability de edição.

Novo passo 0 do §26.10, antes do passo 1:

> 0. Precedência de contexto. O lexer resolve, nesta ordem e antes de qualquer
>    candidato HTML: (i) fenced code — um fence aberto consome todos os bytes até
>    seu fence de fechamento correspondente ou, na ausência dele, até EOF;
>    (ii) code span inline com delimitador balanceado na mesma linha; (iii) escape
>    `\<`. Um byte `<` dentro de fenced code, de code span balanceado ou escapado
>    **não inicia candidato HTML** e é texto literal para todos os efeitos deste
>    algoritmo. Um `<` dentro de um code span cujo delimitador não fecha na mesma
>    linha não está protegido por esta regra e volta a ser candidato. Fora desses
>    três contextos, a varredura de candidatos segue nos passos 1–7.

Fixtures obrigatórias acrescentadas ao §26.15:

> - fence ```` ```html ```` contendo `<div class="card">` sem `</div>`: o bloco de
>   código é Protected/SourceVisible até seu fence de fechamento, e o parágrafo
>   seguinte permanece editável; nenhuma região Opaque é criada;
> - `` Use `<div>` aqui. ``: o code span é Protected/SourceVisible, o resto do
>   parágrafo permanece editável.

### 27.4 B3 — escopo de bloco na álgebra de seleção

As cinco regras do §26.6 são exaustivas por construção e estavam enunciadas somente
sobre inline-mark paths. Em `# T\n\npara`, uma seleção do interior do heading ao
interior do parágrafo tem path vazio em todos os EGCs selecionados: a regra 1 não
enxerga o prefixo `# ` porque ele é Protected mas **invisível** e não contribui EGC,
e a regra 3 **permite** a operação. O mesmo vale em B.3 para `ab\n\ncd`.

Nova regra 0 do §26.6, antes da regra 1, com as demais renumeradas:

> 0. Escopo de bloco. Se `start` e `end` não pertencem ao mesmo `BlockId`, a
>    seleção é multibloco. Uma seleção multibloco só é mutável quando **todos** os
>    seguintes valem, e é `Refusal` caso contrário: (i) cada bloco atravessado
>    concede a capability pedida; (ii) cada fronteira de bloco interna à seleção
>    concede `Join` ou `DeleteBoundary` para o par exato de construções envolvido,
>    segundo a matriz do §26.9; (iii) nenhum lexeme `BlockPrefix`, `Metadata` ou
>    `Protected` fica parcialmente contido na união dos ranges de fonte resultantes.
>    Os bytes de line ending entre blocos pertencem ao envelope apenas quando (ii)
>    autoriza o Join daquela fronteira, e então integralmente (CRLF é indivisível).
>    Na arquitetura inicial nenhum par com prefixo Protected concede (ii), logo toda
>    seleção multibloco que atravesse um heading, lista, task, quote, callout, fence
>    ou região Opaque é `Refusal`.

### 27.5 Correções do algoritmo de limite HTML

**M1 — span de candidato autocontido e de close malformed.** O passo 5 definia span
só para close provado e o passo 6 só para falha; `<br>abc` e `<x/>abc` não caíam em
nenhum dos dois. Acrescenta-se ao passo 3:

> Quando o candidato inicial é ele próprio autocontido — void do allowlist ou `/`
> imediatamente antes do `>` em `Unquoted` — o span do elemento é exatamente
> `[candidate_start, tag_end)`, onde `tag_end` é o byte seguinte ao `>`. Não há
> busca de close, a pilha permanece vazia e os bytes seguintes voltam à varredura
> normal.

e ao passo 6:

> Um close lexicalmente malformed — `</` seguido de nome e de qualquer tail que não
> seja apenas whitespace ASCII e `>` — é tratado como close órfão: inicia por si uma
> região malformed `[malformed_close_start, source_len)`.

**M2 — o teste de autocontenção era fail-open.** Um valor de atributo não quoted
terminado em `/` satisfazia o teste posicional, e
`<custom-widget data-src=a/>hello</custom-widget>` tornava `hello` texto editável
dentro de um elemento desconhecido. Substitui-se a primeira cláusula do passo 3:

> Open tag é autocontido se, em estado `Unquoted`, existir um byte `/` que seja o
> último byte não whitespace ASCII antes do `>` **e** que não faça parte de um valor
> de atributo não quoted — isto é, o byte imediatamente anterior a esse `/` deve ser
> whitespace ASCII, o último byte do nome do tag, ou a aspa que fechou um valor
> quoted. Um `/` que apenas termina um valor de atributo não quoted (`a=x/`) não
> torna o tag autocontido. A segunda condição, independente, é o nome pertencer ao
> allowlist de void elements.

**M3 — o budget de nesting passa a ser parte do algoritmo.** O bullet do §26.10 fica
revogado e vira o passo 4a:

> 4a. Budget de nesting. A profundidade contada é exclusivamente a da pilha de open
>     tags do passo 4. Ao encontrar um open tag não void/autocontido com a pilha já
>     em 32 elementos, o matching é abandonado imediatamente: o candidato inicial é
>     declarado não fechado e seu span é `[candidate_start, source_len)`, pela mesma
>     regra do passo 6. Não se continua a varredura para tentar provar um close. O
>     limite de aninhamento de nodes do projetor fora de HTML — marks Markdown e
>     blocos — é igualmente 32 e sua ultrapassagem torna literal SourceVisible o
>     node mais externo que excedeu, sem afetar HTML.

**M4 — comentário não terminado dentro de uma busca de close.** Substitui a frase
"Comentários completos são um token…":

> Comentários participam da varredura como token único. Um comentário completo
> (`<!--` … `-->`) é um token e seu conteúdo não participa da pilha. Um `<!--` sem
> `-->` consome todos os bytes até EOF: nenhum close tag dentro dele desempilha,
> e o candidato inicial em andamento termina com a pilha não vazia, recaindo no
> passo 6.

**m1 — o candidato inicial é empilhado.** O passo 4 passa a dizer explicitamente que
a pilha começa contendo o nome do candidato inicial; sem isso, somente a linha
`pré <x>ç **b**</x> pós` da tabela desambigua o algoritmo.

**m12 — sinal visível.** `if a <b and c> d` continua, corretamente, Opaque até EOF
pelo princípio comum, mas é caso de fixture obrigatória e a aplicação deve nomear a
região protegida ao abrir a nota, nunca silenciar.

### 27.6 M6 — as classificações são três eixos ortogonais

`Recognized`, `Projected`, `Atomic`, `Protected`, `SourceVisible` e `Opaque` eram
nomeados e nunca definidos, e a matriz os escrevia ora em pares, ora em triplas. Sem
definição, uma implementação passa as 27 propriedades e ainda assim torna `**abc**`
simultaneamente portador de caret e inselecionável.

> As classificações são três eixos ortogonais, e todo node/lexeme carrega os três:
> - **Reconhecimento**: `Recognized` (a grammar lossless atribuiu semântica) ou
>   `Opaque` (não atribuiu; os bytes são preservados sem interpretação);
> - **Visibilidade**: `Projected` (os delimitadores/atributos são invisíveis e só o
>   conteúdo é projetado) ou `SourceVisible` (a grafia literal da fonte é projetada
>   como graphemes, inclusive delimitadores);
> - **Proteção**: `Editable` (as capabilities concedidas na matriz da subfase valem),
>   `Atomic` (existe caret apenas nas pontas; a unidade é removida inteira ou não é
>   removida) ou `Protected` (nenhum slot editável interno; nenhuma mutação).
>
> `Opaque` implica `SourceVisible` e `Protected` em si e em todos os descendentes.
> `Protected` domina todos os descendentes. Nenhum node pode ser `Projected` e
> `Protected` ao mesmo tempo em B.1–B.7: toda região protegida é mostrada como
> fonte, para que o usuário veja o que não pode editar.

### 27.7 M7 — entidades entram na matriz de capabilities

`formatting.rs::escaped_typed` já grava `&amp;`, `&lt;` e `&gt;` na fonte quando o
usuário digita `&`, `<` ou `>` dentro de um trecho colorido ou marcado, de modo que
notas produzidas pela própria 5.0D.3 contêm entidades. O §26.11 mandava que Delete
removesse os cinco bytes; o §26.5 mais a propriedade 27 negavam a operação por
ausência de linha na matriz. Acrescentam-se duas linhas:

| Construção | Reconhecida/projetada | Operações visuais iniciais |
|---|---|---|
| entidade do allowlist (`amp`, `lt`, `gt`, `quot`, `apos`, `nbsp`) | sim/sim; um grapheme projetado, lexeme de N bytes | `Atomic` desde B.3: sem slot interno; `DeleteInside` e `ReplaceSelection` removem ou substituem o lexeme inteiro; `Insert` só nas pontas; `Format` segue o contexto do node pai; `Split` proibido |
| entidade fora do allowlist (`&foo;`, `&#65;`) | não decodificada | texto literal comum; cada caractere é um EGC editável pelas capabilities do contexto |

**m9 — um único allowlist.** Existiam três listas divergentes: seis nomes nesta
especificação, seis nomes mais entidades numéricas em `inline.rs`, e três em
`draft.rs`. O allowlist do **projetor** é normativamente o dos seis nomes; entidade
numérica é texto literal para o projetor. O renderer legado pode continuar a decodificar
mais do que isso, porque apresentar não é editar, e essa divergência é registrada
aqui em vez de ser corrigida em silêncio.

### 27.8 M8 — o slot canônico de texto é uma função total

A regra anterior só decidia quando o caret chegava por Left/Right e existia run
visível daquele lado. `End`, clique, Up/Down, `resulting_cursor_anchor` e undo não
tinham resposta, e em `**abc**` digitar após `c` produzia `**abcX**` ou `**abc**X`
conforme a implementação.

> Left/Right navega entre fronteiras de grapheme e seleciona o **slot canônico de
> texto** da fronteira alcançada. A escolha é uma função total de
> `(fronteira, direção)`, onde `direção ∈ {FromLeft, FromRight, Absoluta}`:
> 1. se `direção` é `FromLeft`/`FromRight` e existe run visível adjacente do lado
>    para o qual o movimento se deu, o slot canônico é o mais interno que contém
>    esse run;
> 2. caso contrário — extremo de bloco ou de documento, ou `direção` `Absoluta`
>    (Home, End, Up/Down, clique, `resulting_cursor_anchor` após transação,
>    undo/redo, snap de `RawBookmark`) — o slot canônico é o mais interno que contém
>    o run visível adjacente do lado oposto;
> 3. se não há run visível de nenhum lado, é o slot mais externo, isto é, o de
>    `context_path` mais curto.
>
> Isso mantém o estilo ao digitar no começo/fim de um run e torna `End` seguido de
> digitação equivalente a chegar por Right.

**m10 — a métrica do snap.** "Slot legal mais próximo" mede distância em
`SourceOffset` (bytes), não em EGC; empate resolve pelo lado da direção de navegação
e depois pelo anterior, como já dizia a R1.

### 27.9 M9 — `RawBookmark` não desfaz a navegação do usuário

Só mutação invalidava o bookmark. Navegar três vezes para a direita no modo Visual e
voltar ao Markdown reposicionava o cursor onde ele estava antes, para trás do próprio
movimento do usuário — e a propriedade 16 congelava esse comportamento.

> O bookmark vale enquanto estiver intacto: nenhuma mutação do `Draft` e nenhum
> movimento de cursor ou seleção no modo Visual desde a entrada no modo. Intacto e
> na mesma generation, Visual -> Markdown restaura anchor/head raw exatos. Qualquer
> mutação ou qualquer movimento visual o consome; a partir daí a volta deriva os
> offsets do `source_offset` do slot canônico corrente de anchor e head, preservando
> a direção. A mesma regra vale para seleção.

### 27.10 M10 e M21 — histórico sem identidades de generation e com piso

> Undo/redo restaura `EditorSnapshot`, que contém apenas valores independentes de
> generation: a fonte, a `ScalarPosition` raw de cursor, a seleção raw e uma âncora
> visual estrutural `(SourceOffset, [NodeKind] do caminho, stickiness)`. Nenhuma
> identidade de generation — `BlockId`, `NodeId`, `CaretSlotId`, `LexemeId`,
> `Generation` — é armazenada no histórico. Após restaurar, a projeção é refeita e o
> slot é resolvido casando primeiro o `SourceOffset`, depois o caminho de
> `NodeKind`, depois a stickiness; se o casamento falhar, aplica-se a regra 2/3 do
> slot canônico do §27.8.

E o budget ganha piso e consequência definida:

> O estado corrente do `Draft` nunca é contabilizado no budget nem removido: o budget
> governa apenas as entradas de undo/redo. Ao exceder o limite de entradas ou de
> bytes, removem-se snapshots mais antigos, inteiros, até caber. Se um único snapshot
> anterior já não couber no budget, o histórico fica vazio: undo torna-se no-op
> anunciado ao usuário, a edição prossegue normalmente e nada é descartado da fonte.
> Nenhuma transação é recusada por falta de budget de histórico.

### 27.11 M11 — ordenação estrita de patches

Dois ranges vazios no mesmo offset são disjuntos por qualquer leitura de conjuntos, e
"do maior offset para o menor" não os ordena entre si — exatamente o que Enter dentro
de `**ab**` emite.

> `patches` é estritamente ordenado por `start` crescente e, para `i < j`, vale
> `patches[i].end < patches[j].start`. Consequências: dois patches nunca partilham
> um offset, nenhum patch de range vazio coincide com o `start` ou o `end` de outro,
> e todo conjunto de inserções no mesmo offset deve ser fundido em **um único**
> patch cujo `replacement` já está na ordem final dos bytes. O `Draft` aplica do
> maior offset para o menor; com esta ordenação o resultado independe da ordem de
> aplicação.

### 27.12 M12 — o envelope passa a ser declarado e comparado

A propriedade 12 era vacuamente verdadeira: um único patch cobrindo o documento
inteiro não deixa byte algum "fora do envelope". `SourceTransaction` ganha o campo
`envelope: SourceRange`, todo patch está contido nele, e a propriedade passa a
comparar o envelope declarado com o envelope mínimo esperado por fixture — incluindo
`<span red>a</span><span red>b</span>` com edição só no primeiro node, `**abc** xyz`
com format dentro do Strong, e uma inserção de um caractere em 100 KB cujo envelope
deve ser O(bloco) e não O(documento).

### 27.13 M13 — convenção de line ending sem buraco

> Um patch que cria nova quebra usa a convenção local determinística, nesta ordem:
> (i) o line ending que termina o bloco atual; (ii) se o bloco atual não termina em
> line ending, o line ending imediatamente anterior ao bloco no documento; (iii) se
> não houver nenhum antes, o primeiro line ending do documento; (iv) se o documento
> não possui nenhum line ending, `LF`. Misturas preexistentes são preservadas.

E a propriedade 18 passa a declarar seu escopo em vez de ser falsa hoje:

> 18. nenhuma projeção, transição de modo ou `SourceTransaction` normaliza CRLF, LF,
>     whitespace, escapes ou grafia da fonte; um `SourcePatch` nunca intersecta
>     parcialmente um lexeme de line ending. **Nota de escopo:** o editor raw
>     5.0D.2 trata `\r` como caractere comum e sua tecla Enter escreve `LF`; essa
>     divergência conhecida fica fora da prova até que B.3 a alinhe ou a documente
>     como limitação explícita.

### 27.14 M14 e M15 — capabilities de parágrafo e matriz de boundary

A linha de parágrafo era a única sem rótulo de gate e concedia `Split`/`Join` já na
subfase corrente, contra o §26.14, além de omitir `DeleteBoundary`. Passa a ser:

| Construção | Reconhecida/projetada | Operações visuais iniciais |
|---|---|---|
| texto de parágrafo | sim/sim | `Insert`, `DeleteInside`, `ReplaceSelection` em B.3; `Split`, `Join` e `DeleteBoundary` em B.4 |

E o §26.9 ganha, depois de "DeleteInside nunca implica DeleteBoundary":

> Backspace no início de um bloco e Delete no fim de um bloco exigem, cumulativamente,
> `Join` concedido em ambos os blocos do par e `DeleteBoundary` concedido ao bloco que
> possui os lexemes de line ending consumidos. Sem ambos, a tecla é `Refusal`.

A matriz do §26.9 recebe a célula de heading corrigida e três linhas novas:

| Construção | Enter início | Enter meio | Enter fim | Join/Backspace/Delete de boundary |
|---|---|---|---|---|
| heading | parágrafo antes; heading fica | dois headings do mesmo nível | parágrafo depois | B.4: Backspace no início do texto de um heading cujo bloco anterior é parágrafo ou heading é `Refusal` enquanto o prefixo for `Protected`; Delete no fim de um heading cujo próximo bloco é parágrafo move o texto do parágrafo para dentro do heading, preservando o nível, em uma transação; heading+heading é `Refusal` em B.4 |
| fronteira com bloco Opaque/SourceVisible vizinho | — | — | — | `Refusal` em ambas as direções |
| início do documento (Backspace) / fim do documento (Delete) | — | — | — | no-op: nenhum patch, nenhuma entrada de history, nenhum aviso de recusa |
| bloco vazio | Enter cria outro bloco vazio | — | idem | Backspace remove o bloco vazio e junta, quando `Join`+`DeleteBoundary` do par existem |

### 27.15 M16 — procedimento explícito de delimiter runs

O subconjunto descrevia propriedades de um resultado, não um procedimento, e o
catch-all "casos cuja precedência não seja decidida por estas regras" herdava a
indefinição: `*a**b*` e `**a*b**` eram decididos por uma implementação e não por
outra.

> O reconhecimento de runs é uma varredura única da esquerda para a direita, por
> linha, fora de code, HTML, escape e opaque. Um *run* é uma sequência maximal de
> 1 a 3 bytes `*` (respectivamente `~`), e runs de largura 4 ou mais são literais.
> Um run é **abridor** se for precedido por início de linha, whitespace ou
> pontuação, e seguido por um byte que não seja whitespace; é **fechador** se for
> precedido por um byte que não seja whitespace e seguido por fim de linha,
> whitespace ou pontuação. Um par é reconhecido quando um abridor é seguido, na
> mesma linha, pelo primeiro fechador de largura **idêntica**, sem que exista entre
> os dois qualquer run de `*` de largura diferente. Qualquer outra configuração —
> incluindo um run que seja abridor e fechador ao mesmo tempo, larguras
> divergentes, runs vazios, `****` e pares que se cruzariam — é literal
> SourceVisible, e nenhum de seus bytes recebe semântica.
>
> "Qualquer outra configuração" refere-se ao **par candidato examinado**, não ao
> restante da linha: um run que falha em parear fica literal e a varredura
> prossegue a partir do **fim daquele run**, nunca de um byte dentro dele, já
> que um run é maximal por definição. Assim `*a**b*` e `**a*b**` não produzem
> node algum: no primeiro, o run `**` não fecha dentro do intervalo e o `*`
> final é precedido por letra; no segundo, o `*` interno é precedido pela letra
> `a` e não satisfaz a regra de abridor. O resultado é único em ambos, que é o
> que o M16 exige; e nenhum desses bytes ganha editabilidade em B.1, porque todo
> mark reconhecido nasce SourceVisible.

**m11 — `***abc***`.** O node composto de ownership único entra explicitamente na
enumeração de B.5; sem isso a negação por omissão o deixaria permanentemente negado.

### 27.16 M17 — digitação não pode proteger o documento

Em B.3, digitar `<` e depois `b` no fim de um parágrafo criava a região
`[5, source_len)` Opaque, e o snap expulsava o cursor para antes do `<`: a partir dali
toda tecla inseria antes do que o usuário acabara de escrever, sem saída além de undo
ou modo raw.

> Regra de digitação. Em contexto de texto plano editável, um `<` digitado é gravado
> escapado como `&lt;` e um `&` digitado como `&amp;`; o `>` é gravado literal.
> Assim nenhuma digitação normal cria candidato HTML, e a entrada de HTML literal
> permanece possível apenas pelo modo Markdown raw ou por colagem, que é comando
> próprio. Se, ainda assim, uma transação fizer com que a posição resultante do
> cursor caia dentro de região `Protected`/`Opaque` recém-criada, o cursor é
> reposicionado pelo snap do §27.8 **e** a aplicação exibe aviso nomeando a região
> protegida e oferecendo o modo Markdown; o silêncio é proibido.

### 27.17 M18 — o segmentador do source map deve ser total

`ratatui::text::Span::styled_graphemes` existe e é público, e um implementador o
encontraria ao ler "se Ratatui não expuser o necessário". Ele filtra
`!g.contains(char::is_control)`, descartando todo grapheme com caractere de controle
— inclusive TAB, que o §26.2 lista explicitamente entre os bytes que precisam ser
cobertos. Um TAB no parágrafo deslocaria todo índice de EGC posterior.

> O segmentador usado pelo source map deve ser total: itera todos os EGCs, sem
> filtrar, reordenar ou substituir nenhum, e a concatenação dos seus EGCs reproduz a
> string de entrada byte a byte. `ratatui::text::Span::styled_graphemes` e
> `Line::styled_graphemes` são explicitamente **proibidos** como segmentador do
> source map: são helpers de renderização e descartam todo grapheme que contenha
> caractere de controle (inclusive TAB). Podem continuar a ser usados apenas para
> desenhar. A frase "se Ratatui não expuser o necessário" refere-se somente a
> largura de célula (`unicode-width`), nunca a segmentação.

### 27.18 M5 e M20 — regras de seleção e herança de estilo

A regra 4 da R2 era inalcançável: sua pré-condição implica a da regra 3, e as regras
são avaliadas em ordem. Pior, a regra 3 autorizava apagar todo `abc` em `**abc**` sem
cleanup, produzindo `****`, que o §26.11 declara **não** ser um Strong vazio e sim
literal SourceVisible — o usuário converteria um construto editável em literal
protegido apagando três caracteres. As duas regras trocam de lugar e a de path
constante é estreitada:

> 3. caso whole-leaf-node: a seleção coincide exatamente com o visual range de um
>    único mark, sem boundary de mark descendente -> permitido **somente** se a
>    capability pedida possui contrato explícito de cleanup dos open/close lexemes
>    possuídos por esse mark; sem esse contrato, `Refusal`;
> 4. todos os EGCs selecionados têm exatamente o mesmo inline-mark path e a seleção
>    é um subconjunto próprio do conteúdo visual do node mais interno desse path ->
>    operação permitida somente se cada node/contexto concede explicitamente a
>    capability. Se a operação esvaziaria o conteúdo do node mais interno, aplica-se
>    a regra 3 e o mesmo requisito de contrato de cleanup.

E `ReplaceSelection` deixa de herdar um estilo cujos delimitadores a mesma transação
apaga:

> A herança é calculada sobre o `insertion_context` do slot canônico do head **antes**
> do delete. Se o plano de delete aprovado inclui cleanup dos open/close lexemes de
> um mark, esse mark é removido do contexto herdado: a inserção é feita no contexto
> do node pai. Assim, substituir todo o conteúdo de um mark o elimina, e substituir
> parte dele o preserva. Um `active_style_override` explícito prevalece sobre ambos.

### 27.19 M19 — `Underline` pertence a B.6

`<u>abc</u>` é HTML: editá-lo exige o matching de close do §26.10, a dominância de
ancestral, a regra de proteção até EOF e a ordem canônica de serialização — tudo o
que o gate P3 existe para liberar. O item 8 do §26.14 passa a ler:

> Strong, Emphasis, Strike e InlineCode individualmente; nenhum é liberado em lote.
> `Underline` (`<u>`) é sintaxe HTML canônica e pertence a B.6, atrás do gate P3,
> junto de Color e Highlight.

e o item 10 acrescenta `Underline` à lista de B.6.

### 27.20 Propriedades corrigidas

As 27 propriedades do §26.15 permanecem, com estas substituições e uma adição:

- **8** deixa de ser vacuosa ("zero, um ou N" é toda cardinalidade possível): para a
  fixture `<mark …><span …>abc</span></mark>`, a fronteira anterior a `a` tem
  exatamente 3 slots, com `context_path` `[]`, `[Highlight]`, `[Highlight, Color]` e
  `source_offset` estritamente crescentes; em toda fronteira, os slots são ordenados
  por profundidade de `context_path` crescente na abertura e decrescente no
  fechamento.
- **9** acrescenta `SourceVisible`: nenhum slot editável existe dentro de
  `Protected`, `Opaque` **ou** `SourceVisible`.
- **12** passa a ser a propriedade de envelope declarado do §27.12.
- **16** passa a ser a do bookmark intacto do §27.9.
- **18** passa a ser a do §27.13, com nota de escopo do editor raw.
- **19** (decorations nunca serializam) é verdadeira por vacuidade em B.1–B.7 porque
  decorations são 5.0D.5. Fica registrada como **diferida**: não conta como gate
  aprovado da 5.0D.4B e é provada na 5.0D.5.
- **23** deixa de ser prosa e vira teste mecânico: nenhum módulo do projector
  (`projection.rs`, `source_map.rs`, `visual.rs`, `visual_edit.rs`) referencia
  `markdown.rs`, `inline.rs` ou `formatting.rs`; um teste de dependência verifica a
  ausência desses `use` e o gate de fronteira falha se aparecerem.
- **28** (nova): seleção mutante multibloco é `Refusal` sempre que qualquer fronteira
  atravessada não conceda `Join`/`DeleteBoundary` explicitamente; nenhum
  `SourcePatch` jamais intersecta parcialmente um lexeme `BlockPrefix`, `Metadata`
  ou `Protected`.

### 27.21 Correções menores

- **m2** — lexemes são normativamente **não vazios**. Âncoras virtuais de range vazio
  continuam existindo para layout/decoration e continuam não sendo lexemes.
- **m3** — `GraphemeIndex` carrega seu `BlockId`: `(BlockId, GraphemeIndex)` é a
  unidade, e um índice nu não atravessa blocos.
- **m4** — `Refusal` carrega motivo tipado (`StaleGeneration`, `ProtectedRegion`,
  `PartialMarkBoundary`, `MissingCapability`, `BlockBoundary`), para que a aplicação
  possa nomear a causa e oferecer o modo Markdown em vez de falhar em silêncio.
- **m5** — são **cinco** os modos de falha proibidos do §26.13, não quatro: full
  projection incondicional por tecla, source map por célula, prefix scan O(n²),
  history sem budget e nesting sem limite. O gate P1 exclui os cinco.
- **m6** — o §26.17 fala em "nove gates separados"; são **catorze** desde a R2.
- **m8** — o exemplo de três slots de `<mark><span>` é inalcançável antes de B.6 e
  serve como fixture de B.2 somente sobre fonte preexistente, nunca como prova de
  editabilidade.

### 27.22 Status da arquitetura

Esta R3 corrige documentação. Ela não implementa subfase alguma e não aprova a si
mesma. Com B1, B2 e B3 fechados, a revisão independente declarou que B.1 pode
começar, com M1–M4, M6, M16, M18, m1 e m2 incorporados ao trabalho de B.1/B.2 por
serem regras do próprio projetor; M5, M7, M8, M10–M15, M17, M19–M21 valem cada um
antes do gate nomeado em seu título e nenhum bloqueia B.1.

Como a R1 e a R2, esta R3 **não emite o próprio veredito**: a concessão da revisão
anterior era condicional aos três BLOCKERs, e o veredito cabe à revisão independente.

**5.0D.4A.R3 READY FOR RE-REVIEW**

## 28. Fase 5.0D.4A.R4 — fechamento após a segunda revisão independente

### 28.1 Status

A segunda revisão independente confirmou **B1 e B2 fechados** e 17 dos 21 MAJORs
fechados, manteve **B3 aberto** e apontou oito defeitos novos introduzidos pela
própria R3. Esta R4 fecha B3, os três blockers novos (N1, N2, N3) e os MAJORs
restantes, com a redação mínima proposta pela revisão. Onde R4 e R3 divergirem, R4
prevalece; onde R4 e R1/R2 divergirem, R4 prevalece.

### 28.2 B3 e N3 — seleção multibloco versus teclas de boundary

A R3 fechou o §27.4 com "nenhum par com prefixo Protected concede (ii) … logo … é
`Refusal`", mas o §27.14 concedeu o join heading→parágrafo. Em `# T\n\npara` as duas
frases davam respostas opostas e ambas eram R3, de modo que a cláusula de precedência
do §27.1 não arbitrava. A frase final da regra 0 do §27.4 passa a ser:

> Esta regra governa apenas seleções multibloco; teclas de boundary em caret vazio
> seguem a matriz do §26.9. Na arquitetura inicial nenhum par que envolva prefixo
> `Protected` concede (ii) **para fins de seleção multibloco**, logo toda seleção
> multibloco que atravesse um heading, lista, task, quote, callout, fence ou região
> Opaque é `Refusal`, ainda que a matriz do §26.9 conceda a tecla de boundary
> correspondente.

E na cláusula (iii) da mesma regra, "fica parcialmente contido" passa a ser
"é intersectado, total ou parcialmente,": a contenção **total** de um lexeme
`BlockPrefix`, `Metadata` ou `Protected` também é proibida, não só a parcial.

A célula de capabilities da linha de heading do §26.5 passa a ser:

> texto: `Insert`/`DeleteInside`/`ReplaceSelection` em B.4; `Split`, `Join` e
> `DeleteBoundary` em B.4 nos pares que o §26.9 concede; `Format` somente após o
> gate B.5/B.6 da capability inline e permissão explícita no contexto

E o §26.2 ganha a regra de propriedade que a regra de boundary pressupunha:

> Um lexeme de line ending entre dois blocos pertence ao bloco **anterior**.

### 28.3 N1 — apagar um parágrafo inteiro não é `Refusal`

O estreitamento do §27.18 ("subconjunto **próprio** do conteúdo visual do node mais
interno") foi escrito para paths de mark não vazios. Texto plano tem path vazio, de
modo que selecionar um parágrafo inteiro e apagá-lo caía na regra de whole-leaf e
exigia um contrato de cleanup que um `Paragraph` nunca pode ter. A regra 4 passa a
ser:

> 4. todos os EGCs selecionados têm exatamente o mesmo inline-mark path e, quando
>    esse path é não vazio, a seleção é um subconjunto próprio do conteúdo visual do
>    node mais interno dele -> operação permitida somente se cada node/contexto
>    concede explicitamente a capability. Path vazio (texto plano de um único bloco)
>    satisfaz esta regra para qualquer extensão da seleção, inclusive o conteúdo
>    inteiro do bloco, porque um bloco não possui delimitadores a limpar. Se a
>    operação esvaziaria o conteúdo do node mais interno de um path **não vazio**,
>    aplica-se a regra 3 e o mesmo requisito de contrato de cleanup.

E a regra 0 deixa de ser um beco sem saída:

> Uma seleção multibloco aprovada pela regra 0 é classificada pelas regras 1–5
> aplicadas ao conjunto de inline-mark paths dos EGCs selecionados, ignorando a
> fronteira de bloco; os bytes de line ending já estão no envelope pela regra 0.

### 28.4 N2 — os três eixos passam a ser totais

O §27.6 não atribuía visibilidade a lexeme de range visual vazio, e a própria
justificativa do B3 depende de `# ` ser Protected **e invisível** — o que a frase
"nenhum node pode ser `Projected` e `Protected`" proibia. Com a propriedade 9
emendada para incluir `SourceVisible`, a lacuna significaria que nada é editável.
Acrescenta-se ao §27.6:

> A visibilidade é atribuída apenas a lexemes com range visual não vazio. Um lexeme
> de range visual vazio (`OpenSyntax`, `CloseSyntax`, `BlockPrefix`, `Metadata` de
> node `Projected`) é `Protected` e **invisível**: não projeta grapheme algum, não
> contribui EGC e não é `SourceVisible`. `SourceVisible` designa exclusivamente o
> node ou lexeme cuja própria sintaxe — delimitadores, tags, atributos, prefixo — é
> projetada literalmente; texto sem sintaxe própria é sempre `Projected`. A proibição
> de `Projected` + `Protected` vale para o **node**, nunca para seus lexemes de
> sintaxe invisíveis, que é o que permite a `# ` de um heading ser Protected e
> invisível.
>
> A enumeração entre parênteses é **fechada e por espécie de lexeme**:
> `OpenSyntax`, `CloseSyntax`, `BlockPrefix` e `Metadata`. Um lexeme
> `LineEnding` **não** pertence a ela e nunca é `Protected` por esta regra —
> sem isso, a cláusula (iii) do §28.2 proibiria intersectar qualquer line
> ending, a regra 0 não aprovaria seleção multibloco alguma e o parágrafo
> seguinte do §28.3 seria código morto. Line endings entre blocos entram no
> envelope exatamente quando a cláusula (ii) autoriza o Join daquela fronteira,
> e então integralmente.

E a propriedade 9 passa a ler: nenhum slot editável existe dentro de `Protected`,
`Opaque`, ou de um **node** `SourceVisible` — node, não lexeme.

### 28.5 N4 — aninhamento volta a ser representável

A cláusula "sem que exista entre os dois qualquer run de largura diferente" tornava
`**a *b* c**` irreconhecível, contradizendo a ordem canônica do §26.7 que o §27.1
diz preservar: aplicar Emphasis dentro de um Strong emitiria `**a*b*c**`, que
reprojetaria como literal inelutável. A cláusula de par passa a ser:

> Um par é reconhecido quando um abridor é seguido, na mesma linha, pelo primeiro
> fechador de largura idêntica, sem que exista entre os dois nenhum run de largura
> diferente **que não forme ele próprio um par balanceado inteiramente contido no
> intervalo**. Pares assim contidos produzem nodes aninhados na ordem canônica do
> §26.7; qualquer run de largura diferente que não feche dentro do intervalo torna
> o par externo literal.

Os exemplos do §27.15 permanecem válidos e nenhum deles produz node: em `*a**b*` o
run `**` não fecha dentro do intervalo e o par externo fica literal; em `**a*b**` o
`*` interno é precedido por letra e nem abridor é. Como o run é maximal, a varredura
retoma no fim do run que falhou, de modo que o segundo asterisco de um `**` nunca é
lido como abridor de um par de largura 1.

### 28.6 N5 — o slot canônico é total também para `Absoluta`

A regra 2 do §27.8 dizia "lado oposto" sem referente quando a direção é `Absoluta` e
há run visível dos dois lados — clique ou Up/Down em `xy**abc**`. Sua cauda passa a
ser:

> … o slot canônico é o mais interno que contém o run visível adjacente do lado
> oposto ao extremo alcançado; quando a direção é `Absoluta` e existe run visível
> dos dois lados, o slot canônico é o mais interno que contém o run visível **à
> esquerda** da fronteira.

### 28.7 N7 — um delimitador recém-digitado nunca fica preso

O §27.16 escapava `<` e `&` mas não `*` nem `~`: um `*` digitado sozinho vira run
literal `SourceVisible`, e a regra 1 mais a propriedade 9 emendada o tornariam
inapagável — exatamente a armadilha que o M17 existe para matar. Acrescenta-se:

> A mesma regra vale para qualquer byte que, digitado isoladamente, produziria um
> run de delimiter literal `SourceVisible`: enquanto um `*` ou `~` digitado não
> formar par reconhecido, seus bytes permanecem `Editable` como texto comum e podem
> ser apagados; a classificação `SourceVisible` do §27.15 nunca torna
> inselecionável um run que a própria sessão acabou de digitar.

### 28.8 N6, N8, N10, N11 — vocabulário e resíduos

- **N6.** `NodeKind` e `LexemeKind` passam a ser enumerações declaradas no §26.2.
  `NodeKind` é o discriminante **sem payload**: nível de heading e valor de cor não
  fazem parte dele, o que é o que torna a âncora estrutural do §27.10
  (`SourceOffset`, `[NodeKind]`, stickiness) bem definida. `LexemeKind` inclui
  `BlockPrefix` e `Metadata`, que a R3 usava normativamente e só existiam no esboço
  da seção 7 declarado superado.
- **N8.** A redação do §27.19 para o item 8 do §26.14 acrescenta "o node composto
  `***abc***`, com ownership único", que o m11 exigia e a R3 havia deixado cair.
- **N10.** No passo 4 do §26.10, "fences" passa a ler "fences já resolvidos pelo
  passo 0 são opacos à busca de close: nenhum `</nome>` dentro deles desempilha".
- **N11.** Para `~`, apenas a largura 2 tem vocabulário (`Strike`); larguras 1 e 3
  são literais, e `~~~` em início de linha é fence, resolvido no passo 0 antes de
  qualquer varredura de run.

### 28.9 N12 — a regra de escape e os consumidores do Core

Escrever `&lt;` onde o usuário digitou `<` muda os bytes persistidos que CLI, MCP e
GUI leem. Isso é deliberado e já é o comportamento da 5.0D.3
(`formatting.rs::escaped_typed`), e o `visible_text` do Core decodifica entidades
para busca e apresentação. Fica registrado como consequência conhecida: a regra vale
somente para digitação em contexto de texto plano editável do **editor Visual**; o
editor Markdown raw continua gravando o byte literal, e nenhuma nota existente é
reescrita.

### 28.10 Status da arquitetura

Esta R4 corrige documentação. Não implementa subfase alguma e, como a R1, a R2 e a
R3, **não emite o próprio veredito**.

**5.0D.4A.R4 READY FOR FINAL RE-REVIEW**

## 29. Fase 5.0D.4B.1 e 5.0D.4B.P0 — fundação de projeção lossless e baseline

### 29.1 Escopo entregue

B.1 entrega a partição física e a árvore semântica, sem UI visual, sem cursor, sem
mutação e sem dependência de grapheme, exatamente como o §26.14 exige. Dois módulos
novos:

- `noteit-tui/src/source_map.rs` — os tipos do §26.3 (`Generation`, `SourceOffset`,
  `SourceRange`, `ScalarPosition`, `GraphemeIndex`, `DisplayColumn`) sem
  `From<usize>`, sem `Deref` e sem aritmética entre espécies, mais as duas
  conversões nomeadas e validadas `offset_of_scalar` e `scalar_of_offset`, que são a
  ponte para o `Draft` raw.
- `noteit-tui/src/projection.rs` — `Lexeme`, `LexemeKind`, `Node`, `NodeKind`,
  `Classification` (os três eixos do §27.6/§28.4) e `project()`.

Nenhuma dependência nova; `Cargo.lock` byte-idêntico. O `Draft`, o `app.rs`, o
renderer e o Core não foram tocados.

### 29.2 Provas

Os 30 testes de `noteit-tui/tests/visual_projection.rs` foram escritos primeiro e
falharam na baseline `04532f1` pelo motivo esperado — os módulos não existiam
(`unresolved import noteit_tui::projection`). As seis propriedades de cobertura
(§26.15.1–6) são verificadas por um helper único aplicado a toda fixture, a todo
caso gerado e a todo prefixo de um documento hostil.

As nove linhas da tabela normativa do §26.10 são reproduzidas exatamente. Também são
provados: `2 < 3 and 4 > 1` literal sem Opaque; open tag sem close protegendo até
EOF; close órfão e close malformed até EOF; void e self-closing autocontidos;
Markdown dentro de Opaque nunca reclassificado; dominância de ancestral; budget de
nesting em 32; CRLF e LF preservados como lexemes distintos; entidade atômica de
cinco bytes; lexemes nunca vazios; e projeção independente da viewport.

Os property tests são determinísticos e locais, sem framework novo: um LCG de semente
fixa compõe 4.000 fontes a partir de 50 fragmentos escolhidos para colidir
(delimitadores junto de tags, tags junto de entidades, Unicode junto de tudo), e todo
prefixo UTF-8 válido de um documento adversarial é projetado. Uma falha aqui é
reproduzível na próxima máquina, que é a única espécie de falha de propriedade que
vale num gate.

A propriedade 23 deixou de ser prosa: `scripts/check-tui-boundary` falha se
`projection.rs`, `source_map.rs`, `visual.rs` ou `visual_edit.rs` referenciarem
`markdown.rs`, `inline.rs` ou `formatting.rs`. O gate foi verificado por injeção de
violação — falha com exit 1 e volta a passar quando removida.

### 29.3 Defeitos encontrados e corrigidos durante B.1

Três defeitos reais apareceram ao executar os testes, e não por inspeção:

1. **CRLF virava dois lexemes.** `line_end` apontava para o `\n`, deixando o `\r` no
   texto da linha. Corrigido: a linha termina antes do `\r`, e o CRLF é um lexeme de
   dois bytes.
2. **Lexeme retrocedendo.** Uma região HTML que escapasse de um construto inline
   delimitado (mark, code span, link) fazia o emissor andar para trás e quebrava a
   partição. Corrigido com `html_escapes`: o construto delimitado perde para a
   região HTML e fica literal, que é a leitura fail-closed.
3. **Regra de run ambígua.** `*a**b*` e `**a*b**` foram encontrados pela própria
   redação do M16 e resolvidos no §27.15/§28.5; o resultado documentado foi corrigido
   depois que o teste mostrou que `**a*b**` não produz node algum — o `*` interno é
   precedido por letra e não é abridor.

### 29.4 P0 — baseline de projeção

Medido em release nesta máquina, melhor de três execuções por cenário:

| Cenário | Bytes | Tempo | Lexemes | Nodes |
|---|---|---|---|---|
| 1 KB realista | 1.110 | 0,01 ms | 156 | 40 |
| 100 KB realista | 102.490 | 1,36 ms | 14.404 | 3.602 |
| 1 MB realista | 1.048.580 | 11,77 ms | 147.368 | 36.843 |
| 20.000 linhas | 420.000 | 4,30 ms | 40.000 | 2 |
| uma linha de 100.000 | 100.000 | 0,49 ms | 1 | 2 |
| nesting adversarial (31 níveis) | 217 | 0,01 ms | 1 | 3 |
| Unicode denso (ZWJ/CJK) | 600.000 | 1,82 ms | 1 | 2 |
| HTML adversarial (2.000 candidatos) | 12.000 | 0,07 ms | — | — |

Crescimento medido: 10x bytes custam 8,9x tempo no documento realista e 9,6x na linha
única — linear nos dois eixos que o §26.13 proíbe serem quadráticos.

**Um defeito de performance real foi encontrado e corrigido aqui.** A primeira
medição deu 31,4x para 10x bytes, porque a consulta às regiões literais do passo 0
era uma varredura linear feita uma vez por byte candidato. Trocada por bisseção
sobre a lista já ordenada, o crescimento caiu para 8,9x e 1 MB passou de 48,53 ms
para 11,77 ms. Os testes de P0 afirmam a **forma** — razões de crescimento — e não
leituras de cronômetro, porque um limiar em milissegundos é a opinião de uma
máquina, enquanto uma razão sobrevive a ser executada noutra.

P0 é informativo quanto aos números e obrigatório quanto à sua existência; ele não
substitui P1, que é o gate que precisa excluir os cinco modos de falha do §26.13
antes de B.3.

## 30. Fase 5.0D.4B.2 — source map visual somente leitura

### 30.1 Escopo entregue

B.2 acrescenta visão, não mãos. O módulo novo `noteit-tui/src/visual.rs` traz
`VisualDocument` (derivado, imutável, válido para uma única `Generation`),
`GraphemeCell`, `VisualBlock`, `CaretSlot`, `RawBookmark` e `Direction`. Nenhum
método devolve mutação, e o `Draft` continua sendo tocado apenas pelo editor raw.

Marks e HTML canônico continuam `SourceVisible` neste gate: seus delimitadores são
projetados literalmente e não têm caret interno. Eles só se tornam invisíveis quando
B.5 e B.6 concederem as capabilities que tornam honesto escondê-los — projetar um
delimitador como invisível antes disso afirmaria ao leitor que ele pode editá-lo.

### 30.2 Dependência Unicode — autorização e auditoria

`unicode-segmentation 1.13.3` e `unicode-width 0.2.2` passam a ser dependências
diretas de `noteit-tui`. A autorização condicional do §26.12 exige auditoria, e ela é:

- **Licença:** `MIT OR Apache-2.0` em ambas, compatível com o MIT do projeto.
- **Árvore transitiva:** `unicode-segmentation` tem **zero** dependências próprias;
  `unicode-width` também. Nenhuma rede, nenhum I/O, nenhum runtime.
- **MSRV:** 1.85.0 e 1.66, ambos abaixo do 1.89 declarado pelo workspace.
- **Impacto no lock:** nenhum crate novo foi resolvido e nenhuma versão mudou. As
  duas já estavam no `Cargo.lock` transitivamente, via `ratatui-core`,
  `unicode-truncate` e `crossterm → derive_more → convert_case`. O diff inteiro do
  lock são **duas linhas**, acrescentando-as à lista de dependências deste pacote.
- **Necessidade:** provada por teste. `e` + U+0301, `👨‍👩‍👧‍👦`, `🇧🇷` e `👍🏽` são
  um EGC cada e vários escalares cada; um cursor que conta `char` pousa dentro de
  todos eles. Escrever um segmentador próprio é proibido pelo §26.12.
- **Proibição específica:** `ratatui::text::Span::styled_graphemes` não pode ser o
  segmentador do source map (§27.17). Ele filtra
  `!g.contains(char::is_control)`, descartando TAB, o que deslocaria em um todo
  índice de EGC posterior. Há teste de regressão para isso: `a\tb` projeta três
  graphemes.

### 30.3 Regra de slot descoberta ao implementar

A especificação diz que não existe caret dentro de `Protected`/`Opaque`, mas ranges
são semiabertos: numa nota que é inteiramente `<x>abc</x>`, o offset do fim do bloco
cai **fora** do range Opaque e produziria um caret flutuando ao fim de algo não
editável. A regra implementada, e agora documentada, é mais estreita e falha fechada:

> Uma fronteira só é caret quando um grapheme que o leitor pode editar a toca — o
> anterior ou o seguinte. Assim `<x>abc</x>` não tem caret algum, `# T` tem
> exatamente dois (antes e depois do `T`, nunca dentro de `# `), e um parágrafo tem
> um por fronteira.

### 30.4 Provas

Os 17 testes de `noteit-tui/tests/visual_source_map.rs` foram escritos primeiro e
falharam na baseline pelo motivo esperado — o módulo não existia. Cobrem: EGC nunca
partido (NFC, NFD, marcas combinantes empilhadas, ZWJ, bandeiras, tom de pele, CJK);
TAB preservado; largura de célula separada da contagem de graphemes; todo slot numa
fronteira de EGC; nenhum slot dentro de região protegida; prefixo de heading sem
caret; zero/um/N slots com ordenação por profundidade; slot canônico total para as
três direções; round-trip de todo slot pelo seu offset; ponte escalar↔byte através de
Unicode; recusa de objeto de outra generation; bookmark exato quando intacto e
recusado quando consumido; troca de modo repetida sem mudar byte nem history; e
região sem slots ainda projetando sua fonte para o leitor.

A cardinalidade N>1 de slots por fronteira só é alcançável com duas marks
**projetadas** aninhadas, o que existe a partir de B.6; o §27.21/m8 já registra isso.
O que B.2 prova é a regra de ordenação e os casos zero e um.

## 31. Fase 5.0D.4B.P1 — gate pré-interativo

### 31.1 O que P1 tem de excluir

O §26.13, corrigido pelo §27.21/m5, proíbe **cinco** formas, não quatro. B.3 não pode
começar enquanto qualquer uma existir. Cada uma tem agora um teste que falharia se
ela estivesse presente, em `noteit-tui/tests/visual_performance.rs`:

| Modo proibido | Como está excluído | Prova |
|---|---|---|
| comportamento O(n²) / prefix scan repetido | consulta de regiões literais por bisseção, não varredura | crescimento de 10x bytes para 4,0x–8,9x tempo no documento realista e 9,6x–12,7x na linha única |
| source map por célula de terminal | as células são por grapheme e carregam largura apenas para layout | `日本語` são 3 células de largura 2, não 6 entradas |
| reparse ligado à viewport | `project()` recebe `(source, generation)` e nada mais | não há parâmetro de largura, altura ou scroll a passar |
| history sem budget | `HISTORY_BYTE_BUDGET` de 16 MiB ao lado de `UNDO_LIMIT` | 50 edições numa nota de 200 KB ficam dentro do budget |
| nesting sem limite | `NESTING_BUDGET` de 32, dentro do algoritmo pelo passo 4a | 52 níveis continuam rápidos e falham fechados |

### 31.2 O budget de bytes do histórico

Um limite de passos não é um limite de memória: 200 snapshots de uma nota de 1 MB são
200 MB. `draft.rs` ganhou `HISTORY_BYTE_BUDGET` (16 MiB) e `history_bytes()`, e a
evicção remove snapshots **inteiros** mais antigos — meia história não é um estado a
que alguém possa voltar.

O piso do §27.10 está implementado e testado: o texto vivo nunca é contado nem
removido, e nenhuma edição é recusada por falta de histórico. Numa nota maior que o
budget inteiro, a história fica vazia, `undo()` devolve `false` como no-op anunciado,
e a edição prossegue — recusar a edição para preservar o registro dela seria perder o
trabalho do usuário para salvar sua anotação.

### 31.3 Uma regressão real, causada por este gate

Ao acrescentar os cenários de performance, o teste
`a_sigterm_with_a_pending_draft_preserves_it_and_restores_the_terminal` passou a
falhar de forma intermitente — uma vez em três execuções da suíte completa, nunca
isoladamente.

A causa foi investigada e não é flakiness do teste: os cenários novos projetam 1 MB e
1.000 KB em build de debug, saturando os 8 núcleos da máquina, e o teste de sinal
espera texto de um pseudoterminal real dentro de um prazo. Rodar as duas coisas juntas
tirava dele o tempo de CPU de que precisa. A prova é direta — com os testes de
performance fora da invocação, três execuções limpas; com eles, uma falha em três.

A correção não foi aumentar o prazo, que esconderia um deadlock se algum dia houvesse
um, e sim reconhecer que build de debug não mede nada útil: os números desta seção e da
§29.4 vêm de `--release`. Em debug, cada cenário roda a décima parte do tamanho —
prova as mesmas formas e devolve a máquina aos testes que precisam de um terminal
respondendo a tempo. Cinco execuções consecutivas da suíte completa passaram depois
disso.

## 32. Fase 5.0D.4B.3 — edição visual mínima

### 32.1 Escopo entregue

O módulo novo `noteit-tui/src/visual_edit.rs` é o único lugar da TUI que converte
intenção visual em bytes. `plan()` é **puro**: lê uma projeção imutável e devolve
`SourceTransaction` ou `Refusal`, e não pode alterar nada em nenhum dos dois casos.
Essa separação é o que transforma "uma recusa preserva todo estado observável" de
promessa em propriedade — não há nada a desfazer, porque nada foi feito.

`SourceTransaction` carrega `generation`, `patches`, `envelope` e
`resulting_cursor`. O `envelope` é **declarado**, não inferido, conforme o §27.12:
sem ele a propriedade 12 seria satisfeita vacuamente por um único patch cobrindo o
documento inteiro.

B.3 concede exatamente três operações sobre texto plano de parágrafo — `Insert`,
`DeleteInside` e `ReplaceSelection`. Todo o resto é `Refusal` com motivo tipado
(`StaleGeneration`, `ProtectedRegion`, `PartialMarkBoundary`, `MissingCapability`,
`BlockBoundary`, `InvalidPosition`), para que a aplicação possa nomear a causa e
oferecer o modo Markdown em vez de uma tecla que não faz nada em silêncio.

### 32.2 `Generation` por sessão

O §27.2 exige um contador de sessão, estritamente monotônico, que avança em **toda**
mutação. `draft.rs` ganhou um `AtomicU64` estático e `Draft::generation()`; toda
mutação passa por `touch()`, inclusive undo, redo e cada tecla de um agrupamento de
inserção. Um agrupamento é um passo de **undo** e ainda assim muitas versões do
texto: um objeto medido antes da terceira tecla não pode ser aceito depois dela.

Nenhum `EditorSnapshot` armazena generation, de modo que restaurar não pode mover o
contador para trás.

`Draft::apply_transaction` é a última linha de defesa, não a primeira: o planner já
provou que os patches são ordenados, disjuntos e válidos; o `Draft` prova que a
generation ainda é aquela em que foram medidos. O §26.15.22 faz do `Draft` o único
lugar onde uma mutação visual pode pousar, portanto é também o único lugar onde uma
pode ser barrada.

### 32.3 Provas

Os 21 testes de `noteit-tui/tests/visual_editing.rs` foram escritos primeiro e
falharam na baseline pelo motivo esperado. Cobrem: patches estritamente ordenados e
sem compartilhar offset; todo patch dentro do envelope declarado; envelope local que
não alcança bloco vizinho; bytes fora do envelope idênticos; inserção, remoção de
grapheme inteiro (família ZWJ de 25 bytes vai inteira ou não vai) e substituição de
seleção; parágrafo inteiro apagável (§28.3/N1); recusa em região protegida; recusa de
generation antiga; recusa de seleção multibloco em `# T\n\npara` e em `ab\n\ncd`;
recusa de seleção que cruza parcialmente uma mark; recusa de formatação por ausência
de capability; um comando igual a um passo de undo; generation avançando em transação,
undo, redo e tecla raw; transação stale recusada pelo próprio `Draft`; e o slot
canônico decidindo onde o texto digitado pousa.

### 32.4 Limitação registrada

A integração com `app.rs` e `ui.rs` — a tecla real, a alternância de modo na
interface e o desenho do cursor visual — **não** faz parte de B.3 e não foi feita.
O que existe é o motor: projeção, mapa, planner, transação e autoridade do `Draft`,
com cobertura própria. A interface entra depois de B.4 fechar as fronteiras de bloco,
para que a tecla `Enter` não precise mudar de contrato no meio da integração.

## 33. Fase 5.0D.4B.4 — headings e fronteiras de bloco

### 33.1 Escopo entregue

`Enter`, `Backspace` e `Delete` deixam de ser inserção e passam a ser estrutura.
Três comandos novos em `visual_edit.rs`: `SplitBlock`, `JoinBackward` e
`JoinForward`, cada um implementando a célula correspondente da matriz do §26.9
completada pelo §28.2.

| Construção | Enter início | Enter meio | Enter fim | Boundary |
|---|---|---|---|---|
| parágrafo | parágrafo vazio antes | dois parágrafos | parágrafo vazio depois | une os dois; uma transação |
| heading | parágrafo antes, heading fica inteiro | dois headings do mesmo nível | parágrafo depois | Backspace no início: `Refusal`; Delete no fim, com parágrafo à frente: puxa o texto para dentro do heading preservando o nível |
| heading + heading | — | — | — | `Refusal` |
| vizinho Opaque/SourceVisible/fence | — | — | — | `Refusal` em ambas as direções |
| início/fim do documento | — | — | — | no-op: nenhum patch, nenhuma history, nenhum aviso |

O no-op tem valor próprio, `Refusal::NothingToDo`, precisamente para que a interface
possa ficar calada sobre ele e ainda dizer algo útil sobre uma recusa de verdade.

### 33.2 Convenção de line ending

`local_line_ending` implementa a cascata ordenada do §27.13/§28.3: o ending do bloco
atual, depois o imediatamente anterior, depois o primeiro do documento, depois `LF`.
A regra da R2 não tinha resposta para o último bloco de um documento CRLF, que é
exatamente o caso em que um `LF` seria escrito num arquivo que nunca teve um. Há
teste com `ab\r\n\r\ncd`.

### 33.3 Dois defeitos encontrados ao executar a matriz

1. **O fim do bloco incluía o line ending.** `content_end` era o fim do último
   grapheme projetado, e o line ending é um deles, então um Join removia só um dos
   dois `\n` e produzia `ab\ncd` em vez de `abcd`. `content_end` passou a ser o fim
   do último grapheme **que não é line ending** — o ending pertence ao bloco (§28.2)
   mas não é conteúdo, e um caret depois dele seria um caret no vão entre blocos.
2. **O caret escapava de uma região opaca pelo line ending.** Numa nota
   `<custom>x</custom>\n\npara`, o primeiro bloco é um parágrafo cujo conteúdo
   inteiro é opaco. A busca por "vizinho editável" encontrava o line ending final —
   que não está dentro do range Opaque, porque ranges são semiabertos — e criava um
   slot. Com isso o bloco parecia editável e o Join era permitido. A busca passou a
   ignorar line endings: são quebras, não conteúdo.

Ambos são a mesma classe de erro que a §30.3 já havia registrado, agora numa
posição diferente, e ambos foram encontrados por teste e não por inspeção.

### 33.4 Limitação registrada: não existe "bloco vazio"

A matriz do §26.9 tem uma linha "bloco vazio". Este modelo não tem um bloco vazio
para pôr nela: uma sequência de linhas em branco é o **separador** entre blocos, não
um bloco, de modo que nenhum node a cobre e nenhum caret pode ser posto ali. A
limitação fica registrada em vez de ser disfarçada, e o que o modelo faz em seu lugar
é testado: `ab\n\n\n\ncd` tem dois blocos, o vão não tem caret, e um Join entre eles
remove o vão inteiro sem deixar nenhum `\n` órfão.

Representar parágrafos vazios como nodes próprios é uma decisão de projeto que
pertence a B.7, junto de listas e citações, e não foi tomada aqui.

## 34. Fase 5.0D.4B.P2 — gate pré-inline

### 34.1 O que P2 mede

P2 remede o que B.3 e B.4 acrescentaram e tem de passar antes de B.5. A forma que
ele protege é a do §26.13: planejar uma tecla não pode custar mais conforme a nota
cresce, porque um planner que varresse a nota inteira a cada tecla seria a "full
projection por tecla" proibida em tudo menos no nome.

| Medida (release) | Resultado |
|---|---|
| planejar uma tecla em 50 KB | 0,000 ms (abaixo da resolução do relógio) |
| planejar uma tecla em 500 KB | 0,000 ms |
| aplicar a transação em 200 KB | 0,92 ms |
| recusar dentro de região opaca de 100 KB | 0,001 ms |

### 34.2 Uma quadrática escondida, encontrada por P2

A primeira medição deu 16,5x de tempo para 10x de bytes ao planejar uma tecla. Duas
consultas eram varreduras lineares: `caret_is_legal`, sobre a lista de slots, e
`Projection::node_at`, sobre a lista de nodes. Ambas são feitas uma vez por tecla, e
`node_at` era feita **uma vez por grapheme** ao construir os slots da B.2 — isto é,
a construção do `VisualDocument` era `O(graphemes × nodes)`.

As duas viraram bisseção. Slots são construídos bloco a bloco e, dentro do bloco, em
ordem crescente de grapheme, portanto a lista está ordenada por offset. Nodes são
construídos em pré-ordem — pai antes dos filhos, irmãos em ordem de fonte — logo
`coverage.start()` é não decrescente e o node mais interno que contém um byte é o
**último** dessa ordem que o contém: bisseta-se até o último candidato e caminha-se
para trás.

O efeito medido: planejar uma tecla caiu de 0,183 ms para tempo constante abaixo da
resolução do relógio, e a suíte de performance inteira caiu de 10,95 s para 0,69 s.
A recusa dentro de uma região opaca também é constante, o que importa porque uma
recusa que varresse o documento seria negação de serviço: basta segurar uma tecla
dentro de uma região protegida de uma nota grande.

A asserção de P2 passou a ser absoluta em vez de razão: a esses tempos a razão mede
o relógio, não o código, e o que tem significado é que uma tecla numa nota de meio
megabyte planeje muito abaixo de um quadro.

## 35. Fase 5.0D.4B.5 — capacidades Markdown inline

### 35.1 Uma capability por vez, de verdade

O §26.14 exige que Strong, Emphasis, Strike e InlineCode sejam liberados
**individualmente** — "nenhum é liberado em lote". Isso virou tipo: `Capabilities`
tem um booleano por construção, `VisualDocument::project_with` recebe o conjunto
exato que o gate provou, e `project()` continua entregando `Capabilities::NONE`,
para que quem não pensou no assunto receba a resposta conservadora.

Uma mark cuja capability está concedida torna-se **projetada**: seus delimitadores
deixam de chegar à tela e seu conteúdo passa a ter caret. Enquanto não está, ela
continua `SourceVisible` com os delimitadores visíveis — esconder sintaxe que o
editor ainda não sabe editar diria ao leitor uma mentira sobre o que ele pode fazer
com ela. Há teste para cada combinação: com apenas Strong, `**forte** e *ênfase*`
projeta `forte e *ênfase*`; com apenas Emphasis, `**forte** e ênfase`.

`Underline` não está aqui. É HTML canônico, e o §27.19 o moveu para B.6 atrás do
gate P3: liberá-lo em B.5 seria editar HTML um gate antes do gate que autoriza
editar HTML.

### 35.2 N slots por fronteira, finalmente alcançável

Com marks projetadas, o caso "N slots" do §26.15.8 deixa de ser hipotético. Em
`**abc**` há **seis** posições de caret, exatamente as que o §26.11 enumera: 0 e 2
antes de `a` — externa e interna — depois 3, 4, 5, e 7 depois de `c`. O par
externo/interno é o que permite ao caret estar visualmente no mesmo lugar e
semanticamente dentro ou fora da mark, e é o que faz digitar no começo de um trecho
em negrito continuar em negrito.

Isso exigiu reconstruir a geração de slots em torno de **seams**: entre dois
graphemes visíveis há uma região de sintaxe escondida, e cada aresta de lexeme
dentro dela é uma posição de caret. Um `BlockPrefix` é a exceção — `# ` é estrutural,
e um caret antes dele seria um caret que digita fora do heading que está editando —
então a aresta anterior a um prefixo de bloco é recusada e a posterior é aceita.

A ordenação segue o §26.4: do exterior para o interior na abertura, do interior para
o exterior no fechamento. O teste verifica monotonicidade em uma direção ou na
outra, não ascendente sempre, porque ascendente sempre é a regra errada.

### 35.3 A álgebra de seleção de marks

`check_mark_boundaries` implementa as regras 3 e 4 do §27.18/§28.3 sem discrição:

- todo grapheme selecionado precisa ter exatamente o mesmo inline-mark path;
- path vazio — texto plano num único bloco — é permitido em qualquer extensão,
  inclusive o bloco inteiro, porque um bloco não tem delimitadores a limpar (N1);
- path não vazio é permitido apenas para um **subconjunto próprio** do conteúdo
  visual da mark. Selecionar todo o conteúdo é o caso whole-leaf, que exige contrato
  explícito de cleanup dos delimitadores, e nenhuma subfase até B.5 tem um — então
  recusa, em vez de deixar `****`, que o §26.11 diz não ser um Strong vazio e sim
  texto literal protegido. Apagar três caracteres converteria um construto editável
  num não editável.

Provas: `x **ab**` selecionando `x a` recusa; `**ab** x` selecionando `b x` recusa;
`**a *bc* d**` entrando parcialmente no Emphasis recusa; `**abc**` selecionando `ab`
é permitido e dá `**c**`; selecionando `abc` inteiro recusa com
`MissingCapability`. E `CopyVisualSelection` sucede em todos eles, porque não muta —
essa assimetria é o ponto.

### 35.4 Uma quadrática introduzida e removida no mesmo gate

A primeira implementação de seams varria **todos** os lexemes para cada fronteira e
**todos** os nodes para cada seam, e derivava o content range de uma mark varrendo
lexemes de novo. Isso é `O(cells × nodes × lexemes)`. A suíte de performance deixou
de terminar — não ficou lenta, travou.

Três correções: os content ranges passaram a ser calculados uma vez, numa única
passada que alarga o range de cada ancestral; `seam_offsets` e `seam_is_legal`
viraram bisseção sobre a lista de lexemes, que é ordenada porque ladrilha a fonte; e
`path_of_seam` passou a subir a partir do node mais interno em vez de varrer todos,
o que é trabalho limitado porque o aninhamento é limitado a 32.

Depois disso a suíte de performance voltou a 2,5 s, 1 MB projeta em 13,22 ms e
planejar uma tecla continua constante. Vale registrar o padrão: as três quadráticas
encontradas até aqui (§34.2 e esta) foram todas achadas por um gate de performance
que roda de verdade, e nenhuma por leitura do código.

## 36. Integração do editor Visual na aplicação

### 36.1 O que ficou acessível

Até aqui os gates provaram o motor e nada disso era alcançável pela TUI real. Esta
entrega liga B.1–B.5 à aplicação:

- **`Alt+V`** alterna entre o editor Markdown e o Visual, nos dois sentidos. A nota
  sempre **abre** em Markdown, e sair do editor devolve o modo a Markdown — o caret
  visual é uma posição numa projeção que deixa de existir, e não é carregado para a
  próxima nota.
- O título diz qual editor tem o teclado. Markdown mantém exatamente o título que
  sempre teve; Visual acrescenta ` · Visual`, porque ele é o que esconde algo.
- O rodapé ganhou `Alt+V` **sem perder nada**: Salvar, Formatar e Sair continuam
  descobríveis de 50 a 140 colunas, que é o contrato da 5.0D.3.
- No Visual, negrito parece negrito, itálico parece itálico, riscado parece riscado e
  código inline muda de cor — e seus delimitadores não são desenhados. O que o gate
  ainda não sabe editar continua visível como fonte, em cinza: cor, marca-texto,
  links, listas, tarefas, citações e HTML desconhecido.
- Teclas: caracteres inserem, `Enter` divide o bloco, `Backspace`/`Delete` removem um
  grapheme inteiro ou unem blocos na fronteira, setas andam por slots, `Home`/`End`
  vão aos extremos do bloco, `Shift+setas` selecionam.
- `Ctrl+S`, `Ctrl+Z`, `Ctrl+Y` e `Esc` continuam sendo exatamente o que eram. Salvar
  do Visual passa pela mesma `authority::perform_at` com a mesma revision.

### 36.2 Recusa nunca é silêncio

Toda recusa vira aviso nomeando a causa e oferecendo o caminho: "Recusado: esta
região é fonte protegida e só pode ser alterada no modo Markdown. Alt+V volta ao
Markdown." A única exceção é `NothingToDo` — Backspace no início do documento —, que
o §28.2 define como sem aviso, porque anunciar que nada aconteceu é ruído.

Uma nota que é inteiramente fonte protegida diz isso ao entrar **e a cada tecla**:
o aviso anterior é limpo pela ação seguinte (contrato da 5.0D.3), e sem repetir a
mensagem o leitor ficaria diante de uma tela que não faz nada e não explica por quê.

### 36.3 Dois defeitos encontrados na integração

1. **`Ctrl+S` e `Ctrl+Z` eram digitados na nota.** O despacho para o Visual estava
   **antes** do bloco de atalhos, então `Ctrl+Z` chegava ao handler visual como o
   caractere `z` e era inserido: desfazer escrevia "z" no texto e salvar escrevia
   "s". Corrigido movendo o despacho para depois dos atalhos. Encontrado por teste,
   não por leitura.
2. **O rodapé de 50 colunas perdeu "Formatar".** Acrescentar `Alt+V` à variante mais
   estreita empurrou `Alt+F` para fora, quebrando a regressão da 5.0D.3 que exige
   Salvar, Formatar e Sair descobríveis em todas as larguras práticas. Corrigido
   sacrificando as dicas de undo, que têm alternativa óbvia, em vez de um comando que
   não tem.

### 36.4 O que ainda não está no Visual

Registrado honestamente, porque a tela mostra essas construções como fonte cinza e o
leitor vai perguntar por quê:

- cor, marca-texto e sublinhado — B.6, atrás do gate P3;
- links, listas, tarefas, citações e callouts — B.7, atrás do gate P4;
- matemática e flashcards — 5.0D.5;
- seleção por mouse no Visual, e blocos vazios como nodes próprios (§33.4).

Nada disso está quebrado: está recusado, visível e editável pelo modo Markdown, que
continua sendo o fallback integral e permanente.

## 37. Fase 5.0D.4B.P3 — gate pré-HTML

### 37.1 Medidas

| Medida (release, 200 KB realista) | Resultado |
|---|---|
| `VisualDocument` sem capabilities | 60,9 ms |
| `VisualDocument` com todas as capabilities de B.5 | 40,8 ms |
| slots por bytes | 10x bytes → 10x slots (12.232 → 121.792) |
| planejar uma tecla num documento com marks | 0,000 ms |

Conceder as capabilities inline **não** muda a ordem do custo — sai mais barato,
porque esconder delimitadores produz menos células do que mostrá-los. O mapa de
carets cresce exatamente na proporção do texto, que é o que separa um source map por
grapheme de um por célula de terminal.

### 37.2 A quarta quadrática

P3 encontrou a maior delas. Construir um `VisualDocument` de 200 KB levava **598 ms**
contra 2,4 ms da projeção crua — `cells_of` varria **todos** os lexemes para **cada**
bloco, isto é `O(blocos × lexemes)`, e o mesmo padrão estava em `content_end` e em
`range_touches_protected_lexeme`.

Bisseção nos três, e o mesmo limite de `selected_cells` aos blocos que a seleção
realmente toca: 598 ms → 60,9 ms, e a suíte de performance inteira de 4,18 s para
0,37 s.

Quatro quadráticas encontradas até aqui, todas por um gate de performance que roda
de verdade, nenhuma por leitura do código. Vale como registro de método: a forma de
um custo não é visível numa revisão, e um gate que só mede uma vez no fim teria
encontrado as quatro juntas, tarde.

### 37.3 A projeção por tecla, e o que fica para B.P

Medir a construção do `VisualDocument` expôs uma consequência da integração: a
aplicação a reconstruía **quatro vezes por tecla** — ao decidir o que a tecla
significa, ao planejar, ao reposicionar o caret e ao desenhar.

Agora há cache por `Generation`. A chave é exatamente a certa: ela muda em toda
mutação, então uma entrada obsoleta não pode ser servida, e a invalidação é por
construção e não por alguém lembrar de invalidar. As quatro projeções viraram uma.

**Fica aberto para B.P:** essa uma projeção ainda é integral. Numa nota de 200 KB são
~41 ms por tecla, o que é perceptível, e o §26.13 proíbe full projection
incondicional por tecla em notas grandes. A correção é invalidação localizada por
bloco, que é trabalho de B.P e está registrada aqui com a medida, não escondida.
Notas de tamanho comum — as de 1 KB a 20 KB que o store real contém — projetam em
menos de um milissegundo e não são afetadas.

## 38. Fase 5.0D.4B.6 — HTML canônico de formatação

### 38.1 Escopo entregue

Cor, marca-texto e sublinhado — as três construções que o editor gráfico persiste
como HTML. `Underline` está aqui e não em B.5 porque o §27.19 o moveu: `<u>` é HTML,
e editá-lo precisa do matching de close, da dominância de ancestral e da proteção até
EOF que o gate P3 existe para liberar.

Cada uma tem sua própria capability, como as inline. Com apenas `underline`,
`<u>sub</u> e <mark …>marca</mark>` projeta `sub e <mark …>marca</mark>` — o `<u>`
some e o `<mark>` fica.

No editor, cor vira cor de verdade e marca-texto vira fundo: o valor é lido do
atributo canônico e convertido, e um valor que não seja `#RRGGBB` deixa o estilo
intacto em vez de inventar um.

### 38.2 O rewrite envelope, na prática

Três comandos novos: `SetColor`, `SetHighlight` (com `None` para limpar) e
`ToggleUnderline`. O que cada um escreve é exatamente a grafia de
`formatting::wrapper`, a mesma função que o editor raw e a GUI usam — duas cópias
dessa grafia seriam o risco de verdade.

O envelope é a seleção quando um wrapper é acrescentado e o span do próprio wrapper
quando um é removido; em nenhum caso alcança um irmão. A prova direta é o exemplo do
§26.7: em `<span red>a</span><span red>b</span>`, editar o primeiro deixa o segundo
byte a byte como estava, e os dois continuam dois. "Wrapper máximo" significa máximo
**dentro do envelope**.

Acrescentar um wrapper emite dois patches vazios nas duas pontas, estritamente
ordenados e sem se tocar (§27.11): o conteúdo entre eles não está em patch algum e
portanto não é reescrito. É por isso que `ç日👍🏽` sobrevive a ganhar cor.

### 38.3 A paleta é uma lista fechada

Um valor de cor chega a um atributo HTML. `SetColor` só aceita as grafias que este
projeto escreve — as oito de texto e as cinco de marca-texto de `formatting.rs` — e
recusa qualquer outra em vez de escapá-la. Há teste com `javascript:alert(1)` e com
`"><script>`: ambos são `MissingCapability`, não conteúdo escapado. Um valor que não
está na paleta não é uma cor que alguém escolheu.

### 38.4 O contrato de cleanup, finalmente exercido

A regra whole-leaf do §27.18 recusa uma seleção que cubra todo o conteúdo de uma mark
**a menos que a capability pedida tenha contrato explícito de cleanup** dos
delimitadores daquela mark. Limpar um wrapper **é** esse contrato — é o caso que a
regra estava reservando. Sem essa leitura, limpar uma cor seria o único comando que
nunca poderia ser emitido, porque limpar sempre seleciona todo o conteúdo.

Então a verificação de fronteira de mark roda para tudo, exceto para o wrapper exato
que está sendo removido. Encontrado por teste: os dois testes de "limpar" falharam
com `MissingCapability` antes de a ordem ser corrigida.

### 38.5 O gate de fronteira, refinado em vez de afrouxado

`visual_edit.rs` passou a usar `formatting::wrapper` e as duas paletas, e o gate da
propriedade 23 — que eu mesmo havia escrito — falhou.

A pergunta é se isso é violação. O que o §26.16 nomeia como perigo é
**reconhecimento**: o parser do leitor descarta delimitadores, decodifica entidades e
para o destino de um link no primeiro `)`. `wrapper()` é um `format!` e as paletas são
duas tabelas de constantes; não analisam nada.

O gate passou a proibir o código que **reconhece**, por nome — `markdown::*`,
`inline::*`, `clear_selected`, `clear_enclosing`, `use crate::formatting::*` — o que
também pega um re-export ou um import com alias que a checagem por caminho de módulo
deixaria passar. Verificado nos dois sentidos: com `use crate::inline` falha, com
`clear_selected` falha, e limpo passa. É um gate mais preciso, não mais frouxo.

## 39. Fase 5.0D.4B.P4 e 5.0D.4B.7 — gate pré-blocos e blocos estruturados

### 39.1 P4 — medidas

| Medida (release) | Resultado |
|---|---|
| nota densa em HTML canônico, 200 KB, só inline | 78,3 ms |
| a mesma com as capabilities de HTML | 24,9 ms |
| aplicar cor a uma seleção numa nota de 500 KB | 0,365 ms |

Conceder as capabilities de HTML sai mais barato pelo mesmo motivo das inline:
esconder tags produz menos células do que mostrá-las.

P4 encontrou mais uma varredura desnecessária. Aplicar cor custava 1,8 ms porque
`range_is_editable` percorria **todos** os blocos e todos os seus graphemes para
decidir sobre um punhado de caracteres. Passou a perguntar apenas pelos graphemes da
seleção: 1,8 ms → 0,365 ms.

### 39.2 B.7 — escopo entregue

Links, listas, tarefas, citações e callouts, cada um atrás de sua própria capability.
A regra comum é que **estrutura não é texto**: o marcador de uma lista, a caixa de uma
tarefa e o destino de um link são atributos, e digitar não alcança nenhum deles.

**Link.** O rótulo é projetado e editável; o destino é escondido e protegido, e não
recebe caret em byte algum — há teste que varre cada byte do destino. O destino é
casado com parênteses balanceados, que é exatamente o caso que o §26.16 nomeia como
prova de que o parser do leitor não serve: `inline.rs` pararia no primeiro `)` e leria
`https://example.com/a_(b` em vez de `https://example.com/a_(b)`.

Nada no projetor abre, resolve ou normaliza uma URL — é um intervalo de bytes com uma
marca de proteção. A prova é que `javascript:alert(1)`, `file:///etc/passwd` e um
caminho com `../../` recebem exatamente o mesmo tratamento de qualquer outro destino:
só o rótulo é desenhado, e os bytes são preservados.

**Listas, tarefas, citações e callouts.** O conteúdo passa a ser editável e o marcador
continua protegido e sem caret. `ToggleTask` troca **um único caractere**, o de dentro
dos colchetes: o marcador, o texto e o metadado de conclusão do Core estão todos fora
do patch, de modo que alternar uma tarefa não pode reescrever seu texto nem forjar uma
data de conclusão. É uma transação e um passo de undo, pela mesma autoridade.

### 39.3 O marcador escondido e o glifo desenhado

Esconder o `- ` de uma lista perderia a informação de que aquilo é uma lista. A
solução é a separação que o documento inteiro defende: a **fonte** mantém seu `- `, e
a **tela** desenha um glifo derivado do tipo do node — `•` para lista, `☐`/`☑` para
tarefa, `│` para citação. E só quando o marcador está de fato escondido: um bloco
ainda mostrado como fonte tem seu marcador na tela e não pode ganhar um segundo.

### 39.4 Defeitos encontrados

1. **Prefixos de bloco eram invisíveis sempre.** `- item` aparecia como `item` mesmo
   sem capability alguma, o que é a mentira que este documento proíbe. A visibilidade
   de um prefixo passou a depender da capability do seu bloco, como já era para os
   delimitadores de mark. Parágrafo e heading são a exceção deliberada: são editáveis
   desde B.3 e B.4 e não têm flag própria.
2. **Um caret pousava dentro do destino de um link.** O destino é invisível, então
   sem uma regra própria a costura entre `](` e a URL era uma posição legal — e um
   caret no meio de uma URL que o leitor nem vê. `seam_is_legal` passou a recusar
   offsets no início de, e dentro de, `LinkDestination`, como já fazia para
   `BlockPrefix`.
3. **Um caret pousava depois do metadado de conclusão.** O fim do bloco incluía o
   `-->`, então digitar ali cairia fora da tarefa. `content_end` passou a excluir
   `Metadata` além de `LineEnding`.
4. **Uma fixture de teste inventava a grafia do metadado.** Eu escrevera
   `note-it:concluida=`; a grafia canônica que o Core escreve e reconhece é
   `note-it:completed_at=`. Corrigida a fixture, não o código — o teste é que estava
   errado sobre o formato.

### 39.5 Uma sensibilidade a carga, registrada e não rotulada

Durante uma execução do gate completo, `isolated_desktop_discard_conflict_keeps_open_and_success_closes_canonical_state`
falhou com "timed out: mapped desktop note". Investigado em vez de rotulado:

- isolado, passa em 2,95 s — exatamente o tempo da baseline;
- o estágio `workspace-tests` sozinho passou duas vezes seguidas;
- o gate completo, repetido, passou.

`workspace-tests` é `cargo test --workspace`, que roda os 382 testes da TUI junto com
um teste que abre uma **janela real** e espera o compositor mapeá-la dentro de um
prazo. A suíte da TUI leva 8 s e nada nela é patológico, mas o gate completo encadeia
vários estágios e a máquina ainda está ocupada quando esse chega.

Não é defeito do código desta fase — nenhuma linha de GUI ou de Core foi tocada — e o
prazo **não** foi aumentado, porque isso esconderia um deadlock se um dia houvesse um.
Fica registrado como o que é: um teste de janela real com prazo de relógio é sensível
à carga da máquina, e a suíte da TUI cresceu bastante nesta fase.

## 40. Fase 5.0D.4B.P — fechamento de performance

### 40.1 A pergunta que B.P existe para responder

O §26.13 pede invalidação incremental "apenas se necessária e provada". B.P mediu o
custo **inteiro** de uma tecla — reprojetar, planejar, aplicar — e a resposta foi que
sim, era necessária:

| Nota | tecla, antes | tecla, depois |
|---|---|---|
| 1 KB | 0,203 ms | 0,051 ms |
| 4 KB | 1,553 ms | 0,076 ms |
| 16 KB | 6,347 ms | 0,584 ms |
| 64 KB | **31,662 ms** | 2,511 ms |
| 128 KB | **84,451 ms** | 5,421 ms |
| 512 KB | — | 17,783 ms |

O orçamento é um quadro a 60 Hz, 16 ms. Antes, uma tecla estourava o quadro a partir
de ~32 KB; agora só a partir de ~256 KB.

### 40.2 O que foi feito: construção preguiçosa por bloco

Não foi reparse incremental do lexer. A medição mostrou que o custo não estava em
analisar a fonte — 100 KB projetam em 3 ms — e sim em construir **células e slots de
todos os blocos** quando quem pergunta precisa de um.

Células e caret slots passaram a ser construídos sob demanda, por bloco, com cache.
`slot_at_offset` localiza o bloco por bisseção e pergunta só a ele; `slot_for_offset`
consulta o bloco e seus dois vizinhos; mover o cursor pergunta ao bloco atual e ao
seguinte apenas quando sai dele. `slots()`, que constrói tudo, continua existindo para
os testes e não é usado em nenhum caminho de tecla.

Efeito colateral medido: construir um `VisualDocument` de 100 KB caiu de **48,57 ms
para 2,50 ms**.

### 40.3 Budgets consolidados

| Cenário | Medida (release) | Budget |
|---|---|---|
| 1 KB | 0,03 ms | 5 ms |
| 100 KB | 2,50 ms | 200 ms |
| 20.000 linhas | 9,79 ms | 500 ms |
| linha de 100.000 | 0,85 ms | 200 ms |
| Unicode denso, 600 KB | 3,63 ms | 500 ms |
| histórico após 500 edições | 200 entradas, 13,25 MB | 16 MiB |

Os cinco modos de falha proibidos continuam excluídos, agora com um teste consolidado
que falha aqui se qualquer gate anterior regredir.

### 40.4 Dois erros de medição, corrigidos

1. **O benchmark media a si mesmo.** A primeira versão pegava o caret com
   `document.slots()`, que constrói todos os blocos — exatamente o que a preguiça
   evita. Com isso o número da tecla não melhorava, embora o editor tivesse melhorado
   12 vezes. Corrigido para resolver o caret contra um bloco, que é o que a aplicação
   faz.
2. **O orçamento de quadro era exigido em debug.** O gate roda em debug, que é cerca
   de uma ordem de grandeza mais lento, e ali o orçamento afirma algo sobre o `rustc
   -O0` e não sobre o editor. Os números documentados vêm de `--release`, e é lá que o
   quadro é exigido; debug mantém um teto generoso que ainda falha num travamento.

Pelo mesmo motivo, as asserções de **razão** de crescimento passaram a valer somente
acima de um piso de ruído: a razão entre duas medidas de microssegundos mede o
relógio. Abaixo do piso vale um teto absoluto, que continua pegando uma quadrática —
meio megabyte de uma linha só, se fosse quadrático, levaria segundos e não
milissegundos.

### 40.5 O que fica fora

Reparse incremental do **lexer** não foi implementado e não foi provado necessário: a
projeção crua de 1 MB leva 13 ms e a de 100 KB, 3 ms. Se um dia notas dessa ordem
forem comuns, a medida já está aqui e a decisão terá números.
