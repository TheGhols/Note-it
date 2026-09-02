# Arquitetura do Note-it

## Visão geral da arquitetura

Note-it tem uma autoridade de domínio/persistência headless e adaptadores em torno dele. O adaptador de desktop adiciona integração nativa do sistema e incorpora o editor TypeScript; futuros adaptadores CLI e MCP devem chamar o mesmo Core em vez de recriar suas regras.

```text
                         ┌───────────────────────────────┐
                         │ noteit-core (crate headless)  │
                         │ domínio + persistência XDG    │
                         └───────▲───────────────▲───────┘
                                 │               │
                  adaptador desktop chama o Core │ CLI headless chama o Core
                                 │               │
 ┌───────────────────────────────┴────────┐     ┌┴──────────────────────────────┐
 │ GUI note-it: GTK4 + layer-shell + WebKit│     │ CLI noteit: binário headless   │
 │ instância única, ciclo de vida, janelas│     │ terminal / script / agente     │
 └───────────────────────────────▲────────┘     └───────────────────────────────┘
                                 │ mensagens JSON
 ┌───────────────────────────────▼────────────┐
 │ TypeScript WebView: Vite + Tiptap          │
 │ editor, serializador Markdown, sanitizador │
 └────────────────────────────────────────────┘
```

A direção da dependência é imposta por Cargo: tanto o pacote desktop (`note-it`) quanto o pacote CLI (`noteit-cli`) dependem de `noteit-core`, enquanto `noteit-core` tem zero dependências de desktop ou CLI. `scripts/check-core-boundary` e `scripts/check-cli-boundary` evitam que bibliotecas GUI (GTK, GDK, WebKitGTK, layer-shell, Wayland, Niri) entrem em qualquer componente headless.

## Componentes do Core (`noteit-core`, Rust)

`NoteItCore` é a pequena fachada voltada para o adaptador. Atualmente, ele expõe operações canônicas para listar, ler e pesquisar notas ao vivo, derivar catálogos de metadados, listar lixo, carregar estado de estudo e resolver puramente caminhos do store (`StorePaths`). Seus consumidores de gravação e ciclo de vida usam o mesmo `StorageManager` mantido por essa fachada, portanto ainda há uma implementação de gravações atômicas, recência, lixo, backup e persistência de estudo.

- `noteit-core/src/model.rs`: modelos de dados da nota, análise de metadados e projeção `NoteSummary`. `split_front_matter` e `body_of` são compartilhados com a pesquisa, então “o corpo da nota” significa a mesma coisa em todos os lugares.
- `noteit-core/src/filter.rs`: `NoteFilter` tipado, com correspondência AND de tags/propriedades por `semantic_identity`, e `NoteSelectorError` seguro.
- `noteit-core/src/task.rs`: um scanner de tarefa compartilhado por leitura e gravação - estados de caixa de seleção, hierarquia de profundidade, exclusão de código protegido, extração ISO 8601 `completed_at`, o `TaskRef` otimista e a reescrita de linha que completa ou reabre uma tarefa. Uma tarefa falsa dentro de uma cerca é invisível para ambos, pois existe apenas um scanner.
- `noteit-core/src/write.rs`: toda mutação como uma operação de domínio digitada — `WriteOperation`, `NoteMutation`, `WriteOutcome`, `WriteError` — mais `apply_over_live_body`, a regra para aplicar uma mutação sobre o texto que um editor está segurando, mas não salvou. Ambos os adaptadores executam esta implementação.
- `noteit-core/src/coordination.rs`: o lease de escrita. Um `flock` consultivo por store, em um diretório de tempo de execução nomeado a partir do digest desse store, com verificações de propriedade e permissão que falham de modo seguro (fail-closed).
- `noteit-core/src/control.rs`: o protocolo de controle privado — com prefixo de comprimento JSON sobre um soquete Unix local, versionado e limitado. **Não é uma interface pública**; consulte ADR-038.
- `noteit-core/src/hashing.rs`: um resumo determinístico e documentado (FNV-1a 64) para a chave do store e a referência da tarefa. Nunca `DefaultHasher`, cuja estabilidade não é prometida.
- `noteit-core/src/warning.rs`: anomalias de leitura não fatais estruturadas e digitadas (`ReadWarning`, `ReadBatch<T>`) retornadas por operações Core sem impressão de terminal.
- `noteit-core/src/metadata.rs`: tags validadas e propriedades textuais, identidade semântica compartilhada com normalização de busca (folding), baldes de cores determinísticas e entradas de catálogo digitadas. Os adaptadores nunca precisam de `serde_yaml::Value`.
- `noteit-core/src/storage.rs`: resolução de diretório XDG pura (`StorePaths`), abertura de store estritamente somente leitura (`open_read_only`), E/S de disco Markdown, salvamento atômico e as operações de persistência e storage usadas pelos adaptadores GUI e CLI.
- `noteit-core/src/search.rs`: normalização de acentos (accent folding), correspondência, snippets, rótulos e ordenação — funções puras sobre `(Uuid, &str)`.
- `noteit-core/src/trash.rs`: exclusão recuperável e listagem de lixo somente leitura. Consulte ADR-028.
- `noteit-core/src/backup.rs`: instantâneos locais, retenção e política de manifesto. Consulte ADR-029 e ADR-032.
- `noteit-core/src/study.rs`: o modelo `study.json` versionado e o agendador Ladder-v1.
- `noteit-core/src/assets.rs`: validação de imagens, identificadores, referências de storage e regras de importação.
- `noteit-core/src/autopaste.rs` e `timer.rs`: máquinas de estados e políticas headless; a área de transferência e a integração de notificação permanecem no host da área de trabalho.
- `noteit-core/src/settings.rs` e `state.rs`: configuração e estado operacional do aplicativo versionado, com persistência atômica, mas sem dependência de janelas.
- `atomic_file.rs` e `visible_text.rs` são módulos de implementação privados compartilhados por esses recursos públicos.

Os testes Core usam apenas stores sintéticos temporários. As portas canônicas headless são:

```bash
env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noteit-core
scripts/check-core-boundary
```

## Ferramentas de desenvolvimento (`scripts/`)

Os gates de qualidade vivem no repositório e o CI os consome. `scripts/check` é a
autoridade sobre o que precisa passar; `.github/workflows/ci.yml` decide *onde* cada
coisa roda e chama os mesmos estágios, um por step, para que um run vermelho aponte
o gate. Nenhum comando de qualidade é reescrito no workflow, no CONTRIBUTING ou no
guia de desenvolvimento.

- `scripts/doctor`: diagnóstico somente leitura do ambiente (`rust`, `frontend`, `all`).
  Verifica presença e versão; nunca instala, eleva privilégio ou altera a máquina.
  A versão mínima do Rust é lida de `rust-version` no `Cargo.toml`, e não redeclarada.
- `scripts/check`: os gates, como estágios atômicos (`rust-format`, `rust-check`,
  `rust-clippy`, `core-boundary`, `cli-boundary`, `core-tests`, `cli-tests`,
  `workspace-tests`, `frontend-install`, `frontend-lint`, `frontend-test`,
  `frontend-build`) e como agregados (`rust`, `frontend`, `all`). Fail-closed: para no
  primeiro que falha e propaga o código dele.
- `scripts/build.sh`: build release reprodutível do frontend e do workspace inteiro,
  com lockfile congelado, que confere os binários antes de dizer que terminou.
- `scripts/check-core-boundary`, `scripts/check-cli-boundary`: continuam sendo a
  autoridade sobre dependências de desktop nos crates headless. `scripts/check` os
  chama; não os substitui nem afrouxa.
- `scripts/note-it-isolated`, `scripts/test-isolation`, `scripts/run-note-it`:
  inalterados. O harness de isolamento é alcançado por `cargo test --workspace`
  através de `tests/isolation.rs`, e por isso `scripts/check` não o executa de novo.

Os três entrypoints resolvem a raiz do repositório a partir do próprio caminho, então
funcionam de qualquer diretório de trabalho.

## Componentes do adaptador CLI (`noteit-cli`, Rust)

- `main.rs`: Ponto de entrada para o binário `noteit`, despachando argumentos e mapeando códigos de saída padrão.
- `cli.rs`: análise de linha de comando usando Clap com comandos primários PT-BR e aliases internacionais (`listar`/`list`, `ler`/`read`, `buscar`/`search`, `tags`, `propriedades`/`properties`, `tarefas`/`tasks`, `lixeira`/`trash`, `status`, `ajuda`/`help`, `versao`/`version`), mais a opção global `--json`.
- `outcome.rs`: o que um comando produziu, antes que alguém decida como dizê-lo - `Outcome`, `CommandError`, os nomes canônicos `Command` e `CliResponse` (código de saída mais ambos os canais como dados). Ambos os renderizadores leem isso e nenhum lê o outro.
- `output.rs`: o renderizador humano. Apresentação do terminal, estilo ANSI e higienização da segurança do terminal (`sanitize_for_terminal`). Aqui também mora `OutputContext` — o que **um** canal pode fazer (aceita cor, largura conhecida, desenha blocos) — e `Channels`, o par. Cada canal é decidido a partir dele mesmo: um terminal na saída padrão não diz nada sobre a saída de erro, e um aviso estilizado dentro de um arquivo redirecionado é um aviso que ninguém consegue filtrar. Como as capacidades são um valor, e não uma chamada a `is_terminal()` espalhada pelo código, toda a matriz (estilizado, puro, largo, estreito, `dumb`) é alcançável em teste sem terminal físico.
- `welcome.rs`: a apresentação de `noteit` sem argumentos. Logotipo `NOTE-IT` em blocos, versão vinda de `CARGO_PKG_VERSION`, uma linha sobre o que o Note-it é e cinco comandos por onde começar. Função pura de `OutputContext` e da versão do pacote: não lê, não abre e não grava nada. Cor e largura variam de forma independente — retirando toda a cor e reduzindo à largura mínima, nenhuma informação se perde.
- `machine.rs`: o renderizador da máquina. O esquema público JSON como DTOs explícitos, um documento versionado por execução, tokens em inglês estáveis ​​para cada decisão tomada por um consumidor. Consulte `docs/machine-interface.md` e ADR-041.
- `authority.rs`: a decisão de quem escreve. Adquire o lease quando ele está livre e grava pelo Core; quando está ocupado, envia a alteração ao detentor pelo soquete privado; quando está ocupado e inacessível, falha de modo seguro e não altera nada. Nunca tenta contornar outro gravador.
- `lib.rs`: Interface programática (`run_with_args`), análise de filtro, despacho Core, códigos de saída padrão, tratamento de entrada padrão para `--stdin` e escolha do renderizador.

```text
                    ┌──▶ output::render   ──▶ frases, estilo, datas locais, prefixos de 8 caracteres
domínio ─▶ Outcome ─┤          │
       │            │          └──▶ welcome::render ──▶ a apresentação, por largura e por cor
       │            └──▶ machine::render  ──▶ um documento JSON, UTC, UUIDs completos, tokens estáveis
       └──▶ CommandError
```

Os dois adaptadores compartilham a operação e não compartilham mais nada. Uma frase humana nunca é analisada para construir um documento e nenhum caminho de gravação existe duas vezes.

O binário CLI não tem nenhuma dependência gráfica e é testado headless:

```bash
env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS cargo test -p noteit-cli
scripts/check-cli-boundary
```

## Componentes do adaptador de desktop (`src`, Rust)

- `main.rs`: Ponto de entrada e despachante CLI de instância única (`gtk::Application`).
- `app.rs`: estado do aplicativo, coordenação do ciclo de vida e manipulação de IPC.
- `cli.rs`: análise de linha de comando (`--background`, `new`, `toggle`, `show`, `hide`, `quit`).
- `layer_shell.rs`: Wayland Layer Shell inicialização, âncoras, camadas e gerenciamento de foco.
- `note_window.rs`: GTK4 wrapper de janela incorporando WebKitGTK webviews 6.0.
- `webview_bridge.rs`: Mensagens bidirecionais entre o host Rust e TypeScript WebView. Os tipos de mensagens reutilizam tipos de domínio Core, enquanto o caminho de envio real WebView permanece específico do desktop.
- `write_authority.rs`: a instância do desktop como gravador do store. `claim` pega o lease, vincula e restringe o soquete e retorna um `WriteAuthority` **somente em caso de sucesso completo**; `AppContext` mantém isso por valor, portanto, uma instância em execução que não detém o lease de seu store não é um estado que o programa possa descrever. A inicialização recusa em vez de degradar — consulte ADR-039. `serve` então executa o pipeline de gravação externa: congela o editor, coleta seu texto ativo, modifica, confirma, adota, avança a geração, devolve a nota confirmada e espera que a página diga que a aceitou.

### O caminho de gravação quando uma nota está aberta

```text
noteit adicionar        lease mantido pela instância desktop
      │                        │
      └── soquete de controle ─┤
                               ├─ 1. recusar se estiver ocultando, saindo ou excluindo
                               ├─ 2. congelar o editor ── então ──▶ ler seu texto
                               ├─ 3. incorporar esse texto ao documento a confirmar
                               ├─ 4. aplicar a mutação sobre *esse* documento
                               ├─ 5. confirmar pelo gravador atômico
                               ├─ 6. adotar o documento; geração += 1
                               ├─ 7. devolvê-lo à página e descongelar
                               └─ 8. esperar a página confirmar que o adotou
```

A etapa 2 ocorre nessa ordem e em nenhuma outra: a leitura primeiro deixa uma lacuna na qual uma tecla é digitada e essa tecla é então reescrita. A etapa 6 é o que torna recusáveis ​​todas as mensagens ainda em trânsito da execução anterior. O passo 8 é a própria palavra da página — `ExternalWriteApplied`, nomeando a nota, a solicitação e a geração — porque um script avaliado não diz nada sobre se um documento foi adotado. Tudo a partir da etapa 5 já passou do ponto de commit, portanto a etapa 8 só pode decidir se a resposta contém um aviso; ele nunca pode transformar uma gravação concluída em uma falha. Consulte ADR-038 e ADR-039.

Antes de tudo, o processo adquire o store. Nenhuma janela, documento ou salvamento automático existe até que esse processo seja o único gravador; se não puder sê-lo, informa o problema e encerra.

```text
inicialização do desktop
   → preparar coordenação       ─┐
   → adquirir lease do gravador  ├─ qualquer falha: liberar, explicar, sair com código diferente de zero
   → vincular e restringir soquete ─┘
   → construir o aplicativo
```

## Componentes de front-end (TypeScript / Vite / Tiptap)

- `ui/src/main.ts`: ponto de entrada do Webview e bootstrap da ponte.
- `ui/src/bridge/externalWrite.ts`: a metade da página de uma gravação externa - congelar, capturar instantâneo, adotar e uma fila que contém todas as edições que chegam enquanto isso, para que nenhuma seja perdida.
- `ui/src/editor/documentLock.ts`: uma porta ProseMirror `filterTransaction`. Enquanto uma gravação está em andamento, nada altera o documento - nem a digitação, nem um comando, nem um plugin. O documento é liberado pelo host e somente pelo host: a página não tem tempo limite que possa devolvê-lo enquanto um commit ainda estiver em andamento.

### Quando a página poderá ser editada novamente

Depois que o instantâneo é divulgado, exatamente duas respostas liberam o documento - e há uma terceira que não:

| host diz | arquivo | página | resultado |
| --- | --- | --- | --- |
| `AbortExternalWrite` | inalterado | ainda corresponde ao arquivo | descongelar, drenar a fila, mesma geração |
| `ApplyExternalDocument`, adotado | mudado | agora o texto confirmado | descongelar, drenar, nova geração, `ExternalWriteApplied` |
| `ApplyExternalDocument`, **não** adotado | mudado | obsoleto | **continua retido (stays held)**: sem descongelamento, sem drenagem, geração antiga, `ExternalWriteApplyFailed` |

A terceira linha é aquela que vale a pena indicar claramente. A gravação está no disco e é relatada como confirmada com `ui_sync_warning`; a janela não é liberada, porque uma janela liberada seria editada em uma geração pela qual o host já passou e cada salvamento feito seria recusado - trabalho digitado e perdido silenciosamente. A nota diz isso, e reabri-la é a recuperação. Consulte ADR-040.

A página contém uma fase e é a única coisa que decide o que pode acontecer a seguir:

```text
  idle ──iniciar──▶ syncing ──aviso de lentidão──▶ slow
                     │                      │
                     ├──abortar─────────────┤──▶ idle           (nada gravado)
                     ├──aplicar, adotado────┤──▶ idle, gen N+1  (gravado e exibido)
                     └──aplicar, não adotado┴──▶ unsynchronised
```

`unsynchronised` não possui aresta de saída (outgoing edge). Cada transição solicita a fase primeiro, então um callback que já estava na fila quando a fase mudou — um aviso lento cujo temporizador acabou de ser cancelado, obviamente — encontra uma fase na qual pode não agir e não faz nada. É isso que torna o estado terminal, em vez de meramente terminal; cancelar o cronômetro também apenas mantém o caso comum organizado.
- `ui/src/editor/`: configuração do editor Tiptap, extensões, atalhos de teclado e barra de ferramentas.
- `ui/src/markdown/`: Markdown analisador, serializador e conversores de ida e volta.
- `ui/src/flashcards/`: a definição única do flashcard ProseMirror e a sessão de revisão efêmera.
- `ui/src/study/`: identidades semânticas SHA-256, um analisador de catálogo Tiptap reutilizável sob demanda e projeções puras de vencimento/mapa de calor/sequência. `ui/src/ui/studyHub.ts` e o `flashcardPanel.ts` existente renderizam o catálogo global e o agendamento dentro do WebView atual.
- `ui/src/math/`: o mecanismo matemático, independente do editor — `lexer.ts`, `parser.ts`, `evaluate.ts`, `document.ts` (as linhas de uma nota, avaliadas de cima para baixo) e `format.ts`. Não sabe nada sobre ProseMirror; `ui/src/editor/math.ts` é a única coisa que une os dois, lendo as linhas do documento e pintando os resultados como decoração.
- `ui/src/units/`: a tabela de unidades e a própria conversão — `types.ts`, `registry.ts` e `convert.ts`. Não sabe nada sobre análise, notas ou editor: são dados mais aritmética. A dependência é executada em uma direção, `math/parser.ts` → `units/registry.ts`, porque o analisador precisa saber o que conta como uma unidade; nada em `units/` se refere de volta. Essa aresta de dependência é também a fronteira que uma futura fonte de moeda deve respeitar – ver ADR-025.
- `ui/src/editor/find.ts`: encontre e substitua o documento ativo - correspondência por bloco de texto, decorações de destaque e `Replace All` como uma transação ProseMirror. `ui/src/editor/linkPaste.ts` é a colagem de URL sobre seleção, controlada pela lista de permissões de links do próprio aplicativo.
- `ui/src/ui/searchPalette.ts`, `ui/src/ui/findBar.ts` e `ui/src/ui/trashPanel.ts`: os três painéis. Todos ficam na página e não em uma segunda janela, possuem suas chaves e não fazem parte do documento. `ui/src/ui/status.ts` é a linha no final da nota que informa o que uma ação de dados fez; não é um diálogo e não tira nada do leitor.
- `ui/src/ui/metadataPanel.ts`: o editor único de tags/propriedades e a faixa de tags responsiva. Ele lida apenas com valores digitados, renderiza com `textContent`/`value` e adota um rascunho somente depois que o host reconhece o commit de Core.
- `ui/src/markdown/assetReference.ts`: o que uma nota pode dizer sobre uma imagem — o formato de referência gerenciado, os limites de largura, os três alinhamentos e a única função que transforma uma referência armazenada em algo que a página pode carregar. Uma preocupação de Markdown, e não do editor, porque o sanitizador a reconhece na entrada e o editor a escreve na saída.
- `ui/src/editor/image.ts` e `ui/src/editor/imageView.ts`: o nó da imagem e sua própria interface — os dois formulários armazenados e o percurso entre eles, e as alças, controles de alinhamento e redimensionamento de transação única que nunca tiram o foco ou movem a seleção.
- `ui/src/flashcards/`: uma projeção do documento ProseMirror ativo. `extract.ts` reconhece a sintaxe inline e estrutural, mantém os lados como fragmentos de documentos e expande fontes reversíveis em itens de revisão; `session.ts` possui apenas a ordem efêmera, o cursor e o estado de revelação. O plugin do editor em `ui/src/editor/flashcardMark.ts` pinta delimitadores e mantém a contagem ao vivo sem uma transação de documento, enquanto `ui/src/ui/flashcardPanel.ts` renderiza fragmentos de instantâneo com o `DOMSerializer` da nota e nunca recebe um editor ou função de despacho.
- `ui/src/capture/autoPaste.ts`: o que uma nota se torna quando uma captura chega — a própria divisão de texto simples que ProseMirror usa para `text/plain`, os três delimitadores e a transação única que é anexada no final sem tirar o foco, mover a seleção ou rolar. Ele não lê nenhuma área de transferência: a página não participa da observação de uma.
- `ui/src/timer/`: a contagem regressiva em si, independente do DOM — `engine.ts` (a máquina de estado dentro de um prazo, com o relógio injetado), `format.ts` (`MM:SS` / `H:MM:SS` e as palavras para cada estado) e `controls.ts` (qual botão se aplica em qual estado, como um valor em vez de quatro ramificações dentro de um manipulador). Não sabe nada sobre o cabeçalho ou o popover; `ui/src/ui/timerPanel.ts` é a única coisa que une os dois e possui o único redesenho pendente em vez de um intervalo.
- `ui/src/bridge/`: manipuladores de mensagens nativos para carregar, salvar, tema e alterações de fonte.
- `ui/src/styles/`: Temas minimalistas, definições de cores de papel e estilo de layout.

## Onde reside a pesquisa

A pesquisa é uma capacidade do domínio, não da interface:

```text
NoteItCore::search_notes
   ↓ delega ao leitor existente do StorageManager
store (read_note_bodies_by_recency)
   ↓ pares (Uuid, corpo) — front matter já removido
search.rs (normalizar → comparar → trecho → ordenar → limitar)
   ↓  Vec<SearchResult>
webview_bridge (SearchResults { request_id, results })
   ↓
searchPalette.ts (renderiza e solicita por note_id)
```

Duas propriedades desse arranjo são deliberadas.

O frontend nunca nomeia um arquivo. Ele recebe valores `note_id` gerados pelo host e envia um de volta; não há mensagem na ponte que conduza um caminho, portanto não há nada para atravessar.

E nada através de `Vec<SearchResult>` precisa de exibição. Um futuro CLI chama `NoteItCore::search_notes` pela mesma implementação de storage e busca que a GUI utiliza; GTK e WebKit entram somente depois que o resultado chega ao adaptador de desktop.

## Fluxo de metadados semânticos

```text
painel de metadados confirma um rascunho tipado + Markdown atual do editor
  → mensagem do WebView endereçada por UUID
  → Core valida NoteMetadata
  → clona o NoteDocument candidato mantido em memória
  → inclui texto pendente (e altera updated_at somente se o texto diferir)
  → StorageManager::save_note_atomic (backup → temporário → confirmação por rename)
  → adota o candidato confirmado
  → confirma o MetadataView exato que foi persistido
```

`note_it` permanece de propriedade do aplicativo. `tags`, `properties` e valores desconhecidos de nível superior YAML vivem no mesmo front matter, mas o próprio YAML nunca cruza a ponte. Valores desconhecidos são mantidos como detalhe de persistência Core; comentários, âncoras e formatação original não podem ser representados por `serde_yaml` e podem normalizar quando um salvamento real de conteúdo/aparência/metadados reserializa o arquivo. Abrir e fechar uma nota intocada não executa gravação e, portanto, permanece idêntica em bytes.

Os catálogos são derivados sob demanda, varrendo apenas `notes/`. Não há `tags.json`, banco de dados ou cache para ficar obsoleto; o lixo desaparece de um catálogo porque seu arquivo não está ativo e a restauração faz com que ele retorne naturalmente.
