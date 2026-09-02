# Arquitetura do Note-it

## Visão geral da arquitetura

O Note-it possui uma autoridade única e headless de domínio/persistência, cercada por adaptadores. O adaptador de desktop adiciona integração nativa ao sistema e incorpora o editor em TypeScript; futuros adaptadores de CLI e MCP devem chamar o mesmo Core em vez de recriar suas regras.

```text
                         ┌───────────────────────────────┐
                         │ noteit-core (crate headless)  │
                         │ domínio + persistência XDG    │
                         └───────▲───────────────▲───────┘
                                 │               │
                     adaptador desktop chama Core│ CLI headless chama Core
                                 │               │
 ┌───────────────────────────────┴────────┐     ┌┴──────────────────────────────┐
 │ note-it GUI: GTK4 + layer-shell+WebKit │     │ noteit CLI: binário headless  │
 │ instância única, ciclo de vida, janelas│     │ terminal puro/scripts/agentes │
 └───────────────────────────────▲────────┘     └───────────────────────────────┘
                                 │ mensagens JSON
 ┌───────────────────────────────▼────────┐
 │ TypeScript WebView: Vite + Tiptap      │
 │ editor, serializador Markdown, sanitize│
 └────────────────────────────────────────┘
```

A direção das dependências é imposta pelo Cargo: tanto o pacote de desktop (`note-it`) quanto o pacote de CLI (`noteit-cli`) dependem de `noteit-core`, enquanto `noteit-core` possui zero dependências de desktop ou CLI. Os scripts `scripts/check-core-boundary` e `scripts/check-cli-boundary` impedem que bibliotecas de GUI (GTK, GDK, WebKitGTK, layer-shell, Wayland, Niri) entrem em qualquer componente headless.

## Componentes do Core (`noteit-core`, Rust)

`NoteItCore` é a fachada concisa voltada aos adaptadores. Atualmente, expõe operações canônicas para listar, ler e buscar notas ativas, derivar catálogos de metadados, listar a lixeira, carregar o estado de Study e resolver puramente caminhos de store (`StorePaths`). Seus consumidores de escrita e ciclo de vida utilizam o mesmo `StorageManager` mantido por essa fachada, garantindo uma única implementação para gravações atômicas, recência, lixeira, backup e persistência de Study.

- `noteit-core/src/model.rs`: modelos de dados de notas, parsing de metadados e projeção `NoteSummary`. `split_front_matter` e `body_of` são compartilhados com a busca, garantindo que "o corpo da nota" signifique a mesma coisa em todos os lugares.
- `noteit-core/src/filter.rs`: `NoteFilter` tipado com correspondência AND de tags/propriedades via `semantic_identity`, e `NoteSelectorError` seguro.
- `noteit-core/src/task.rs`: scanner único de tarefas compartilhado entre leitura e escrita — estados de checkbox, hierarquia de indentação, exclusão de blocos de código cercados (fenced code), extração de `completed_at` em ISO 8601, o `TaskRef` otimista e a reescrita de linha que conclui ou reabre uma tarefa. Uma tarefa falsa dentro de um bloco de código é invisível para ambos, pois existe apenas um scanner.
- `noteit-core/src/write.rs`: cada mutação como uma operação de domínio tipada — `WriteOperation`, `NoteMutation`, `WriteOutcome`, `WriteError` — além de `apply_over_live_body`, a regra para aplicar uma mutação sobre o texto que o editor mantém em memória mas ainda não salvou. Ambos os adaptadores executam essa mesma implementação.
- `noteit-core/src/coordination.rs`: o lease do escritor. Um `flock` consultivo por store, em um diretório de runtime nomeado após esse store, com verificações de propriedade e permissões que falham de forma segura (fail-closed).
- `noteit-core/src/control.rs`: o protocolo privado de controle — JSON prefixado por tamanho sobre um socket Unix local, versionado e delimitado. **Não é uma interface pública**; consulte ADR-038.
- `noteit-core/src/hashing.rs`: um digest determinístico e documentado (FNV-1a 64) para a chave de store e para a referência de tarefa. Nunca utiliza `DefaultHasher`, cuja estabilidade não é garantida.
- `noteit-core/src/warning.rs`: anomalias não fatais de leitura tipadas e estruturadas (`ReadWarning`, `ReadBatch<T>`) retornadas pelas operações do Core sem impressão no terminal.
- `noteit-core/src/metadata.rs`: Tags e Propriedades textuais validadas, identidade semântica compartilhada com a normalização da busca, agrupamentos determinísticos de cores e entradas tipadas de catálogo. Os adaptadores nunca precisam de `serde_yaml::Value`.
- `noteit-core/src/storage.rs`: resolução pura de diretórios XDG (`StorePaths`), abertura de store estritamente somente leitura (`open_read_only`), E/S de Markdown em disco, salvamento atômico e operações de armazenamento utilizadas pelos adaptadores GUI e CLI.
- `noteit-core/src/search.rs`: normalização de acentos (accent folding), correspondência, snippets, rótulos e ordenação — funções puras sobre `(Uuid, &str)`.
- `noteit-core/src/trash.rs`: exclusão recuperável e listagem somente leitura da lixeira. Consulte ADR-028.
- `noteit-core/src/backup.rs`: snapshots locais, retenção e política de manifesto. Consulte ADR-029 e ADR-032.
- `noteit-core/src/study.rs`: modelo versionado de `study.json` e agendador Ladder-v1.
- `noteit-core/src/assets.rs`: validação de imagens, identificadores, referências de storage e regras de importação.
- `noteit-core/src/autopaste.rs` e `timer.rs`: máquinas de estado e políticas headless; a integração com área de transferência e notificações permanece no host de desktop.
- `noteit-core/src/settings.rs` e `state.rs`: configuração versionada da aplicação e estado operacional, com persistência atômica e sem dependências de janelas.
- `atomic_file.rs` e `visible_text.rs` são módulos de implementação privada compartilhados por essas capacidades públicas.

Os testes do Core utilizam exclusivamente stores sintéticos temporários. Os gates canônicos headless são:

```bash
env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noteit-core
scripts/check-core-boundary
```

## Componentes do adaptador CLI (`noteit-cli`, Rust)

- `main.rs`: ponto de entrada do binário `noteit`, despachando argumentos e mapeando códigos de saída padrão.
- `cli.rs`: parsing de linha de comando com Clap, comandos primários em PT-BR e aliases internacionais (`listar`/`list`, `ler`/`read`, `buscar`/`search`, `tags`, `propriedades`/`properties`, `tarefas`/`tasks`, `lixeira`/`trash`, `status`, `ajuda`/`help`, `versao`/`version`), além da opção global `--json`.
- `outcome.rs`: o que um comando produziu, antes de qualquer decisão sobre como apresentá-lo — `Outcome`, `CommandError`, os nomes canônicos de `Command` e `CliResponse` (código de saída mais ambos os canais como dados). Ambos os renderizadores leem isso e nenhum lê o outro.
- `output.rs`: o renderizador humano. Apresentação no terminal, estilização ANSI, detecção de NO_COLOR/não-TTY e sanitização de segurança para terminal (`sanitize_for_terminal`).
- `machine.rs`: o renderizador para máquinas. O schema JSON público como DTOs explícitos, um documento versionado por execução, tokens estáveis em inglês para cada decisão que um consumidor tome. Consulte `docs/machine-interface.md` e ADR-041.
- `authority.rs`: a decisão de quem grava. Adquire o lease de escritor quando estiver livre e grava através do Core; quando estiver retido, envia a alteração para quem o detém através do socket privado; quando estiver retido e inacessível, falha de forma segura (fail-closed) e não altera nada. Nunca recorre a gravações paralelas contornando outro escritor.
- `lib.rs`: interface programática (`run_with_args`), parsing de filtros, despacho para o Core, códigos de saída padrão, tratamento de entrada padrão para `--stdin` e seleção do renderizador.

```text
                    ┌──▶ output::render   ──▶ frases, estilização, datas locais, prefixos de 8 caracteres
domain ──▶ Outcome ─┤
        │            └──▶ machine::render  ──▶ um documento JSON, UTC, UUIDs completos, tokens estáveis
        └──▶ CommandError
```

Os dois adaptadores compartilham a operação e nada mais. Uma frase humana nunca é analisada para construir um documento, e nenhum caminho de escrita existe duplicado.

O binário CLI possui zero dependências gráficas e é testado de forma headless:

```bash
env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS cargo test -p noteit-cli
scripts/check-cli-boundary
```

## Componentes do adaptador de desktop (`src`, Rust)

- `main.rs`: ponto de entrada e despachante CLI de instância única (`gtk::Application`).
- `app.rs`: estado da aplicação, coordenação do ciclo de vida e tratamento de IPC.
- `cli.rs`: parsing de linha de comando (`--background`, `new`, `toggle`, `show`, `hide`, `quit`).
- `layer_shell.rs`: inicialização do Wayland Layer Shell, âncoras, camadas e gerenciamento de foco.
- `note_window.rs`: wrapper de janela GTK4 incorporando webviews WebKitGTK 6.0.
- `webview_bridge.rs`: mensageria bidirecional entre o host Rust e o WebView TypeScript. Os tipos de mensagens reutilizam os tipos de domínio do Core, enquanto o canal de envio real ao WebView permanece específico do desktop.
- `write_authority.rs`: a instância de desktop como gravador do store. `claim` adquire o lease, vincula e restringe as permissões do socket, retornando um `WriteAuthority` **apenas em caso de sucesso absoluto**; `AppContext` mantém isso por valor, de modo que uma instância em execução que não possua seu store não é um estado que o programa possa descrever. A inicialização é recusada em vez de operar de forma degradada — consulte ADR-039. `serve` então executa o pipeline de gravação externa: congela o editor, coleta seu texto ativo, aplica a mutação, faz o commit, adota a nota, avança a geração, entrega a nota confirmada de volta e aguarda a página confirmar sua adoção.

### O caminho de gravação quando uma nota está aberta

```text
noteit adicionar        lease retido pela instância de desktop
      │                        │
      └── socket de controle ──┤
                               ├─ 1. recusar se estiver ocultando, saindo ou excluindo
                               ├─ 2. congelar o editor  ── depois ──▶ ler seu texto
                               ├─ 3. incorporar esse texto ao documento com commit
                               ├─ 4. aplicar a mutação sobre *esse* texto
                               ├─ 5. realizar o commit pelo escritor atômico
                               ├─ 6. adotar a nota, geração += 1
                               ├─ 7. entregar de volta à página e descongelar
                               └─ 8. aguardar a página confirmar que a adotou
```

O passo 2 ocorre nessa ordem e em nenhuma outra: ler primeiro criaria uma janela de tempo na qual uma tecla digitada cairia, sendo subsequentemente sobrescrita. O passo 6 é o que torna recusável qualquer mensagem anterior ainda em trânsito. O passo 8 é a própria confirmação da página — `ExternalWriteApplied`, informando a nota, a requisição e a geração —, pois a mera avaliação de um script não garante que o documento tenha sido adotado. Tudo a partir do passo 5 já ultrapassou o ponto de commit; portanto, o passo 8 pode apenas decidir se a resposta conterá um aviso (warning), nunca transformar uma gravação concluída em falha. Consulte ADR-038 e ADR-039.

Antes de qualquer uma dessas etapas: o store é reivindicado. Não existe janela, documento ou salvamento automático até que este processo seja seu único escritor; se não puder ser, relata o fato e encerra a execução.

```text
inicialização do desktop
   → preparar coordenação        ─┐
   → adquirir lease de escritor   ├─ qualquer falha: liberar, explicar, sair com código não-zero
   → vincular e restringir socket─┘
   → construir a aplicação
```

## Componentes de front-end (TypeScript / Vite / Tiptap)

- `ui/src/main.ts`: ponto de entrada do WebView e bootstrap da bridge.
- `ui/src/bridge/externalWrite.ts`: a contraparte da página em uma gravação externa — congelar, snapshot, adotar e uma fila que armazena todas as edições que chegam durante o processo para que nenhuma seja perdida.
- `ui/src/editor/documentLock.ts`: um gate ProseMirror via `filterTransaction`. Enquanto uma gravação está em trânsito, nada altera o documento — nem digitação, nem comandos, nem plugins. O documento é liberado pelo host e unicamente pelo host: a página não possui timeouts que pudessem liberá-lo prematuramente enquanto um commit ainda estiver em andamento.

### Quando a página poderá ser editada novamente

Assim que o snapshot é enviado, exatamente duas respostas liberam o documento — e existe uma terceira que não libera:

| host diz | arquivo | página | resultado |
| --- | --- | --- | --- |
| `AbortExternalWrite` | inalterado | ainda corresponde ao arquivo | descongelar, drenar a fila, mesma geração |
| `ApplyExternalDocument`, adotado | alterado | agora o texto com commit | descongelar, drenar, nova geração, `ExternalWriteApplied` |
| `ApplyExternalDocument`, **não** adotado | alterado | obsoleto | **permanece retido**: sem descongelar, sem drenar a fila, geração antiga, `ExternalWriteApplyFailed` |

A terceira linha é a que merece ser expressa com clareza. A gravação está no disco e é reportada como confirmada com um `ui_sync_warning`; a janela não é liberada porque uma janela liberada editaria sobre uma geração que o host já ultrapassou, e qualquer salvamento que fizesse seria recusado — trabalho digitado e silenciosamente perdido. A nota exibe esse aviso, e reabri-la é o procedimento de recuperação. Consulte ADR-040.

A página mantém uma única fase, e ela é a única coisa que decide o que pode acontecer a seguir:

```text
  idle ──iniciar──▶ syncing ──aviso de lentidão──▶ slow
                     │                              │
                     ├──abortar─────────────────────┤──▶ idle           (nada gravado)
                     ├──aplicar, adotado────────────┤──▶ idle, gen N+1  (gravado e exibido)
                     └──aplicar, não adotado────────┴──▶ unsynchronised
```

`unsynchronised` não possui nenhuma aresta de saída. Cada transição consulta a fase primeiro, de modo que um callback que já estava na fila quando a fase mudou — por exemplo, um aviso de lentidão cujo temporizador acabou de ser cancelado — encontra uma fase na qual não pode atuar e não faz nada. É isso que torna o estado terminal em vez de apenas tardio; cancelar o temporizador serve apenas para manter o fluxo padrão organizado.

- `ui/src/editor/`: configuração do editor Tiptap, extensões, atalhos de teclado e barra de ferramentas.
- `ui/src/markdown/`: parser de Markdown, serializador e conversores de ida e volta (round-trip).
- `ui/src/flashcards/`: a definição única de flashcards no ProseMirror e a sessão de revisão efêmera.
- `ui/src/study/`: identidades semânticas SHA-256, um parser de catálogo Tiptap reutilizável sob demanda e projeções puras de vencimentos/heatmaps/sequências. `ui/src/ui/studyHub.ts` e o `flashcardPanel.ts` existente renderizam o catálogo global e as sessões agendadas dentro do WebView atual.
- `ui/src/math/`: o mecanismo matemático, independente do editor — `lexer.ts`, `parser.ts`, `evaluate.ts`, `document.ts` (as linhas de uma nota, avaliadas de cima para baixo) e `format.ts`. Não tem conhecimento sobre ProseMirror; `ui/src/editor/math.ts` é a única ponte entre os dois, lendo linhas do documento e aplicando os resultados como decorações.
- `ui/src/units/`: a tabela de unidades e a conversão em si — `types.ts`, `registry.ts` e `convert.ts`. Não tem conhecimento sobre parsing, notas ou editor: são dados mais aritmética. A dependência segue em direção única, `math/parser.ts` → `units/registry.ts`, pois o parser precisa saber o que conta como unidade; nada em `units/` faz referência inversa. Essa fronteira é também o limite atrás do qual uma futura fonte de câmbio monetário deve se posicionar — consulte ADR-025.
- `ui/src/editor/find.ts`: busca e substituição no documento ativo — correspondência por bloco de texto, decorações de destaque e `Replace All` como uma única transação ProseMirror. `ui/src/editor/linkPaste.ts` é a colagem de URL sobre a seleção, protegida pela allowlist de links do próprio aplicativo.
- `ui/src/ui/searchPalette.ts`, `ui/src/ui/findBar.ts` e `ui/src/ui/trashPanel.ts`: os três painéis. Todos residem na página e não em uma segunda janela, possuem seus próprios atalhos de teclado e não fazem parte do documento. `ui/src/ui/status.ts` é a linha no rodapé da nota que informa o resultado de uma ação de dados; não é um diálogo modal e não remove nada da visão do leitor.
- `ui/src/ui/metadataPanel.ts`: o editor único de Tags/Propriedades e a faixa responsiva de tags. Lida apenas com valores tipados, renderiza com `textContent`/`value` e adota um rascunho somente após o host reconhecer o commit do Core.
- `ui/src/markdown/assetReference.ts`: o que uma nota tem permissão para expressar sobre uma imagem — o formato de referência gerenciada, os limites de largura, os três alinhamentos e a única função que transforma uma referência armazenada em algo que a página pode carregar. Uma responsabilidade do Markdown e não do editor, pois o sanitizador a reconhece na entrada e o editor a grava na saída.
- `ui/src/editor/image.ts` e `ui/src/editor/imageView.ts`: o nó de imagem e sua interface própria — as duas formas armazenadas e a conversão bidirecional entre elas, além de alças, controles de alinhamento e redimensionamento em transação única que nunca removem o foco nem movem a seleção.
- `ui/src/flashcards/`: uma projeção do documento ProseMirror ativo. `extract.ts` reconhece sintaxe inline e estrutural, mantém as faces como fragmentos de documento e expande fontes reversíveis em itens de revisão; `session.ts` gerencia apenas a ordem efêmera, o cursor e o estado de revelação. O plugin do editor em `ui/src/editor/flashcardMark.ts` pinta delimitadores e mantém a contagem em tempo real sem uma transação de documento, enquanto `ui/src/ui/flashcardPanel.ts` renderiza fragmentos de snapshot com o `DOMSerializer` da nota, nunca recebendo um editor ou função de despacho.
- `ui/src/capture/autoPaste.ts`: o que uma nota se torna quando uma captura chega — a mesma divisão de texto simples que o próprio ProseMirror usa para `text/plain`, os três delimitadores e a transação única que anexa ao final sem remover o foco, mover a seleção ou rolar a página. Não lê nenhuma área de transferência: a página não participa da observação dela.
- `ui/src/timer/`: a contagem regressiva em si, independente do DOM — `engine.ts` (a máquina de estados sobre um prazo, com o relógio injetado), `format.ts` (`MM:SS` / `H:MM:SS` e os textos de cada estado) e `controls.ts` (qual botão se aplica a cada estado, como um valor em vez de quatro ramificações em um manipulador). Não sabe nada sobre o cabeçalho ou o popover; `ui/src/ui/timerPanel.ts` é a única coisa que une os dois e gerencia o único redesenho pendente em vez de um intervalo.
- `ui/src/bridge/`: manipuladores de mensagens nativas para carregamento, salvamento, tema e alterações de fonte.
- `ui/src/styles/`: temas minimalistas, definições de cores de papel e estilização de layout.

## Onde reside a pesquisa

A pesquisa é uma capacidade do domínio, não da interface:

```text
NoteItCore::search_notes
   ↓ delega ao leitor existente do StorageManager
storage (read_note_bodies_by_recency)
   ↓ pares (Uuid, body) — front matter já removido
search.rs (normalizar → comparar → trecho → ordenar → limitar)
   ↓  Vec<SearchResult>
webview_bridge (SearchResults { request_id, results })
   ↓
searchPalette.ts (renderiza e solicita por note_id)
```

Duas propriedades dessa organização são deliberadas.

O frontend nunca nomeia um arquivo. Ele recebe valores `note_id` gerados pelo host e envia um de volta; não há mensagem na bridge que transporte um caminho, portanto não há nada para percorrer no sistema de arquivos.

E nada em `Vec<SearchResult>` necessita de exibição gráfica. A CLI chama `NoteItCore::search_notes` sobre a mesma implementação de storage e busca que a GUI utiliza; GTK e WebKit entram em cena apenas após o resultado chegar ao adaptador de desktop.

## Fluxo de metadados semânticos

```text
painel de metadados confirma um rascunho tipado + Markdown atual do editor
  → mensagem do WebView endereçada por UUID
  → Core valida NoteMetadata
  → clona o NoteDocument candidato mantido em memória
  → inclui texto pendente (e atualiza updated_at apenas se o texto diferir)
  → StorageManager::save_note_atomic (backup → temporário → commit por rename)
  → adota o candidato confirmado
  → confirma o MetadataView exato persistido
```

`note_it` permanece de propriedade exclusiva da aplicação. `tags`, `properties` e valores desconhecidos de nível raiz no YAML residem no mesmo front matter, mas o YAML em si nunca cruza a bridge. Valores desconhecidos são mantidos como detalhe de persistência do Core; comentários, âncoras e formatação original não podem ser representados pelo `serde_yaml` e podem se normalizar quando um salvamento real de conteúdo/aparência/metadados resserializa o arquivo. Uma operação de abrir/fechar sem alterações não realiza gravações e, portanto, permanece idêntica em bytes.

Os catálogos são derivados sob demanda, varrendo apenas o diretório `notes/`. Não existe `tags.json`, banco de dados ou cache sujeito a ficar desatualizado; notas na lixeira desaparecem do catálogo porque seu arquivo não está ativo, e a restauração faz com que retornem naturalmente.
