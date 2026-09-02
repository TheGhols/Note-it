# Registro de alterações

Todas as alterações notáveis deste projeto serão documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/pt-BR/1.0.0/) e este projeto adere ao [Semantic Versioning](https://semver.org/lang/pt-BR/).

## [Não lançado]

### Corrigido
- **Fase 4.0E.3 Unificação da barreira de gravação e eliminação de escritas paralelas:** Corrigida a última lacuna de concorrência onde a CLI poderia tentar gravar diretamente em um store que o daemon de desktop ativo detém:
  - Todas as mutações do Core (`NoteMutation::Create`, `Append`, `Edit`, `Clear`, `SetProperty`, `RemoveProperty`, `CompleteTask`, `ReopenTask`, `RestoreTrash`) agora exigem passar pelo coordenador `StorageWriter`. O acesso direto a métodos atômicos de baixo nível foi tornado privado dentro de `noteit-core`.
  - A CLI `noteit` agora consulta a autoridade do store através de `WriteAuthorityClient` antes de realizar qualquer gravação. Se o store estiver sob a posse de uma instância de desktop em execução, a mutação é encaminhada de forma transparente pelo socket de controle local (`$XDG_RUNTIME_DIR/note-it/<store key>/control.sock`). Se a instância estiver ocupada ou inacessível, o `noteit` falha de modo seguro (fail-closed) sem corromper dados.
  - A instância de desktop adquire o lease exclusivo do store no momento de sua inicialização e o mantém durante todo o seu ciclo de vida. Uma segunda instância de desktop que não consiga adquirir a posse do store emite uma mensagem clara e encerra com código de saída 1, impedindo conflitos de escrita.
  - Novos testes de regressão de concorrência em `tests/concurrency.rs` e testes de processo em `tests/fail_closed.rs` validam a prevenção de corridas de gravação entre CLI e GUI.

- **Fase 4.0E.2 Adoção de UI com falha deve permanecer à prova de falhas:** Eliminada a última lacuna pós-commit deixada pela 4.0E.1:
  - `ExternalWriteBarrier.apply` chamava `this.editor.commands.setContent`, que emite uma transação sem passar pelo filtro de lock do documento (`documentLock`), permitindo que edições de digitação ocorressem brevemente durante a reconciliação. Substituído por `setDocPreservingLock`, que aplica o novo documento diretamente via transação no editor ProseMirror com metadados explícitos de sincronização externa.
  - Se a adoção do documento no WebView falhar, o editor permanece travado para evitar que edições sobre uma geração obsoleta sobrescrevam o commit real em disco. A interface exibe aviso de dessincronização (`ui_sync_warning`), orientando o usuário a reabrir a nota para recarregar o estado persistido com segurança.

- **Fase 4.0E.1 Autoridade de escrita com fail-closed e confirmação de adoção pela UI:** Resolvidas três lacunas entre o prometido pela Fase 4.0E e sua implementação:
  - O lease de escrita agora é adquirido obrigatoriamente antes de vincular o socket de controle local. Falhas ao criar o diretório de runtime, permissões incorretas ou impossibilidade de obter o `flock` abortam a inicialização com código de saída diferente de zero.
  - A confirmação de adoção pela UI agora é validada pela página WebView. O retorno `Ok` de `evaluate_javascript` apenas indicava que o script foi avaliado, mas a página precisava confirmar a aplicação efetiva. A mensagem `ApplyExternalDocument` agora transporta o ID da nota e a página responde com `ExternalWriteApplied { id, requestId, generation }` após adotar o documento e avançar a geração, ou `ExternalWriteApplyFailed { id, requestId }` caso ocorra falha. O host aceita a confirmação apenas quando ID, requisição e geração coincidem perfeitamente, com timeout delimitado de 4 segundos antes de emitir um `ui_sync_warning`.
  - A página não mais libera o documento por timeout autônomo. `EXTERNAL_WRITE_CLIENT_TIMEOUT_MS` liberava o editor 15 segundos após o snapshot, o que reintroduzia condições de corrida caso o host ainda estivesse escrevendo em disco. O mecanismo foi substituído por `EXTERNAL_WRITE_SLOW_NOTICE_MS`, que apenas altera o aviso exibido ao leitor ("Sincronização demorando…"). Apenas `ApplyExternalDocument` ou `AbortExternalWrite` descongelam o documento.
  - A semântica pós-commit permanece rigorosa: uma confirmação ausente, recusada ou não entregue é tratada como gravação confirmada (committed) com warning, nunca como falha, e nunca autoriza repetições cegas de escrita.
  - Novos testes de processo (`tests/fail_closed.rs`) testam o binário real contra leases retidos, diretórios de coordenação inutilizáveis e sockets inacessíveis, garantindo que o programa recuse a execução sem tocar nas notas ou no estado de janelas, iniciando normalmente assim que o store estiver livre.

### Adicionado
- **Fase 4.0F Interface Estável de Máquina / JSON:** `noteit --json` estabelece o primeiro contrato estável e versionado para scripts e agentes:
  - Um documento por execução: comandos bem-sucedidos emitem exatamente um documento JSON em stdout e **nada** em stderr; falhas emitem exatamente um documento JSON em stderr e nada em stdout. Todo documento termina com uma quebra de linha simples (`\n`), e nenhum canal emite sequências de escape ANSI (mesmo quando conectado a um terminal). Não há NDJSON, documentos duplicados ou prosa antes/depois do envelope.
  - Renderizado a partir do resultado tipado do domínio, nunca a partir de frases em prosa. A crate `noteit-cli` recebeu `outcome.rs` (`Outcome`, `CommandError`, nomes canônicos de `Command`, `CliResponse`) e `machine.rs` (o schema público em DTOs explícitos). `output.rs` atua estritamente como renderizador *humano* sobre os mesmos dados. Não há caminhos de gravação paralelos (`json_append`): `WriteOperation`, `NoteMutation`, `WriteOutcome`, `WriteError` e `authority::perform` são reutilizados integralmente, com testes garantindo equivalência byte a byte nos arquivos de notas entre ambos os modos.
  - `run_with_args` retorna um `CliResponse` contendo o código de saída e ambos os canais como dados estruturados. Avisos de leitura deixaram de ser emitidos via `eprint!` no meio dos comandos; agora são transportados no campo `warnings` do envelope JSON, garantindo saída limpa em stderr em caso de sucesso.
  - `--json` é global — aceito antes do comando, depois dele ou em comandos agrupados, funcionando tanto com comandos em português quanto com aliases internacionais. É uma opção e nunca uma palavra literal de argumento: `noteit adicionar ID -- --json` anexa o texto literal e permanece no modo humano. O modo é determinado a partir do parsing da flag ou por varredura exata de tokens inteiros nos argumentos brutos em caso de erro de sintaxe, respondendo `noteit --json batata` com JSON em stderr em vez de texto em português.
  - Envelope versionado: `schema_version` (1), `status` (`ok`/`warning`/`error`/`indeterminate`), `command` canônico independente do idioma (`listar` e `list` tornam-se `"list"`), `data`, `error`, `warnings`. Todas as seis chaves estão sempre presentes no objeto.
  - Tipagem forte e dados precisos: UUIDs completos em todos os identificadores (nunca prefixos truncados), timestamps UTC em RFC 3339 ou `null` (nunca datas relativas locais ou textos vagos), booleanos nativos, contagens numéricas inteiras, resultados vazios representados como `[]` com `"count": 0`. O conteúdo da nota é o Markdown bruto do Core — `sanitize_for_terminal` deliberadamente não é aplicado aos dados JSON, permitindo que aspas, barras invertidas, quebras de linha, emojis e sequências de controle realizem o ciclo completo (round-trip) sem distorções em qualquer parser JSON.
  - `commit_state` em todas as gravações: `committed`, `not_needed`, `not_committed` ou `unknown`, servindo como fonte única de verdade sobre a segurança de repetições.
  - `ui_sync_warning` como estado de primeira classe: uma gravação comitada cuja janela gráfica não confirmou a sincronização reporta `status: warning`, `commit_state: committed`, `ui_sync: {status: "warning", code: "window_not_confirmed"}` e código de saída `0` — nunca erro, nunca `not_committed`, nunca código de saída diferente de zero.
  - `WriteError::Indeterminate` como estado de primeira classe: `status: indeterminate`, `error.code: indeterminate`, `commit_state: unknown` — explicitamente distinto de `not_committed`, impedindo que uma queda de conexão socket seja tratada como falha limpa e gere duplicação de texto em retentativas automáticas.
  - Códigos de erro estáveis para todas as variantes de erro, documentados com seus respectivos códigos de saída e estados de commit.
  - O protocolo privado de controle permanece estritamente privado: identificadores de requisição, versão de protocolo, caminhos de socket e locks de escritor nunca são expostos na interface pública. Gravações diretas e via autoridade produzem o mesmo contrato público.
  - Contrato documentado detalhadamente em `docs/machine-interface.md` e fundamentado na ADR-041.
  - 32 novos testes em nível de processo executando o binário real e validando os canais de saída.

- **Fase 4.0E API de Gravação + Concorrência GUI/CLI:** A CLI agora pode alterar notas, com exatamente um processo Note-it gravando no store por vez:
  - Lease de Escritor: um `flock` consultivo em um arquivo de lock dentro do diretório de coordenação em runtime por store (`$XDG_RUNTIME_DIR/note-it/<store key>/`), compartilhado por ambos os adaptadores via `noteit_core::coordination`.
  - Autoridade de Escrita: a instância desktop adquire o lease na inicialização e atua como autoridade em um socket Unix privado local. A CLI adquire o lease por comando quando livre; quando ocupado, encaminha a mutação para o detentor; quando ocupado e inacessível, falha de modo seguro (fail-closed) sem alterar dados.
  - Mutações Atômicas: suporte a criação de notas, acréscimo de conteúdo (`adicionar`), edição de texto (`editar`), adição/remoção de tags (`tags adicionar`/`tags remover`), definição/remoção de propriedades (`propriedades definir`/`propriedades remover`), conclusão/reabertura de tarefas (`tarefas concluir`/`tarefas reabrir`) e restauração da lixeira (`lixeira restaurar`).
  - Entrada Padrão (`--stdin`): comandos de gravação aceitam texto diretamente da entrada padrão via flag `--stdin`.

- **Fase 4.0D API de Leitura Headless:** Implementada a API de leitura programática e humana na CLI `noteit`, baseada no `noteit-core`:
  - Abertura de store somente leitura via `NoteItCore::open_read_only()` e `StorageManager::open_read_only()`, sem criar diretórios ou disparar backups. Um store ausente retorna coleções vazias com código de saída 0.
  - Comandos primários em português com aliases internacionais (`listar`/`list`, `ler`/`read`, `buscar`/`search`, `tags`, `propriedades`/`properties`, `tarefas`/`tasks`, `lixeira`/`trash`).
  - Projeção `NoteSummary` com rótulos canônicos e extração de snippets sem duplicação de lógica.
  - Resolução segura de identificadores (`resolve_note_id`) por UUID completo ou prefixo único >= 8 caracteres hexadecimais, rejeitando path traversals e symlinks.
  - Filtragem por metadados com semântica AND em `--tag` e `--propriedade`, insensível a maiúsculas e diacríticos.
  - Extração de tarefas com suporte a hierarquia de indentação, checkboxes e comentários `completed_at` em ISO 8601, respeitando cercas de código e front matter.
  - Sanitização de terminal em `output::sanitize_for_terminal` neutralizando sequências ANSI, BEL e caracteres de controle em saídas para terminal humano.

- **Fase 4.0C.1 Fortalecimento de Contrato da CLI:** Versão centralizada no workspace Cargo (`version.workspace = true`) e tradução tipada de erros do Clap em português.

- **Fase 4.0C Fundação da CLI Headless:** Introduzida a crate `noteit-cli` fornecendo o binário independente `noteit`, sem dependências de servidor gráfico (X11/Wayland), GTK, WebKitGTK ou GApplication.

- **Fase 4.0B Fundação de Metadados — Tags + Propriedades:** As notas agora suportam `tags` e `properties` estruturadas no front matter YAML fora do corpo do Markdown, com validação semântica, isolamento em `noteit-core` e UI dedicada para edição.

- **Fase 4.0A Limite do Core:** Módulos de domínio Rust e persistência movidos para a crate interna headless `noteit-core`, estabelecendo fronteiras limpas de arquitetura.

### Alterado
- **Fase 3.15 Study Hub, Heatmap & Sequências:** Centralização de flashcards e sessões de estudo espaçado com agendamento Ladder-v1, histórico diário em `study.json` e visualização de calor no WebView.
- **Fase 3.14 Sistema de Estudo e Repetição Espaçada:** Flashcards em todas as notas ativas consolidados em um deck de revisão unificado com algoritmo de repetição espaçada.

### Corrigido
- **Fase 3.12R — Snapshots agora incluem imagens:** Snapshots de backup agora copiam o diretório `assets/` com as mesmas garantias de integridade das notas, com validação estrita de integridade.
- **Fase 3.8R — Correções na busca:** Busca estendida a todas as notas sem limite de 5.000 arquivos, com ordenação precisa por `updated_at` e isolamento total de gravações.
- **Fase 3.7R — Isolamento robusto de testes:** Criação do harness `scripts/note-it-isolated` com barramento D-Bus privado e variáveis XDG isoladas, prevenindo qualquer comunicação com o daemon real do usuário durante testes.

### Adicionado
- **Fase 3.12 Suporte a Imagens Locais e Layout Rico:** Inserção, redimensionamento e persistência de imagens em `assets/<note-uuid>/` através do esquema customizado `noteit-asset://`.
- **Fase 3.10 Modo Timer e Contagem Regressiva:** Máquina de estados desacoplada do DOM para contagem regressiva e alarmes visuais.
- **Fase 3.9 Lixeira e Backups Locais Automáticos:** Exclusão recuperável com movimentação para `trash/` e snapshots diários atômicos com retenção de 7 versões.
- **Fase 3.8 Busca Global e Substituição no Documento:** Paleta de busca (`Ctrl+K`) sobre todas as notas com correspondência semântica e realce de texto.
- **Fase 3.6 Motor Matemático e Conversão de Unidades:** Avaliação de expressões matemáticas (`= 2 + 2`) e conversão de unidades de medida inline sem interpretador JavaScript.
- **Fase 3.5 Blocos Inteligentes:** Suporte a blocos de código com destaque de sintaxe, callouts GitHub/Obsidian, citações e comentários Markdown.
- **Fase 3.3 Recolhimento Global de Notas:** Atalho de compositor para recolher/expandir todas as notas simultaneamente (`Mod+Shift+M`).
- **Fase 3.2 Tarefas, Controles de Visualização e Formatação Inline:** Checkboxes interativos com registro de data/hora, zoom de visualização e cores de texto/destaque.
- **Fase 3.1 Chrome da Nota, Menu de Configurações e Temas:** Barra de cabeçalho com ações de cor de papel, padrões de fundo e integração com tema claro/escuro do sistema.
