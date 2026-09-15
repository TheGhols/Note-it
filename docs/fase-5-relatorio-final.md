# Fase 5 — Relatório Final

**Data:** 15/09/2026 · **Documento de fechamento de ciclo** · Fase 5: TUI, acabamento, empacotamento e distribuição

Este relatório funciona sozinho. Quem não acompanhou a execução deve conseguir
entender daqui onde a Fase 5 começou, o que ela entregou e em que estado o
projeto ficou.

---

## A. Resumo executivo

| Item | Valor |
| --- | --- |
| **Objetivo da Fase 5** | Interface interativa de terminal (`noteit-tui`) e empacotamento para distribuição |
| **Resultado** | **BLOCKED** — por escopo não executado da 5.0E, não por defeito |
| **SHA final** | `6ecb83f3904ad153fadd0e01e482a59e1916638a` |
| **CI final** | Run `34889270358`, SHA `6ecb83f`, **success** (reexecução) |
| **Instalação diária** | `note-it 0.1.0.r214.g159fe2df-1` — **intocada** |
| **Dados reais** | 42 notas, fingerprint `99e0aa9f…` — **inalterados** |
| **Árvore** | limpa |
| **Fase 6** | **NÃO INICIADA** — liberação é decisão do dono (ver §N) |

**O que está fechado.** 5.0A a 5.0D.5, 5.0D.R5, 5.0D.R6, 5.0E-GUI e 5.1A estão
entregues, testadas e com CI verde. O editor Visual da TUI existe, tem paridade
semântica provada com a GUI em matemática e flashcards, e a suíte da TUI foi de
165 para 426 testes.

**O que impede o PASS.** A 5.0E, como está escrita no roadmap, pede seis coisas
que não foram executadas: cinco binários no pacote, `PKGBUILD-git`, atualização
de `scripts/build.sh`, validação em chroot limpo, corte da tag `v0.1.0` e
publicação no AUR. Nenhuma delas é um defeito descoberto agora — são escopo que
nunca foi feito. Detalhamento em §E e §L.

---

## B. Baseline da Fase 5

| Item | Valor |
| --- | --- |
| Último commit antes da Fase 5 | `15b86d2` — *docs(roadmap): close Phase 4.3R with CI run #113 verification* (06/09/2026) |
| Primeiro commit da Fase 5 | `ba87439` — *docs(tui): scope and architecture for Phase 5.0 (TUI + packaging)* (06/09/2026) |
| Commits com identificador `5.x` | 54 |

---

## C. Subfases executadas

Somente subfases que existem em git e em documentação. Nada inventado.

| Subfase | Objetivo | Resultado | Evidência |
| --- | --- | --- | --- |
| **5.0A** | Arquitetura e escopo da TUI + empacotamento. Nenhum `.rs`, `Cargo.lock` byte-idêntico | Concluída | `ba87439`, `docs/tui.md` |
| **5.0B** | Crate `noteit-tui`, shell de terminal e gate de fronteira | Concluída | `d6ed712` |
| **5.0C** | Modo leitura, navegação e apresentação | Concluída | `12381ec` |
| **5.0D.R0** | Contrato de mutação concorrente fechado antes da edição | Concluída | `fc6d39a`, `docs/tui.md` §10 |
| **5.0D** | Modo edição, mutação e concorrência transacional | Concluída após bloqueio e revisão `7196a2f` | `docs/tui.md` §12 |
| **5.0D.1** | Fidelidade de Markdown e compatibilidade com notas da GUI | Concluída — 30 testes de fidelidade, 85 no crate | `docs/tui.md` §13 |
| **5.0D.2** | Editor nativo no painel direito | Concluída — 45 testes de editor, 144 no crate | `docs/tui.md` §14 |
| **5.0D.3** | Polimento visual, responsividade, mouse e formatação inline | Concluída, com correções R1 e R2 | `docs/tui.md` §15 |
| **5.0D.4A** | Arquitetura do editor Visual — gate exclusivamente arquitetural | Concluída após 4 rodadas e 3 revisões adversariais independentes; 28 propriedades verificáveis | `docs/tui.md` §§1–28 |
| **5.0D.4B** | Implementação do editor Visual — 14 portões em ordem (B.1, P0, B.2, P1, B.3, B.4, P2, B.5, P3, B.6, P4, B.7, B.P, B.R) | Concluída — suíte de 165 → 426 testes; 5 custos superlineares encontrados pelos gates de performance | `docs/tui.md` §§29–41 |
| **5.0D.5** | Paridade semântica: matemática e flashcards | Concluída — motor **portado**, não reinterpretado; paridade provada por fixtures cruzadas afirmadas pelos dois lados | `docs/tui.md` §42 |
| **5.0D.R5** | Correção comportamental do editor Visual (9 commits, `59b2d99`→`aa082c8`) | Passou nos testes automatizados e **falhou no reteste manual** | registrado por R6 em `docs/tui.md` §44 |
| **5.0D.R6** | Fechamento comportamental do editor Visual (14 commits, `9c08642`→`c4033cf`) | Concluída | `docs/tui.md` §44 |
| **5.0E-GUI** | Ativação a frio pelo barramento de sessão e pacote diário (7 commits) | Concluída | `159fe2d`, `a59864a`, `ef035ad`, `50b40a5`, `31736db`, `7933204`, `6ecb83f` |
| **5.1A** | Ponte para um cliente de IA externo (`noteit-agent-bridge`) | Concluída como biblioteca; **nenhum consumidor gráfico construído** | `a7eddfc`, `docs/second-brain.md` |
| **5.0E** | Empacotamento completo, auditoria e distribuição | **NÃO CONCLUÍDA** — ver §E e §L | — |

---

## D. Contratos arquiteturais consolidados pela Fase 5

Lista curta, no contexto correto. Não é documentação arquitetural completa.

1. **Mutação sempre revisionada (5.0D.R0/5.0D).** Toda escrita da TUI passa por
   `authority::perform_at` com a `revision` originalmente lida. Conflito externo
   aborta sem sobrescrever um byte, sem retry automático.
2. **Terminal restaurado exatamente (5.0D).** `T0` é um snapshot `termios`
   imutável tomado antes de qualquer alteração, provado em pseudoterminal real.
3. **Três camadas do editor Visual (5.0D.4A).** `Lexeme` (partição física
   byte-exact), `Node` (árvore semântica) e `ProjectionRun` (saída visual
   derivada), com `Generation` estritamente monotônica por sessão.
4. **Precedência de sintaxe (5.0D.4A).** Fenced code, code span balanceado e
   escape vêm **antes** de qualquer candidato HTML.
5. **Recusa nunca é silêncio (5.0D.4B).** O que o editor Visual não sabe editar
   continua visível como fonte e recusa a edição nomeando a causa.
6. **`EditorMode` e o contrato R6.** `Visual` é o editor em que a nota abre
   quando tem projeção utilizável; `Markdown/Fonte` é o modo avançado, que se
   identifica pelo nome. A formatação visual (`Alt+F`) pertence **exclusivamente**
   ao modo Visual: fora dele o menu não abre e o usuário recebe aviso. Decidido
   em `16a182e`.
7. **Paridade por fixture, nunca por comparação (5.0D.5).** Duas implementações
   comparadas entre si provam que concordam, não sobre o quê. As fixtures são
   geradas da implementação canônica e afirmadas pelos dois lados.
8. **GUI principal, Core canônico (ADR-061, fechamento).** Paridade semântica é
   obrigatória; paridade visual não é.
9. **Versionamento `0.MINOR.PATCH` (ADR-062, fechamento).** `0.1.100 → 0.2.0`,
   deliberadamente não SemVer. A versão anda quando uma unidade funcional fecha
   com PASS.

---

## E. Incidentes e regressões

### E.1 — Painel INFO / Atalhos recortado (corrigido)

**Problema.** O botão INFO abria o painel de atalhos com ~20 px de altura para
~1.700 px de conteúdo. Reportado com evidência visual pelo usuário.

**Causa-raiz — o elemento de montagem, não o CSS.** `.note-shortcuts` foi escrito
com a geometria de `.note-trash`: `position: absolute`, `left` e `right` fixos
sem largura própria, `max-height: calc(100% - var(--note-header-height) - 16px)`.
Toda essa régua mede o **bloco contentor**. A lixeira, a busca e o localizar
montam em `#app`, que é estático, então o bloco contentor delas é a viewport — a
nota. O painel de atalhos montava em `#note-controls-left`, que é
`position: relative` e `flex: 0 0 auto` dentro de um cabeçalho de altura fixa:
**26 px de altura** e a largura dos botões. Com `100%` valendo 26 px,
`calc(100% - 28px - 16px)` dá **−18 px**, preso em zero pelo navegador.

**Medição no WebKitGTK real** (mesmo motor que a aplicação usa):

| Viewport | Antes | Depois |
| --- | --- | --- |
| 220×300 | 55,2×19,7 px — **0,9%** de 2105 px | 200,9×252,2 px |
| 420×360 | 210,8×19,7 px — **1,2%** de 1689 px | 397,9×311,3 px |
| 760×560 | 187,1×19,7 px — **1,1%** de 1731 px | 732,8×508,3 px |

**Correção.** Uma linha: `mount: menuMount` → `mount: appRoot` em
`ui/src/main.ts`. Nenhuma alteração de CSS. A 220 px a regra
`@media (max-width: 260px)` finalmente alcança o painel e empilha atalho e
descrição em uma coluna.

**Commit:** `50b40a5`.

**Lacuna de teste fechada.** O teste existente *"scrolls instead of growing past
the note"* passou durante todo o defeito, porque afirmava que havia `100%` e
nunca **de quê**. Dois testes novos fecham isso pelo pai; ambos foram verificados
falhando no código com o defeito e passando com a correção.

### E.2 — `16a182e`: dois fatos distintos

Separados deliberadamente, para não deixar documentação histórica incorreta.

**Fato 1 — reprovação de `rust-format` (real, corrigida).** `16a182e` deixou
`noteit-tui/tests/phase_5_0_d_3_regressions.rs` fora do formato canônico.
`scripts/check rust` reprovava no **primeiro estágio** em `1186224`. O commit
nunca foi publicado, logo o `rust-format` do CI nunca o viu. Corrigido em
`c4033cf` com `cargo fmt --all` e nada além — nenhuma asserção alterada. Isto
confirmou na prática o risco C-8 da auditoria: um commit local sem CI é um
commit sem prova.

**Fato 2 — hipótese PTY/Alt+F (levantada e depois FALSIFICADA).** Foi levantada a
hipótese de que `16a182e` havia deixado dois testes PTY de
`terminal_lifecycle.rs` incompatíveis com o contrato R6 do `Alt+F`, por ter
atualizado `phase_5_0_d_3_regressions.rs` e não aquele arquivo. A hipótese foi
**falsificada** por reproduções controladas contendo o próprio commit:

| Ambiente | Resultado |
| --- | --- |
| Local, `TERM=xterm-256color` | ok, 0,33 s |
| `TERM=dumb` | ok, 0,26 s |
| `TERM` ausente | ok, 0,26 s |
| 15 repetições consecutivas | 15/15 ok |
| 32 loops de CPU sobre 8 núcleos | ok, 0,41 s |
| Núcleo único (`taskset -c 0`) | ok, 0,34 s |
| Container `archlinux:latest`, root, `TERM=dumb` | ok, 0,37 s |
| Container 2 vCPU, suíte completa (13 testes) | 13/13 ok, 0,80 s |

A correlação secundária levantada — os dois serem os únicos que enviam `Ctrl+S`,
sugerindo disputa de lease — também não se sustenta: `coordination.rs` usa
`file.try_lock()`, que é **não-bloqueante**.

**Nenhuma alteração foi aplicada aos testes com base nessa hipótese.** A regra de
parada foi acionada quando a evidência a derrubou. É melhor corrigir uma hipótese
do que alterar código para encaixar a teoria.

### E.3 — Hang do CI (não reproduzido)

| Item | Valor |
| --- | --- |
| Primeira execução | Run `34889270358`, SHA `6ecb83f`, iniciada 14/09 19:50:01 UTC |
| Duração até o cancelamento | **4h10m** (19:50:01 → 15/09 00:00:02) |
| Passo travado | 25 de 25 — *Run Rust Unit Tests* (`cargo test --workspace`) |
| Passos 1–24 | todos **success**, incluindo os sete gates de fronteira |
| Testes envolvidos | `test_real_pty_format_palette_writes_canonical_color` e `test_real_pty_future_color_and_highlight_compose_without_selection` — os únicos 2 dos 13 de `terminal_lifecycle.rs` que não reportaram |
| Sinal no log | ambos "has been running for over 60 seconds" às 20:00:53; nada depois |
| Reproduções locais/container | **todas passaram** (tabela em §E.2) |
| Timeout explícito | o workflow **não** define `timeout-minutes`; o teto é o padrão de 6 h do GitHub |
| Reexecução do mesmo SHA | **success**, Rust job 11m03s (00:05:10 → 00:16:13) |
| Os dois testes na reexecução | `ok` em ambos os passos: *Headless TUI Tests* (00:12:06) e *Run Rust Unit Tests* (00:16:03) |

**Conclusão, baseada somente na evidência:** `CI stall / incidente não
reproduzido`. Nenhuma causa específica é atribuída, porque nenhuma foi
demonstrada. Não há evidência de risco para comportamento de produção: o defeito
não se manifestou em nenhum dos oito ambientes controlados, incluindo a própria
imagem do CI. Nenhum teste PTY foi alterado e nenhum código de produção foi
alterado por causa deste incidente.

**Follow-up não bloqueante:** o workflow não tem `timeout-minutes`. Um estol
custou 4h10m de runner antes de ser notado. Ver §L.

---

## F. Testes

| Suíte | Quantidade | Resultado | Ambiente | Duração |
| --- | --- | --- | --- | --- |
| `scripts/check frontend` (4 estágios) | 62 arquivos, **1342 testes** | **PASS** (exit 0) | local | 25,3 s (testes) |
| `scripts/check rust` (20 estágios) | 3819 asserções agregadas | **PASS** (exit 0) | local | — |
| `terminal_lifecycle` (TUI, PTY real) | 13 | **PASS** | local | 0,34 s |
| `terminal_lifecycle` | 13 | **PASS** | container `archlinux:latest`, 2 vCPU, root | 0,80 s |
| `shortcuts_panel` (frontend) | 17 (2 novos) | **PASS** | local | 0,10 s |
| Regressão dirigida dos 2 testes PTY | 2 × 15 repetições | **30/30 PASS** | local | — |
| `check()` do `makepkg` (release) | Core + CLI + TUI | **PASS** | build do pacote | — |
| CI remoto — todos os jobs | — | **PASS** | GitHub Actions | 11m03s |

Os dois testes novos de `ui/tests/shortcuts_panel.test.ts` foram verificados
**falhando** no código com o defeito (2 failed / 15 passed) e passando com a
correção (17 passed). Um teste que não reprova não é um gate.

---

## G. CI remoto

| Item | Valor |
| --- | --- |
| Run ID | `34889270358` |
| SHA | `6ecb83f3904ad153fadd0e01e482a59e1916638a` |
| Job *Frontend Checks & Tests* | **success** |
| Job *Rust Checks & Tests* | **success** — 25/25 passos |
| Duração (reexecução) | 00:05:10 → 00:16:13 UTC = **11m03s** |
| Testes antes problemáticos | `ok` nos dois passos que os executam |

A evidência está vinculada ao SHA final, não a "último CI verde".

---

## H. Packaging

| Item | Valor |
| --- | --- |
| Versão canônica anterior | `0.1.0` |
| Versão canônica nova | **`0.1.1`** (regra `PATCH < 100` da ADR-062) |
| Fontes de versão sincronizadas | `Cargo.toml` (canônica), `Cargo.lock`, `ui/package.json`, `PKGBUILD` (`_appver`, e `pkgver()` passou a **derivar** do `Cargo.toml` do commit empacotado) |
| Pacote construído | `note-it-0.1.1.r223.g79332043-1-x86_64.pkg.tar.zst` |
| SHA256 do pacote | `6d43f67b0378b7927ca0801259af617ec7177b341f5f599ae5ca7c9dc7d966a1` |
| Conteúdo | 4 binários + `ui/dist` + `.desktop` + serviço D-Bus + 8 ícones + licença |
| Arquivos fora de `/usr` | **nenhum** |
| Diferença vs. pacote instalado | **um único arquivo** — o bundle JS (`index-BsvhiNZK.js` → `index-D2_7DjYi.js`), que é exatamente a correção |
| Frontend **dentro do pacote** verificado | sim — extraído e medido no WebKitGTK: `panel_parent: app`, 200,9×252,2 / 397,9×311,3 / 732,8×508,3 |
| **Instalação realizada** | **NÃO.** A promoção depende do fechamento da 5.0E |

**Nota de build.** O `makepkg` foi executado com `-d` porque o makedepend `pnpm`
não é satisfazível pelos repositórios nesta máquina — está instalado como pacote
npm local em `~/.local/lib/node_modules`, sem dono no pacman. Isto tem
consequência direta para a 5.0E: **um build em chroot limpo falharia**, porque um
chroot não tem `~/.local`. Ver §L.

---

## I. Segurança dos dados

| Item | Valor |
| --- | --- |
| Store usado nos testes | diretórios temporários descartáveis + `scripts/note-it-isolated` (barramento D-Bus privado **e** XDG isolado) |
| Notas reais | **42**, antes e depois |
| Fingerprint antes | `99e0aa9f7093ef50d4eae2c3ad630ab3b028a0ed2bde4c6858dfcd635b33dfa2` |
| Fingerprint depois | `99e0aa9f7093ef50d4eae2c3ad630ab3b028a0ed2bde4c6858dfcd635b33dfa2` |
| Store real usado acidentalmente | **não** — verificado por fingerprint idêntico também em `~/.local/share/note-it` isoladamente |
| Migração involuntária | **nenhuma** — a 0.1.1 não altera formato de nota, front matter, `state.json` nem `config.toml` |

O isolamento por barramento privado é obrigatório e não opcional: o Note-it é uma
`GApplication` de instância única, e isolar apenas XDG já deixou uma nota de teste
cair no store real durante a Fase 3.7.

---

## J. Rollback

| Item | Valor |
| --- | --- |
| Artefato | `packaging/arch/note-it-0.1.0.r214.g159fe2df-1-x86_64.pkg.tar.zst` |
| SHA256 | `8a3db5a8ba3503bf339b458fd5316606175329fd9ad2a11637f837f75038c3d6` |
| Procedimento | `sudo pacman -U packaging/arch/note-it-0.1.0.r214.g159fe2df-1-x86_64.pkg.tar.zst` |
| Compatibilidade de dados | total nos dois sentidos — `package()` escreve apenas dentro de `$pkgdir`, e a 0.1.1 não muda formato algum |
| Situação final | **preservado e utilizável.** Nenhum passo do fechamento destruiu o caminho de reversão |
| Retenção | N = 0.1.1 (construída, não instalada), N−1 = 0.1.0 (instalada). Não existe N−2 a remover |

---

## K. Git final

| Item | Valor |
| --- | --- |
| Branch | `main` |
| HEAD | `6ecb83f3904ad153fadd0e01e482a59e1916638a` |
| `origin/main` | `6ecb83f3904ad153fadd0e01e482a59e1916638a` |
| `HEAD == origin/main` | **sim** (0 ahead / 0 behind) |
| `git status --short` | **vazio** |

**Commits desta execução de fechamento**, do mais antigo ao mais novo:

| SHA | Mensagem |
| --- | --- |
| `50b40a5` | `fix(gui)`: o painel de atalhos abre na nota, não na fila de botões |
| `31736db` | `docs(roadmap)`: fecha a auditoria da Fase 6 e a direção de produto |
| `c4033cf` | `style(tui)`: `cargo fmt` no teste de regressão do Alt+F |
| `7933204` | `build(release)`: versão 0.1.1 — o painel de atalhos corrigido |
| `6ecb83f` | `build(arch)`: fixa o pacote no commit da 0.1.1 |

Publicados em `origin/main` por fast-forward (`a7eddfc..6ecb83f`). Nenhum
`--force`, nenhuma reescrita de histórico.

**Resíduo removido no encerramento:** `packaging/arch/Note-it/` — clone git bare
criado pelo `source=("git+…")` do `makepkg`, 4,8 MB, remote do próprio projeto,
HEAD `6ecb83f`. Comprovado como resíduo regenerável antes da remoção.

---

## L. Débitos técnicos e follow-ups

### Bloqueantes da 5.0E

| # | Item | Estado |
| --- | --- | --- |
| L1 | Pacote deve conter **5 binários** (`note-it`, `noteit`, `noteit-mcp`, `noteit-embed`, `noteit-tui`) | contém **4** — `noteit-embed` ausente |
| L2 | `PKGBUILD-git` para trunk | **não existe** |
| L3 | Atualização de `scripts/build.sh` para a 5.0E | último toque é `6a71ab2`, da Fase 4.3D |
| L4 | Validação de integridade em **chroot limpo** | **não executada** — e falharia hoje: o makedepend `pnpm` só existe como instalação npm local em `~/.local`, que um chroot não tem |
| L5 | Corte da versão `v0.1.0` | **0 tags no repositório.** Deliberado — o `PKGBUILD` registra que inventar uma tag seria publicar uma release que ninguém decidiu fazer, e a ADR-062 passou a governar versionamento |
| L6 | Publicação no AUR | **não executada** |

### Não bloqueantes

| # | Item | Classificação |
| --- | --- | --- |
| L7 | Workflow de CI sem `timeout-minutes`; um estol custou 4h10m de runner | backlog |
| L8 | `.gitignore` cobre `/packaging/arch/src/` e `/pkg/` mas não o clone `Note-it/` que o `makepkg` cria | backlog |
| L9 | Instalação diária em `0.1.0` enquanto `0.1.1` está construída e validada | aguarda decisão de promoção |

Nenhum item acima expande escopo agora. L1–L6 são o que falta da 5.0E; L7–L9 são
candidatos a backlog ou Fase 6.

---

## M. Estado da Fase 6

**FASE 6 NÃO INICIADA.**

Nenhuma linha de código da Fase 6 foi escrita. O planejamento está completo e
commitado em `docs/roadmap.md` (`31736db`): sete macrofases, ~40 subfases, as 22
features do escopo mestre classificadas com uma etiqueta cada — 0 existentes, 6
parciais, 13 ausentes, 2 BLOCKED, 1 precisando de auditoria mais profunda — e
nove conflitos arquiteturais registrados.

**Liberação:** não decidida. Depende do veredito da 5.0E (§N).

**Próxima etapa oficial segundo o roadmap**, quando liberada: **6.0.A —
Identidade nomeável da nota**, gate arquitetural sem código de produção, que
resolve o conflito C-2 (a nota tem `id: Uuid` e nenhum título; o nome é a
primeira linha visível, que não é única e muda quando se edita a nota, de modo
que `[[Título]]` não tem alvo estável). É a raiz do grafo de dependências: sem
ela, wikilinks, aliases, backlinks, embeds e menções não têm alvo.

O roadmap registra, em 6.0.PRE, a decisão do dono de que a 5.0E precede a
implementação da Fase 6.

---

## N. Veredito final

**FASE 5: BLOCKED — FASE 6 NÃO LIBERADA**

**Causa objetiva.** A 5.0E não cumpriu seis itens do próprio contrato documentado
(L1 a L6). Não se trata de defeito descoberto no fechamento: é escopo que nunca
foi executado.

**O que explicitamente NÃO é a causa:**

* o hang do CI — reexecutado no mesmo SHA com **success**, e classificado como
  estol não reproduzido;
* o painel INFO — corrigido, medido e com CI verde;
* regressão em Core, CLI, MCP, TUI ou GUI — não há; todos os gates passam;
* integridade de dados — 42 notas e fingerprint idênticos;
* estado do Git — limpo e sincronizado.

**Decisão que não é minha.** A 5.0E está bloqueada por **escopo de distribuição**
(tag, AUR, chroot, quinto binário), não por defeito técnico e não por
infraestrutura. Tecnicamente, nada disso impede a Fase 6 de começar: a 6.0.A é um
gate arquitetural que não escreve código de produção e não toca em packaging. Mas
o roadmap registra que a 5.0E precede a Fase 6, e essa precedência é uma decisão
do dono do produto.

Portanto, a liberação da Fase 6 **não é tomada aqui**. Os dois caminhos
disponíveis são:

1. **Executar L1–L6** e fechar a 5.0E como escrita, então liberar a Fase 6.
2. **Revisar o escopo da 5.0E** — reconhecendo que a ADR-062 substituiu a
   política de tag `v0.1.0`, e que AUR/chroot podem virar fase própria de
   distribuição — fechando a 5.0E com escopo reduzido e documentado.

A opção 2 é a mais coerente com o que o projeto já decidiu, mas trocar o contrato
de uma fase é decisão de produto, não de execução.
