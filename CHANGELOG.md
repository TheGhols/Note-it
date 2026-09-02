# Registro de alterações

Todas as alterações notáveis ​​neste projeto serão documentadas neste arquivo.

O formato é baseado em [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), e este projeto segue [Versionamento Semântico](https://semver.org/spec/v2.0.0.html).

## [Não lançado]

### Adicionado
- **Fase 4.0G — Experiência humana e apresentação da CLI.** `noteit` sem argumentos deixou de listar comandos e passou a se apresentar, sem que nada disso alcance scripts, canos, agentes ou o contrato `--json`:
  - **A apresentação.** Logotipo `NOTE-IT` em blocos, a versão vinda de `CARGO_PKG_VERSION` — a mesma que `noteit versao` e `noteit status` leem, verificada por teste no binário real —, uma linha dizendo o que o Note-it é e cinco comandos por onde começar. Continua sendo um processo que imprime e sai: código `0`, saída de erro vazia, nenhuma espera por entrada. Não é uma TUI, não é um prompt e não é um REPL.
  - **Identidade.** Amarelo para a marca, magenta para o acento, e nenhuma terceira cor. Cores ANSI básicas, sem exigir true color. Nenhuma informação depende de cor: um teste remove todos os escapes da tela estilizada e compara com a tela pura.
  - **Largura.** Logotipo em blocos a partir de 54 colunas; entre 27 e 53, `NOTE-IT` escrito com os mesmos cinco comandos; abaixo de 27, `NOTE-IT`, versão e os dois comandos essenciais. A largura vem do próprio terminal via `TIOCGWINSZ`, com `COLUMNS` apenas como reserva e recusado quando implausível (zero colunas, cinco dígitos); sem terminal nenhum, 80 colunas por suposição conservadora — larga o bastante para a tela inteira. Nenhuma largura entre 1 e 200 produz uma linha maior que a janela.
  - **Consentimento.** `NO_COLOR`, mesmo definido como string vazia, e `TERM=dumb` desligam a cor. `TERM=dumb` também dispensa a arte em blocos, porque um terminal que se declarou sem recursos não é lugar para seis linhas de caracteres de desenho; um cano, que é um arquivo, continua recebendo o logotipo. Cano e redirecionamento recebem texto puro, determinístico e idêntico a cada execução.
  - **Sem efeitos colaterais.** Executar `noteit` não cria nota, janela, socket, lock ou store, não depende de haver notas e não falha se o store ainda não existe. Provado por fingerprint de cada caminho sob XDG isolado — modo, dono, inode, tamanho, bytes, `mtime` e `ctime` — antes e depois das cinco variantes da tela.
  - **O logotipo aparece uma vez.** `noteit ajuda`, `noteit --help`, a ajuda de cada subcomando, os erros e o `--json` seguem sem ele.
  - **Ajuda revisada.** Passou a documentar `--help`/`-h` e `--version`/`-V`, que são opções reais e estavam omitidas, além dos aliases em inglês de `--estado`; ganhou uma seção de exemplos e uma explicação mais completa de `--json`. Um teste confere a ajuda contra o que o parser realmente aceita.
  - **Interface de máquina intocada.** `--json` continua com exatamente um documento por execução, nos mesmos canais e com os mesmos códigos — agora provado também sobre um terminal real, sob janelas de todos os tamanhos e com `NO_COLOR`, `TERM=dumb` e `COLUMNS` definidos. Os 36 testes da interface de máquina seguem passando sem alteração.
  - **A TUI completa não foi antecipada** e foi registrada como Fase 5.0. Nenhum framework de TUI, loop de eventos, pager ou runtime assíncrono foi adicionado. A única dependência nova é `libc`, para a chamada de largura, já presente no grafo do workspace por `dirs-sys` e `getrandom` — uma aresta, não um crate — e sem alcance sobre o `noteit-core`. Consulte ADR-042.

### Corrigido
- **ANSI vazava para uma saída de erro redirecionada.** `noteit comando-inexistente 2> erros.txt`, rodado de um terminal, gravava sequências de escape dentro do arquivo: havia um único contexto de saída, derivado da saída padrão, e ele estilizava também a saída de erro. Cada canal passou a ser decidido a partir dele mesmo. Regressão coberta nos dois sentidos — terminal na saída padrão com erro em arquivo, e o contrário —, no renderizador e no binário real.
- **Estado não sincronizado do terminal da fase 4.0E.2R.** Tornou `unsynchronised` genuinamente terminal em vez de meramente documentado como tal, e selou a máquina de estado de gravação externa:
  - O cronômetro de aviso lento foi deixado armado após uma adoção fracassada, e seu único guarda perguntou se a solicitação ainda estava ativa – o que esse caminho mantém deliberadamente. Quatro segundos depois, a página substituiu "esta janela não pôde acompanhar, reabra a nota" por "Sincronização demorando…", descrevendo uma gravação em andamento quando não havia nenhuma e apontando para longe da única recuperação disponível. O cronômetro agora é cancelado por meio de um auxiliar que faz apenas isso; buscar `release` para cancelar um cronômetro foi o que causou o bug 4.0E.2, já que ele também descongela e drena.
  - Cancelar não é a garantia. A barreira agora contém um `SyncState` explícito (`idle`, `syncing`, `slow`, `unsynchronised`) e cada transição solicita a fase primeiro, portanto, um callback já enfileirado quando a fase é alterada encontra um estado no qual pode não atuar. `unsynchronised` não tem borda de saída: sem cronômetro, aplicação repetida, aborto, mensagem para outra solicitação, ou `setGeneration` pode retornar a página para `idle`, `syncing` ou `slow`, descongelar o editor, drenar a fila, mover a geração ou emitir uma confirmação positiva. O mesmo guarda fecha o caso simétrico que ninguém havia relatado – um aviso obsoleto chegando após uma gravação *bem-sucedida*, o que faria com que uma gravação finalizada parecesse lenta.
  - `NoteEditor.setMarkdown` suspende o bloqueio da transação ao adotá-lo e restaurá-lo após a chamada, em vez de em um `finally`; uma adoção que foi interrompida, portanto, deixou o bloqueio desativado, exatamente quando todos os comandos que a página pode executar devem ser recusados. Agora restaurado em um `finally`, com um teste que leva `setContent` a lançar.
  - Coberto por uma tabela completa de transição de estado executada como teste, testes de temporizador falso por meio da fiação `window.setTimeout` real que a página usa e callbacks capturados antes do cancelamento e acionados manualmente. Verificado fisicamente no ambiente isolado: após uma falha de adoção forçada, o arquivo contém `ABCD\nXYZ`, a janela permanece fechada, um acréscimo adicional é recusado, esperar bem além do limite lento não altera nada e reiniciar reabre a nota no conteúdo confirmado com `XYZ` exatamente uma vez.

- **Fase 4.0E.2 Adoção de UI com falha deve permanecer à prova de falhas.** Fechada a última lacuna pós-commit deixada por 4.0E.1:
  - `ExternalWriteBarrier.apply` chamou `release()` em todos os caminhos, incluindo aquele onde `adopt()` lançou. `release` descongela o editor e drena as ações do documento na fila, de modo que uma página que não conseguiu assumir um documento já confirmado voltou imediatamente a aceitar entradas - em uma geração pela qual o host já havia passado. Cada salvamento automático enviado foi recusado corretamente, o que significava que o leitor poderia digitar indefinidamente e perder tudo sem nada na tela para indicar isso.
  - Uma adoção fracassada agora mantém o documento retido: sem descongelamento, sem drenagem de fila, sem mudança de geração, sem `ExternalWriteApplied`. Apenas `ExternalWriteApplyFailed` sai e a nota se reporta fora de sincronismo ("A alteração foi gravada, mas esta janela não conseguiu acompanhá-la. Reabra a nota.") através de um novo estado de sincronização `unsynchronised`. Nada mais tarde – uma aplicação repetida, uma anulação ou uma mensagem para outra solicitação – pode fazer com que a página volte a editar o texto obsoleto.
  - A semântica do host permanece inalterada e agora é declarada no tipo: `committed_outcome` retorna um `WriteOutcome` em vez de um `Result`, portanto, após o ponto de commit, não há mais falhas para relatar. Uma confirmação recusada, expirada ou não entregue é uma gravação confirmada contendo `ui_sync_warning`, nunca um `WriteError` e nunca convida a uma nova tentativa.
  - Uma nota retida recusa outras gravações externas em vez de capturar um texto que ninguém pode garantir: a barreira nunca responde, o host atinge o tempo limite *antes* de confirmar e `noteit` é informado de que o armazenamento está ocupado e nada foi alterado.
  - Verificado fisicamente no ambiente isolado (privado XDG, privado D-Bus, real WebKitGTK, real Niri): com a adoção forçada a falhar, o arquivo contém `ABCD\nXYZ`, o CLI sai de 0 com o aviso e sem duplicação, a janela não aceita digitação e não emite nenhum obsoleto `ContentChanged`, um segundo acréscimo é recusado e a reinicialização reabre exatamente a nota sobre o conteúdo confirmado.

- **Fase 4.0E.1 Autoridade de escrita com fail-closed e confirmação de adoção pela UI.** Foram eliminadas três lacunas entre o que a Fase 4.0E prometeu e o que ela impôs:
  - A inicialização do desktop agora falha de forma segura (fail-closed). `AppContext` contém `WriteAuthority` por valor em vez de `Option`, a única maneira de obter um é um `write_authority::claim` completo (coordenação preparada, lease adquirido, limite de soquete e estreitado), e a declaração é executada antes de existir qualquer janela, documento ou salvamento automático. Uma instância que não pode possuir o armazenamento imprime uma frase e sai diferente de zero, em vez de executar como um segundo gravador. Um soquete que não pode ser aberto é igualmente fatal e libera o contrato na saída. Deliberadamente, não existe modo somente leitura.
  - A adoção de UI agora está confirmada pela página. `evaluate_javascript` retornando `Ok` apenas prova que o script foi executado — a página detecta seus próprios erros de ouvinte — portanto, não pode mais substituir a adoção. `ApplyExternalDocument` carrega o ID da nota e a página responde `ExternalWriteApplied { id, requestId, generation }` após ter adotado o documento, gerado a geração e retomado a edição, ou `ExternalWriteApplyFailed { id, requestId }` se não conseguiu. O host aceita uma confirmação somente quando a nota, a solicitação e a geração correspondem e aguarda 4s antes de fazer o downgrade para `ui_sync_warning`. A falha na entrega ainda é usada, mas apenas para falhar rapidamente.
  - A página não libera mais o documento em prazo próprio. `EXTERNAL_WRITE_CLIENT_TIMEOUT_MS` liberou o editor 15s depois que o snapshot foi lançado, enquanto o host ainda poderia estar escrevendo, sincronizando ou renomeando – reintroduzindo a corrida que a barreira existe para remover. É substituído por `EXTERNAL_WRITE_SLOW_NOTICE_MS`, que altera apenas o que é dito ao leitor ("Sincronização demorando…"). Somente `ApplyExternalDocument` ou `AbortExternalWrite` descongela o documento agora.
  - Uma página que não pôde ser adotada mantém a geração substituída, portanto, o texto obsoleto que ela mostra nunca poderá ser substituído pela alteração que acabou de ser confirmada; o editor ainda está liberado, porque o arquivo já está correto e uma nota congelada seria inutilizável e não poderia ser fechada.
  - A semântica pós-commit permanece inalterada e agora é válida em mais casos: uma confirmação ausente, recusada ou não entregue é uma gravação confirmada com um aviso, nunca uma falha e nunca convida a uma nova tentativa.
  - Novos testes processo a processo (`tests/fail_closed.rs`) executam o binário real em um lease genuinamente mantida, um diretório de coordenação inutilizável e um soquete que não pode ser aberto, afirmando que ele recusa, não escreve nenhuma nota, não grava nenhum estado de janela, libera o lease e inicia normalmente assim que o armazenamento estiver livre.

### Adicionado
- **Fase 4.0F Interface estável de máquina / JSON.** `noteit --json` é o primeiro contrato público para scripts e agentes e é um contrato e não uma conveniência:
  - Um documento por execução: um sucesso grava exatamente um documento JSON na saída padrão e **nada** no erro padrão, uma falha grava exatamente um no erro padrão e nada na saída padrão, cada documento termina em uma única nova linha e nenhum dos canais carrega ANSI — inclusive quando o processo é anexado a um terminal. Não há NDJSON, nem segundo documento, nem prosa antes ou depois.
  - Renderizado a partir do resultado tipado, nunca das frases. `noteit-cli` ganhou `outcome.rs` (`Outcome`, `CommandError`, os nomes canônicos `Command`, `CliResponse`) e `machine.rs` (o esquema público como DTOs explícitos). `output.rs` agora é explicitamente o renderizador *humano* sobre o mesmo valor. Não há `json_append`: `WriteOperation`, `NoteMutation`, `WriteOutcome`, `WriteError` e `authority::perform` permanecem intactos e um teste compara o arquivo de notas resultante, byte por byte, entre os dois modos.
  - `run_with_args` retorna um `CliResponse` carregando o código de saída e ambos os canais como dados. O despachante antigo imprimia avisos de leitura com `eprint!` no meio de um comando; um `--json listar` bem-sucedido em um store com uma nota ilegível teria escrito uma frase em português com erro padrão. Os avisos agora são dados no envelope, e "o sucesso não grava nada no stderr" é afirmado em vez de assumido.
  - `--json` é global — aceito antes do comando, depois dele e dentro de um comando agrupado, tanto com a grafia do português quanto com os aliases internacionais. É uma opção e nunca uma palavra: `noteit adicionar ID -- --json` anexa o texto literal e permanece no modo humano. O modo é decidido a partir da opção analisada quando a análise é bem-sucedida e a partir de uma varredura exata de todo o token dos argumentos brutos quando isso não acontece, então `noteit --json batata` responde com um erro de uso de JSON em vez de um parágrafo em português. Um teste afirma que as duas regras concordam.
  - Envelope versionado: `schema_version` (1), `status` (`ok`/`warning`/`error`/`indeterminate`), `command` canônico independente da ortografia (`listar` e `list` são ambos `list`), `data`, `error`, `warnings`. Todas as seis chaves sempre presentes, pedido de chave não faz parte do contrato, novos campos opcionais compatíveis.
  - Dados reais, tipos reais: UUIDs completos em todos os lugares (nunca o prefixo de oito caracteres para o qual o terminal abrevia), RFC 3339 UTC carimbos de data e hora ou `null` (nunca uma data localizada e nunca "desconhecida"), booleanos como booleanos, conta como números, resultados vazios como `[]` com `"count": 0`. Observe que o conteúdo é exatamente o Markdown do Core - `sanitize_for_terminal` não é deliberadamente aplicado aos dados e aspas, barras invertidas, novas linhas, tabulações, emoji e sequências de escape passam por qualquer analisador JSON.
  - `commit_state` em cada gravação: `committed`, `not_needed`, `not_committed` ou `unknown`. É a única fonte de verdade sobre se repetir uma operação é seguro; um sinalizador `retry_safe` foi considerado e rejeitado como segunda resposta à mesma pergunta.
  - `ui_sync_warning` é de primeira classe. Uma gravação confirmada cuja janela não pôde ser trazida para a etapa relata `status: warning`, `commit_state: committed`, um `ui_sync: {status: "warning", code: "window_not_confirmed"}` estruturado e uma saída `0` - nunca um erro, nunca `not_committed`, nunca uma saída diferente de zero. Verificado fisicamente no ambiente isolado com adoção forçada a falhar: o arquivo contém `ABCD\nXYZ` com `XYZ` exatamente uma vez, o erro padrão está vazio e uma segunda gravação ainda é recusada como `writer_busy`/`not_committed`.
  - `WriteError::Indeterminate` é de primeira classe. `status: indeterminate`, `error.code: indeterminate`, `commit_state: unknown` — explicitamente não `not_committed`, portanto, um agente não pode ler um soquete descartado como uma falha limpa e tentar novamente um acréscimo em uma duplicata. Comprovado tanto para uma conexão que desliga após a solicitação quanto para uma resposta que contém o identificador de outra solicitação.
  - Códigos de erro estáveis ​​para cada variante `WriteError` mais o menor conjunto de caminhos de leitura necessários, cada um documentado com seu código de saída e estado de confirmação. Erros de uso são digitados e carregam o comando quando ele é conhecido.
  - O protocolo de controle privado permanece privado: identificadores de solicitação, versão do protocolo, caminho do soquete, bloqueio de gravador, geração de janela e `WritePath` nunca são serializados em um documento público, e um teste verifica cada documento para esse vocabulário. As redações diretas e de autoridade produzem o mesmo contrato público.
  - Contrato documentado em `docs/machine-interface.md`, incluindo tabela de novas tentativas, com fundamentação em ADR-041. O humano CLI permanece inalterado — mesmas frases, cores, ordem, datas locais, avisos, aliases e códigos de saída — exceto por uma linha na ajuda que documenta a opção.
  - 32 novos testes em nível de processo executando o binário real e analisando canais inteiros, além de testes de esquema e contrato; nada no conjunto afirma uma substring de uma mensagem.

- **Fase 4.0E API de gravação + simultaneidade GUI/CLI.** O CLI agora pode alterar notas, e exatamente um processo Note-it grava um store por vez:
  - Lease de escrita: um `flock` consultivo sobre um arquivo de bloqueio em um diretório de coordenação de tempo de execução por store (`$XDG_RUNTIME_DIR/note-it/<store key>/`), compartilhado por ambos os adaptadores por meio de `noteit_core::coordination`. Um arquivo de bloqueio deixado por um processo travado não é um lease retido; um processo que morre o libera imediatamente. Os diretórios são criados `0700`, o soquete `0600` e os caminhos de tempo de execução com links simbólicos ou de propriedade estrangeira são recusados ​​em vez de reparados. Guiado por um resumo determinístico do diretório de notas, de modo que um store de teste isolado e o store real nunca concorram.
  - Autoridade de gravação: a instância da área de trabalho recebe o lease antes de poder salvar qualquer coisa e o mantém até o processo terminar, escutando em um soquete Unix local privado. `noteit` assume o lease pela duração de um comando quando é gratuito; quando é retido envia a alteração ao titular; quando está retido e inacessível, ele falha quando fechado, sem mudar nada e dizendo isso. Nunca mais volta a escrever em torno de outro gravador.
  - Barreira de gravação externa: alterar uma nota aberta na tela congela seu editor *antes* de lê-la (`ExternalWriteBarrier` mais uma porta ProseMirror `filterTransaction` que recusa todas as transações de alteração de documento, não apenas a entrada do usuário), dobra o texto não salvo do editor no mesmo commit via `write::apply_over_live_body`, confirma por meio do gravador atômico canônico e, em seguida, devolve a nota confirmada de volta à página. O texto digitado, mas ainda não salvo, nunca é substituído.
  - Geração em tempo de execução: cada `NoteWindow` carrega uma geração enviada em `LoadNote` e citada por toda mensagem que carrega conteúdo (`ContentChanged`, `SaveAndClose`, `MetadataChanged`, `FlushResponse`, `ExternalWriteReady`). Uma gravação externa confirmada a incrementa, portanto, um salvamento automático já em andamento da execução anterior é recusado em vez de desfazer a confirmação.
  - Operações Core digitadas: `WriteOperation`, `NoteMutation`, `NoteDraft`, `WriteOutcome`, `WriteOutcomeKind` e `WriteError` em `noteit_core::write`. Tanto o caminho direto CLI quanto a autoridade GUI executam a mesma implementação; não existe um segundo conjunto de regras.
  - Comandos: `criar`/`create`, `adicionar`/`append`, `editar`/`edit`, `tags adicionar|remover` (`add|remove`), `propriedades definir|remover` (`set|remove`), `tarefas concluir|reabrir` (`complete|reopen`), `lixeira restaurar` (`restore`), com `--stdin` para entrada multilinha e `--vazio` para a intenção explícita de esvaziar uma nota. Todos os comandos de leitura e ortografia existentes são preservados.
  - Referências de tarefas otimistas: `noteit tarefas` mostra um `TaskRef` de oito caracteres derivado deterministicamente (FNV-1a 64 em `noteit_core::hashing`) da nota, o aninhamento da tarefa, seu estado, seu texto exato e sua ocorrência entre tarefas idênticas. É recalculado no momento da gravação e recusado quando obsoleto ou ambíguo. Sem sidecar, sem banco de dados, sem identidade de tarefa persistente e sem analisador de segunda tarefa – a leitura e a gravação compartilham um scanner, portanto, uma tarefa falsa dentro de uma cerca de código é invisível para ambos.
  - Resultados honestos: uma falha no pré-commit não muda nada e pode ser repetida com segurança; uma gravação confirmada cuja janela não pôde ser atualizada relata um aviso em vez de uma falha, portanto ninguém anexa o mesmo parágrafo duas vezes; uma queda de conexão após a solicitação ser encerrada é relatada como um resultado desconhecido, em vez de uma nova tentativa cega.
  - Invariantes de carimbo de data/hora: anexar, editar e alternar uma tarefa move `updated_at` somente quando o corpo realmente mudou. Tags e propriedades não movem nenhum carimbo de data/hora. `created_at` nunca se move. Uma mutação autônoma não reescreve o arquivo.
  - Protocolo de controle privado: prefixado de comprimento JSON sobre um soquete de domínio Unix local, `protocol_version = 1`, limitado a 1 MiB por quadro, com identificadores de solicitação usados ​​para correlação e para reconhecer uma solicitação repetida em vez de aplicá-la duas vezes. As solicitações carregam seletores de notas, nunca caminhos do sistema de arquivos. Explicitamente **não** uma interface pública e não a superfície da máquina da Fase 4.0F.
  - Isolamento e limites: `noteit-core` e `noteit-cli` permanecem livres de GTK, GDK, WebKitGTK, camada-shell, Wayland e Niri; os comandos de gravação funcionam sem display, compositor ou barramento de sessão. As gravações de notas nunca tocam em `config.toml`, `state.json` ou no cache, e `noteit criar` não abre nenhuma janela, esteja Note-it em execução ou não. `scripts/note-it-isolated` e `scripts/test-isolation` agora removem o diretório de coordenação de tempo de execução pertencente a seus armazenamentos descartáveis.

- **Fase 4.0D.2 Pureza do pipeline de leitura e integridade dos avisos.** Consistência refinada do aviso do pipeline de pesquisa, saída direta erradicada em caminhos de leitura Core, consulta de domínio separada da limpeza de apresentação e correspondência rigorosa de regex de comentários de tarefa:
  - Pipeline de aviso de pesquisa unificado: `NoteItCore::search_notes_filtered` agora usa o pipeline `load_note` + `ReadWarning` idêntico para pesquisas não filtradas e filtradas, verificando o universo completo de notas elegíveis antes de aplicar limites de resultados.
  - Zero impressões diretas em caminhos de leitura de Core: removido o legado `eprintln!` de `StorageManager::read_bodies`, garantindo operações de leitura de headless 100% puras em Core.
  - Separação de consulta de domínio: A consulta de pesquisa do usuário original é passada inalterada para `noteit-core` para correspondência de pesquisa, enquanto a limpeza de terminal (`output::sanitize_for_terminal`) é aplicada estritamente às strings exibidas no adaptador de terminal.
  - Validação Regex de comentário de tarefa estrita: `task::extract_completed_at` impõe exatamente um token candidato em `<!-- note-it:completed_at=... -->`. Comentários com lixo que não seja espaço em branco são rejeitados e preservados sem modificação no texto da nota, correspondendo a `/<!--\s*note-it:completed_at=([^\s]+?)\s*-->/`.

- **Fase 4.0D.1 Contrato da API de leitura e proteção de terminal.** Contratos de apresentação refinada, segurança de terminal e desacoplamento Core:
  - Formatação de fuso horário local: a apresentação humana de data e hora em `noteit-cli` (`listar`, `ler`, `tarefas`, `lixeira`) é padronizada em `output::format_datetime_local` para exibir carimbos de data e hora no fuso horário local da máquina (`dd/MM/yyyy HH:mm`) correspondente ao contrato de desktop GUI. `noteit-core` permanece digitado estritamente com `DateTime<Utc>`.
  - Limpeza abrangente de entrada: a limpeza via `output::sanitize_for_terminal` é aplicada em todas as strings não confiáveis ​​renderizadas, incluindo consultas de pesquisa em títulos, seletores de notas em mensagens de erro, contextos de argumentos Clap em erros de uso e caminhos XDG refletidos em `status`.
  - Avisos Core digitados e zero impressões: todas as chamadas `println!` / `eprintln!` removidas dos caminhos de leitura `noteit-core`. Os métodos de leitura retornam `ReadBatch<T>` juntamente com estruturas `ReadWarning` digitadas, que o adaptador CLI formata corretamente para stderr em português.
  - Análise fiel de comentários de tarefa: `extract_completed_at` procura por `<!-- note-it:completed_at=... -->` em qualquer lugar nas linhas de tarefa, removendo apenas o comentário de metadados Note-it e preservando comentários externos de autoria do usuário HTML.

- **Fase 4.0D API de leitura headless.** Implementada a leitura inicial programática e humana API em `noteit-cli`, apoiada por autoridades centralizadas `noteit-core`:
  - Abertura de armazenamento somente leitura: `NoteItCore::open_read_only()` e `StorageManager::open_read_only()` inspecionam e abrem o armazenamento sem chamar `ensure_directories()`, criar diretórios ou arquivos ausentes ou acionar backups. Um armazenamento ausente retorna coleções vazias e limpas com código de saída 0.
  - Comandos e Aliases: comandos primários em português (`listar`, `ler`, `buscar`, `tags`, `propriedades`, `tarefas`, `lixeira`) com aliases internacionais padrão (`list`, `read`, `search`, `properties`, `tasks`, `trash`).
  - Nota Resumo e rótulos canônicos: a projeção `NoteSummary` em `noteit-core` reutiliza rótulo canônico (`search::label_for`) e ​​lógica de snippet sem criar autoridades de análise paralela.
  - Resolução segura de ID/prefixo: `NoteItCore::resolve_note_id` resolve seletores (UUID completo ou prefixo hexadecimal exclusivo >= 8 caracteres) em relação a identificadores de notas ao vivo. Travessias de caminho (`..`, `/`), caracteres não hexadecimais, prefixos ambíguos e arquivos de notas de link simbólico são rejeitados.
  - Filtragem de metadados: `NoteFilter` digitado suporta opções `--tag` e `--propriedade` (`--property`) únicas e repetidas com semântica AND, reutilizando `semantic_identity` para insensibilidade a maiúsculas e minúsculas. `--limite` (`--limit`) limita a saída (1 a 100).
  - Projeção de tarefa e analisador Markdown: `noteit_core::task` extrai tarefas com aninhamento de profundidade, estados de caixa de seleção (`- [ ]`, `- [x]`, `- [X]`) e carimbos de data/hora `<!-- note-it:completed_at=... -->` válidos sem inventar carimbos de data/hora para datas desconhecidas/ausentes. Blocos de código protegidos (``` e ~~~) e front matter são estritamente protegidos. As tarefas podem ser filtradas por `--estado` / `--state` (`pendentes`, `concluidas`, `todas` / `pending`, `completed`, `all`).
  - Segurança e higienização de terminal: `output::sanitize_for_terminal` neutraliza sequências de escape ANSI (CSI, OSC, injeção de área de transferência), BEL, backspace e caracteres de controle perigosos de conteúdo de nota não confiável antes da apresentação.
  - Estritamente somente leitura: todas as operações de leitura API são estritamente somente leitura e deixam o armazenamento em disco, byte por byte, inalterado.

- **Fase 4.0C.1 Fortalecimento do contrato da CLI.** Autoridade de versão refinada e apresentação de erros:
  - Versão centralizada do projeto em `[workspace.package]` com herança de espaço de trabalho Cargo (`version.workspace = true`) em `note-it`, `noteit-core` e `noteit-cli`.
  - Adicionada tradução de erro Clap digitado em `output::render_error`, enviando mensagens claras em português para stderr para comandos desconhecidos, opções e argumentos inesperados sem substituir Clap como autoridade do analisador.

- **Fase 4.0C Fundação da CLI headless.** Introduziu o crate dedicado `noteit-cli`, que fornece o binário headless independente `noteit`. O aplicativo gráfico de desktop (`note-it`) continua sendo a GUI e o adaptador de ciclo de vida, enquanto ambos os adaptadores consomem a autoridade compartilhada `noteit-core`.
  - Arquitetura headless: `noteit` não requer servidor de exibição X11/Wayland, GTK, WebKitGTK ou
Registro `GApplication`. `scripts/check-cli-boundary` impõe zero dependências de UI/desktop.
  - Interface bilíngue: apresentação humana em português (`ajuda`, `versao`, `status`), com
aliases internacionais padrão (`help`, `version`, `status`, `--help`, `-h`, `--version`, `-V`).
  - Fonte de versão única: as strings de versão derivam estritamente de `CARGO_PKG_VERSION`.
  - Status estritamente somente leitura: `noteit status` inspeciona diretórios XDG resolvidos e existência de armazenamento
sem ler arquivos de notas, analisar Markdown ou gravar em disco.
  - Resolução de caminho pura: `StorePaths::resolve()` em `noteit-core` executa resolução de caminho XDG pura
sem alterar o sistema de arquivos ou criar diretórios no disco.
  - Apresentação limpa: a detecção automática de TTY/NO_COLOR garante que apenas códigos de cores ANSI sejam emitidos
quando stdout é um terminal interativo e NO_COLOR não está definido.
  - Códigos de saída padrão: 0 para sucesso, 2 para uso/argumentos inválidos, 1 para erros de execução.

- **Fase 4.0B Fundação de metadados — tags + propriedades.** As notas agora podem conter `tags` estruturadas de autoria do usuário e `properties` textuais ao lado do bloco reservado de front matter `note_it`. As notas legadas são lidas como metadados vazios e nunca são migradas ou reescritas apenas por serem abertas.
  - `noteit-core` possui validação, identidade que não diferencia maiúsculas de minúsculas/acentos, limites, ordem determinística,
Persistência YAML e catálogos de notas ao vivo derivados. Nenhum índice, banco de dados ou arquivo secundário foi adicionado.
  - Valores YAML de nível superior desconhecidos sobrevivem à análise/serialização semântica. Tags/propriedades vazias são
omitido, enquanto comentários/âncoras e formatação podem ser normalizados somente quando ocorre um salvamento real.
  - As gravações apenas semânticas usam o gravador de notas atômicas canônicas e não movem `created_at` ou
`updated_at`. O WebView envia seu Markdown ativo com um rascunho de metadados confirmado, evitando um
a edição de texto pendente seja substituída pelo corpo do host/disco mais antigo.
  - O menu existente ganha uma entrada **Metadados**. As tags aparecem como uma faixa responsiva de uma linha de
pílulas acessíveis determinísticas; Tags e propriedades são editadas em um teclado acessível,
painel de rolagem interna com preenchimento automático derivado de catálogo.
  - A recência agora lê o delimitador de front matter de fechamento real com 256 KiB documentados
teto, portanto, metadados válidos além do teste anterior de 4.096 bytes ainda usam `updated_at`.

- **Fase 4.0A Fronteira do Core.** O domínio Rust e os módulos de persistência agora residem no crate interno headless `noteit-core`. `NoteItCore` expõe os caminhos canônicos existentes de listagem, leitura, pesquisa, listagem da lixeira e consulta de estudo, e o aplicativo GTK/WebKit consome esse crate em vez de manter implementações paralelas.
  - Core tem seu próprio pequeno manifesto Cargo sem GTK, GDK, WebKitGTK, layer-shell, Wayland, Niri ou
dependência do compositor. `scripts/check-core-boundary` impõe essa regra de dependência e CI executa
os testes Core com `DISPLAY` e `WAYLAND_DISPLAY` removidos.
  - Domínio existente, armazenamento, backup, lixo, ativos, estudo, configurações, estado operacional, cronômetro e
Os testes de política AutoPaste foram movidos com suas implementações; novos testes de fachada usam apenas temporários
stores sintéticos.
  - O ciclo de vida CLI (`--background`, `new`, `toggle`, `show`, `hide`, `quit`) e o TypeScript
editor permanecem preocupações com o adaptador de desktop e mantêm seu comportamento.

- **Fase 3.14R.1 Polimento de interface e acessibilidade visual.** O cabeçalho existente agora está agrupado como Nota, Texto, Conteúdo e Visualização/Ferramentas, com separadores silenciosos e uma pílula de pesquisa centralizada que abre a SearchPalette estabelecida. Ele compacta ou cede ao ícone substituto antes de colidir com o Menu, um Timer/AutoPaste ativo, Lixeira ou Fechar; identificadores e manipuladores de botão permanecem inalterados.
  - A linguagem do Study Hub agora distingue **Cartões** de origem de **Avaliações** direcionais. Um básico
além de uma fonte reversível, portanto, lê 2 cartões e 3 análises, enquanto o progresso da sessão permanece
revisar o progresso.
  - Um vocabulário de movimento de 100/150/180 ms dá aos botões e painéis internos uma resposta contida;
recolher/expandir anima apenas o conteúdo WebView enquanto GTK permanece como autoridade geométrica. Reduzido
o movimento remove a animação, a transição e o dimensionamento da imprensa sem atrasar qualquer ação.
  - O zoom por nota agora abrange 75–300% no caminho existente de 10% e persiste os novos valores em
`state.json`. Uma **escala de interface** global separada abrange 90–160% e é armazenada em `config.toml`,
transmitido para cada WebView e altera as métricas reais do Chrome e a altura recolhida sem
dimensionar ou reescrever o conteúdo da nota.
  - Os rótulos de cabeçalho e de atalho de menu vêm de uma tabela de metadados. As dicas de ferramentas nomeiam a ação e adicionam
apenas atalhos realmente manipulados pelo WebView; `aria-keyshortcuts` carrega o mesmo mapeamento.

- **Sistema de estudo da Fase 3.14 e repetição espaçada.** O baralho agora contém todos os flashcards em todas as notas ao vivo, incluindo notas fechadas, com notas excluídas da lixeira e notas restauradas retornando com sua programação anterior. Um editor Tiptap sob demanda analisa o catálogo de documentos do host por meio do extrator ProseMirror existente; Rust nunca aprende a sintaxe `::`.
  - Cada direção de revisão recebe uma identidade SHA-256 derivada da nota UUID, frente/verso semântico,
direção e ordinal duplicado. A formatação, a largura/alinhamento da imagem e a posição do documento são
apresentação e não zerar o progresso; texto semântico, ativo gerenciado ou mudanças de direção sim.
  - A versão 1 de `study.json` reside em `$XDG_DATA_HOME/note-it/`, separada de Markdown e
`state.json`. Ele contém apenas chaves de revisão opacas, programações Ladder-v1 e contadores diários, é
confirmado atomicamente e falha de modo seguro (fail-closed) sem substituir dados corrompidos ou mais recentes.
  - Difícil, Médio e Fácil usam a escada fixa de 10 minutos a 240 dias. O host Rust possui
o relógio e o dia civil local; o painel avança e atualiza a atividade somente após o atômico
a gravação é reconhecida e uma gravação com falha deixa o cartão e o estado persistente inalterados.
  - A Central de estudos interna oferece **Revisar agora**, **Todos** e **Esta nota**, uma lista global compacta, sete
contagens úteis, um mapa de calor de 365 dias acessível em escala fixa, sequências atuais/mais longas e o mesmo
renderizador FlashcardPanel seguro com rótulos de notas de origem, visualizações de intervalo e um resumo mínimo.
  - O cabeçalho adiciona um deck de um clique, Zoom −/+ e um atalho para lixeira recuperável imediatamente ao lado
Fechar. Zoom reutiliza `zoom_changed`; A lixeira só pode abrir a confirmação existente. Medido
pontos de interrupção ocultam atalhos opcionais antes que possam substituir Menu, Timer/AutoPaste ativo ou X.
  - A versão 3 do manifesto de backup adiciona `study.json` opcional. As versões 1 e 2 permanecem legíveis; um
arquivo de estudo existente que não pode ser copiado falha no instantâneo antes de seu ponto de commit.

- **Fase 3.13 Flashcards Core.** Os cartões são projeções da própria nota: escreva `Pergunta :: Resposta` para uma direção ou `Termo ::: Definição` para ambas, alinhado com espaços ou como um marcador de nível superior entre dois blocos estruturais.
  - A extração percorre o documento ProseMirror em vez de corresponder a Markdown. Código, URLs, horários,
namespaces, atributos de imagem, dois pontos longos e linhas ambíguas permanecem como conteúdo comum, enquanto
marcas ricas, títulos, listas, tarefas, citações, textos explicativos e imagens gerenciadas permanecem intactas.
  - O editor mantém `::` e `:::` visíveis sob uma decoração silenciosa e relata ambos os cartões de origem
e contagens de itens de revisão ao vivo. Detecção e decoração não enviam nenhuma transação e não escrevem
identidade oculta, metadados, banco de dados ou arquivo secundário.
  - *☰ › Estudo* abre um painel somente leitura no WebView atual com progresso, revelação, anterior,
próximo, embaralhamento testável determinístico, navegação pelo teclado, nomes acessíveis, restauração de foco
e um pergaminho interno para cartões longos. Uma nota sem cartões diz isso e não abre nada.
  - Cada sessão captura os itens de revisão quando é aberta. A edição e o AutoPaste continuam sem
reorganizando-o; a reabertura tira o novo instantâneo. Timer/Pomodoro continua enquanto seu popover é
fechado, e o colapso da nota encerra a sessão.
  - As imagens reutilizam a Fase 3.12 `noteItImage`, referência armazenada e rota `note-it-asset:`. Estudar
serializa fragmentos seguros de documentos, não copia nenhum ativo e não expõe controles de edição.
  - Abrir, revelar, navegar, embaralhar e fechar sair Markdown, `updated_at`, desfazer histórico e
estado persistente do aplicativo intocado. O agendamento e a repetição espaçada permanecem fora do 3.13.

### Corrigido
- **Fase 3.12R — um snapshot agora também contém as imagens.** A Fase 3.12 colocou as imagens de uma nota em `assets/<note-uuid>/<asset-uuid>.<ext>` e o backup ainda copiou apenas `notes/`, `trash/`, `config.toml` e `state.json`. Um instantâneo obtido no meio restaura o Markdown de uma nota e não o arquivo para o qual seu `![](../assets/…)` aponta - meia nota, de algo cuja promessa é que mantém tudo recuperável.
  - `assets/` agora faz parte de todos os instantâneos, tanto automáticos quanto manuais, na mesma forma que tem em
o armazenamento e byte por byte. Sem recompactação, sem conversão, sem renomeação: um backup copia bytes.
  - Copiado estritamente e fechado com falha. Dois níveis conhecidos e nunca uma descida recursiva geral; não
o link simbólico é seguido em qualquer um deles; e qualquer coisa que não seja `<note-uuid>/<asset-uuid>.<ext>`
interrompe o instantâneo em vez de ser silenciosamente deixado de fora de um relatado como completo. `assets/` é
escrito por Note-it e por nada mais, então uma estranheza significa que o store não está no estado em que
acredita-se que seja. O risco deixado por uma importação interrompida é ignorado, assim como acontece com as notas.
  - Cada nome é validado pelo mesmo analisador usado pelo esquema `note-it-asset:`, portanto, um instantâneo contém
exatamente os arquivos que o aplicativo pode servir e os dois não podem discordar.
  - Uma imagem para a qual nenhuma nota aponta mais também é copiada. A Fase 3.12 optou por não recolher órfãos, e
um backup não é o lugar para começar a fazer isso por omissão.
  - Uma falha na cópia de uma imagem falha em todo o snapshot antes do ponto de commit: nada é renomeado
no lugar, o diretório temporário é removido e a retenção não é executada — um backup antigo nunca é
excluído para dar lugar a algo que não aconteceu.
  - `manifest.json` é a versão 2 e registra quantas imagens o instantâneo contém. Instantâneos da versão 1
permaneçam listáveis ​​e legíveis e sejam lidas como as imagens zero que eles realmente continham.
  - Um armazenamento escrito antes da existência das imagens não tem `assets/` e faz backup inalterado.
  - `docs/storage.md` agora inclui `assets/` no procedimento de restauração manual.

### Adicionado
- **Fase 3.12R.1 — um clipe no cabeçalho.** Colocar uma imagem em uma nota é a coisa mais comum que alguém faz na seção Mídia, e foi preciso abrir o menu e entrar primeiro em um submenu. Um clipe de papel agora fica na barra entre o **Buscar** e o cronômetro e abre o seletor de arquivos no primeiro clique.
  - O mesmo seletor, a mesma importação, o mesmo `assets/<note-uuid>/<asset-uuid>.<ext>` e o mesmo
referência relativa em Markdown. Ambos os gatilhos executam uma função e enviam a existente
Mensagem `insert_image_requested`: uma segunda porta para a sala, nunca uma segunda sala.
  - *☰ › Mídia › Inserir imagem…* permanece intacto e continua funcionando, assim como colar e soltar.
  - Oculto enquanto a nota está recolhida, como as seis ações rápidas, e oculto em uma nota expandida
mais estreito que 300 px — o orçamento da barra em `MIN_NOTE_WIDTH` tem que ceder para algum lugar, e o
o clipe de papel é o único controle cujo trabalho o menu ainda executa por completo.
  - Seu desenho é SVG embutido escrito na página em tempo de construção a partir da coleção de ícones, como
todos os outros ícones da barra. Nada é buscado, então nada sai em branco sob a página
próprio `default-src 'self'`.
  - Nenhuma nova mensagem IPC, nenhum novo seletor, nenhum novo caminho de importação, nenhum novo atalho de teclado e nenhuma alteração
para `assets`, `backup`, `storage`, `search`, `timer` ou `autopaste`.

- **Fase 3.12 Imagens e layout rico.** Uma imagem em uma nota, mantida como um arquivo em vez de contrabandeada para o texto. Cole um, coloque um na nota ou escolha um *☰ › Mídia › Inserir imagem…*.
  - PNG, JPEG, WebP e GIF, decididos pelos primeiros bytes e nunca por um nome de arquivo – então um PNG
chamado `.txt` é um PNG e algo chamado `.png` que não é uma imagem é recusado. **SVG é
não aceito**: é um formato de documento que pode conter script. Uma recusa diz isso em uma linha em
o pé da nota e não deixa nada para trás.
  - **Nunca base64 no Markdown.** Os bytes vão para
`~/.local/share/note-it/assets/<note-id>/<asset-id>.<ext>`, ao lado de `notes/` e `trash/`, e
a nota armazena um caminho relativo a `notes/`. Caso contrário, uma captura de tela transformaria uma nota que você pode
leia em um megabyte que você não pode, e faça o mesmo com cada backup e cada comparação.
  - Essa forma relativa é a razão pela qual uma nota chega à lixeira e retorna byte por byte: `notes/` e
`trash/` são irmãos, então `../assets/…` resolve o mesmo e nada é reescrito.
Nenhum caminho absoluto da máquina do leitor é escrito em uma nota.
  - **A página nunca informa um caminho do sistema de arquivos.** Ela carrega `note-it-asset:/<note>/<asset>.<ext>`,
que o host atende após analisar ambas as metades como `Uuid`s - um `..`, um caminho absoluto ou um
o separador codificado não resolve um arquivo, ele não analisa. A página
A Política de Segurança de Conteúdo foi ampliada por esse esquema e nada mais. Consulte ADR-032.
  - Simples `![](…)` enquanto não há nada a dizer além de onde está a imagem, e um canônico
`<img src alt data-note-it-width data-note-it-align>` depois que uma largura ou alinhamento for escolhido —
sempre esses atributos, sempre nessa ordem, apenas os definidos. Qualquer outra coisa nessa tag é
descartado: um `onerror`, um `style`, um `srcset` ou uma fonte que não é um dos ativos deste store.
  - Redimensione arrastando qualquer uma das alças, mantendo as proporções porque apenas a largura é armazenada.
Uma imagem pode ser tão larga quanto a nota e não mais larga. Todo o arrasto é uma entrada no
histórico, então `Ctrl+Z` retorna a largura a partir da qual você começou.
  - Esquerda, centro e direita, com o texto percorrendo o outro lado da imagem alinhado à esquerda ou
certo - em torno dele, nunca embaixo dele. Citações, comentários e blocos de código ficam ao lado de um carro alegórico, em vez de
do que abaixo dele.
  - Cada alteração em uma imagem é uma edição comum: o Markdown muda, o `updated_at` se move e o
o salvamento automático existente o grava. Selecionando um, abrindo seus controles, cancelando o seletor de arquivos ou
escolher o alinhamento que ele já mudou não muda nada.
  - **Uma imagem não é texto.** Nada sobre como uma imagem é armazenada chega ao título recolhido, uma pesquisa
snippet, o rótulo da lixeira ou `visibleText`: pesquisando um identificador, uma largura, um alinhamento ou
`assets` não encontra nada, e uma nota contendo uma imagem e nenhuma palavra ainda é *Nota sem título*.
  - Nada é buscado. Não há como inserir uma imagem por URL, e alguém digitou uma imagem remota
é desenhado sem nenhuma fonte, então abrir uma nota chega à rede de graça.
  - A remoção de uma imagem a remove da nota e **sai do arquivo**. Não há automático
coleção de ativos órfãos, deliberadamente: decidir que um arquivo não é utilizado é uma suposição, e agir de acordo
essa suposição destrói alguma coisa.
  - Nenhuma dependência foi adicionada.

### Alterado
- **Roteiro reordenado.** 3.12 é Imagens e Layout Rico; Flashcards Core permanece em 3.13; Captura e Exportação – exportação de texto, PDF e avaliação de OCR offline – volta para 3.14.
- **Fase 3.11 AutoPaste da área de transferência.** Copie algo em qualquer lugar da máquina e ele será colocado no final de uma nota que você escolheu. Nenhuma janela aparece, nenhuma tecla é pressionada para você e nada ocupa o seu cursor. Distinto de *Colar URL na Seleção*, que foi enviado na Fase 3.8 e que está intacto.
  - **Desativado por padrão, e desativado significa que não há ouvinte.** Enquanto o AutoPaste está desativado, não há área de transferência
manipulador conectado, então nada é observado, lido, hash, armazenado, registrado ou enviado. Medido
em uma sessão Niri real: três cópias com o modo desativado produziram zero eventos na área de transferência de qualquer
tipo. Consulte ADR-031.
  - **O modo nunca é anotado.** Nem no Markdown, nem no `state.json`, nem no
`config.toml`. Uma reinicialização, um logout, uma falha ou uma atualização deixa tudo desativado e o leitor decide
novamente - não há nenhum campo no protocolo que possa ativá-lo novamente.
  - Ativado em *☰ › Captura*, com uma linha dizendo exatamente o que fará. Enquanto estiver no
nota mantém sua barra de fora com um 📋 ao lado dos outros controles, em uma nota recolhida também, e pressionando
que abre o painel que o desliga.
  - **Um alvo para todo o aplicativo**, porque a área de transferência do sistema é uma coisa. Armando um
segunda nota libera a primeira na mesma etapa, e a barra e o menu da nota liberada param
reivindicando isso.
  - Orientado por evento através do próprio sinal `changed` de GDK - sem pesquisa, sem intervalo, sem
`navigator.clipboard` e nenhuma nova dependência.
  - Somente texto: uma imagem, uma lista de arquivos ou um formato desconhecido é recusado dos formatos oferecidos
sem que um byte dele seja transferido. Uma cópia vazia ou em branco não arquiva absolutamente nada.
  - **Tudo o que estava na área de transferência antes do switch nunca ser capturado.** Conectando o manipulador
não lê nada, então apenas uma alteração após esse momento é uma captura.
  - As capturas são anexadas ao **final** da nota como uma transação: sem foco, sem
seleção movida, nenhuma rolagem, nenhuma janela levantada, nenhuma camada alterada. Uma captura é uma `Ctrl+Z`.
  - O texto entra como texto, com o mesmo significado que `Ctrl+V` tem aqui: `**isso é literal**` permanece
asteriscos, `<script>alert(1)</script>` permanece com onze caracteres, um URL continua sendo um URL e nada é
buscado. Acentos, emoji, 日本語 e cópias multilinhas permanecem inalterados.
  - Três delimitadores — **Linha**, **Linha em branco** (padrão) e **Separador** — aplicados exatamente
uma vez entre cada par e nunca antes da primeira captura em uma nota vazia. Mudando o
a preferência se aplica à próxima captura e não reescreve nada já escrito.
  - **Proteção de loop do kit de ferramentas, não de comparação.** Uma cópia ou corte dentro de Note-it faz
o aplicativo, o proprietário da área de transferência e GDK diz isso, e essa alteração é recusada antes de qualquer
a leitura começa. A desduplicação de conteúdo foi rejeitada deliberadamente: copiar `ABC` duas vezes, em duas ações,
arquiva duas vezes.
  - Uma geração em cada execução armada, revalidada quando cada leitura assíncrona retorna, portanto, uma leitura
ainda no ar quando o modo é desligado, o alvo muda, a nota fecha ou o
o aplicativo oculta não oferece nada. As leituras são serializadas, então A, B, C chegam como A, B, C.
  - Desligado **antes** de liberar, fechar, ocultar, sair e descartar, para que nenhum callback obsoleto possa chegar
um documento que está prestes a ser escrito e destruído. Recolher, alterar camada e mover
para outro aplicativo, deixe-o ativado.
  - Uma captura é uma edição real - as alterações de Markdown, movimentos de `updated_at`, o salvamento automático existente
escreve e a pesquisa encontra o texto. Ativar ou desativar o modo e alterar o delimitador
não mude nada disso e não coloque nenhum marcador próprio na nota.
  - Note-it nunca se apropria da área de transferência: após uma captura, o que você copiou ainda será colado
normalmente em qualquer outro aplicativo.
- **Fase 3.10 Timer & Pomodoro.** Uma contagem regressiva na nota em que você está trabalhando, acessada a partir de um ⏱ na barra de cabeçalho e mostrada em um pequeno painel abaixo dela. Nenhuma segunda janela e nenhuma faixa permanentemente retirada da nota.
  - **Timer** com predefinições de 5, 10, 15, 25, 30, 45 e 60 minutos e um campo para qualquer outra coisa
de 1 a 600 minutos inteiros. Zero, um negativo, uma fração, `NaN` ou algo além do teto
é recusado e dito isso; nada é arredondado para o intervalo, porque um cronômetro que funcionou silenciosamente por um
a duração que ninguém escolheu é pior do que aquela que se recusou a começar.
  - **Pomodoro 25/5/15**: quatro sessões de foco em um ciclo, a quarta seguida de um longo intervalo,
então a contagem começa novamente. A fase é um modelo explícito, em vez de um comportamento espalhado por
manipuladores de eventos, e o painel mostra qual fase, qual sessão das quatro e o ciclo.
  - Iniciar, pausar, continuar, cancelar, redefinir e pular, com apenas os controles aplicáveis ​​em exibição —
não há pausa em um cronômetro pausado, não há continuação em um que nunca foi iniciado.
  - **Nada começa sozinho.** Uma fase que termina é marcada como concluída e *oferece* a próxima
no botão; o leitor começa. Uma pausa que começasse no meio da frase seria uma
Pomodoro ninguém concordou.
  - **A verdade é um instante, não um contador.** Uma corrida é armazenada como o momento do relógio de parede em que
termina e cada leitura é `deadline - now`, então nada é perdido e nada é perdido para um estrangulamento
WebView, uma máquina ocupada ou um laptop suspenso. Pausar descarta o instante e congela o
restante, então o tempo pausado não pode ser gasto - através de uma ocultação, através de uma reinicialização ou através de qualquer
número de ciclos de pausa/retomada. Consulte ADR-030.
  - A execução sobrevive à nota ser recolhida, ocultada ou ao aplicativo ser fechado e reaberto: ela
volta com o tempo que realmente passou já descolado, e cujo fim já passou
volta **terminado** em vez de contar até zero. Não toca para uma corrida que terminou
enquanto não havia nada lá para ouvi-lo; o estado finalizado está na barra.
  - Uma nota recolhida mantém o relógio na barra ao lado do nome da nota, de modo que uma contagem regressiva contínua nunca
precisa que a nota seja expandida para ser confiável. Uma nota muito estreita para ambos abre mão dos dígitos e mantém
o ícone; o nome e o controle próximo nunca cederam.
  - A conclusão acontece **exatamente uma vez**, protegida pela própria transição de estado e não por um
sinalizador: uma linha no final da nota e uma notificação na área de trabalho, independentemente do tempo que a nota permanecer
em zero. A notificação não traz nada da nota – a página informa que tipo de execução
terminou, de um conjunto fechado de quatro, e o host é o dono das palavras.
  - **Um cronômetro não faz parte da nota.** Ele nunca é escrito no Markdown de qualquer forma.
Iniciando, pausando, finalizando e cancelando deixe o arquivo de notas byte por byte como estava e
deixe `updated_at` onde estava, para que uma nota com cronômetro não salte para o topo do rápido
comutador; pesquisa, o título recolhido e a lixeira nunca o veem. Pesquisar `25:00` não
encontre uma nota apenas porque ela tem um Pomodoro de 25 minutos em execução. O estado vive ao lado do
geometria da janela em `state.json`, escrita apenas em uma mudança semântica e nunca em um tick, então um
executar a contagem regressiva não custa nenhum tráfego de disco e nenhum IPC.
  - Uma contagem regressiva por nota, codificada pelo identificador da nota: duas notas não podem misturar seus temporizadores, e
não há gerenciador de cronômetro global.
- **Fase 3.9 Ergonomia de cabeçalho e UX.** O cabeçalho existente agora recua nas notas expandidas e retorna ao passar o mouse/foco, enquanto uma nota recolhida o mantém visível com um título apenas de apresentação derivado da primeira linha útil Markdown. A cor e o tamanho do texto embutido passaram de `☰` para exatamente duas ações rápidas que abrem seus painéis e pipelines existentes. Um menu mais alto que a nota é limitado a WebView e rola verticalmente, incluindo todos os submenus; notas maiores mantêm o menu natural. Os dois ícones enviados são os SVGs `palette-round` e `larger-text` revisados ​​de `IconesNote-it/`; o restante da coleção fornecida permanece local e ignorado.
- **Lixeira recuperável.** A exclusão de uma nota agora existe e pode ser desfeita.
  - *☰ › Dados › Mover esta nota para a lixeira* pergunta primeiro, e a pergunta diz que a exclusão é
recuperável em vez de apenas "Excluir?". Cancelar é o foco do painel. O botão `×` e
`Ctrl+W` ainda significa **fechar a janela**, exatamente como sempre fizeram.
  - A ordem é flush → mover → estado → superfície, e a movimentação do arquivo é o ponto de commit.
Uma nota cujo último texto não pôde ser escrito **não** é movida: ela permanece aberta, a falha é
relatado, e o leitor pode tentar novamente. Após o movimento, a nota está na lixeira, então nem o
a gravação do estado da janela nem a desmontagem da superfície podem informar o contrário.
  - `notes/<uuid>.md` torna-se `trash/<uuid>.md`, byte por byte — front matter, cor, papel, tarefas,
links, cálculos e comentários viajam com ele. Nada lê, analisa ou reescreve a nota,
portanto, uma nota cujo front matter está danificado também é excluída e recuperada inalterada.
  - Uma nota na lixeira não é uma nota: `Ctrl+K` não a encontra, a lista de consulta vazia não oferece
isso, uma invocação não o traz de volta e uma reinicialização não o reabre - porque todos aqueles leram
`notes/` e o arquivo não está mais lá.
  - *Dados › Lixeira* lista o que pode ser recuperado, primeiro o mais novo, com a primeira linha de cada nota, uma
visualização e quando foi excluído. As setas percorrem a lista, `Enter` restaura, `Esc` fecha; cada linha
também possui um botão nomeado **Restaurar**.
  - A restauração retorna o mesmo arquivo com o mesmo identificador e **nunca substitui uma nota ativa**:
o nome é criado com `hard_link`, que recusa atomicamente um nome existente, então deixa um conflito
ambos os arquivos intocados e diz isso.
  - Nem excluir nem restaurar é uma edição. `updated_at` não se move, então uma nota recuperada
retorna ao seu lugar no switcher rápido em vez de pular para o topo; sua geometria volta
também.
  - A data de exclusão é um arquivo secundário `<uuid>.json` ao lado da nota, nunca escrita em Markdown. UM
faltando ou ilegível, essa entrada custa sua data exata e nada mais.
- **Backup automático local.** Instantâneos de tudo que pode ser recuperado, na mesma máquina e em nenhum outro lugar.
  - `~/.local/share/note-it/backups/<data-e-hora>/` segurando `notes/`, `trash/`, `config.toml`,
`state.json` e um `manifest.json`. Diretórios comuns de arquivos comuns: legíveis com `ls`,
recuperável com `cp`, sem formato de arquivo e sem banco de dados no caminho.
  - No máximo um snapshot automático a cada 24 horas, tirado **antes** da primeira alteração qualificada após
essa janela e não depois dela - o estado ao qual vale a pena retornar é aquele antes da
editar. Não há cronômetro nem thread: um daemon inativo não funciona e outro fica aberto para
dias tira seu instantâneo no momento em que seu proprietário começa a digitar novamente. "Quando foi o último backup" é
leia o próprio manifesto do instantâneo mais recente, para que não haja nenhum arquivo de contabilidade que fique obsoleto.
  - *Dados › Fazer backup agora* pega um imediatamente e relata sucesso ou fracasso em uma fila em
o rodapé da nota em vez de um diálogo sobre ela.
  - Um instantâneo é criado em `backups/.tmp.…` e renomeado: a renomeação é o ponto de commit,
portanto, um backup escrito pela metade nunca pode ser listado como válido. O arranhão deixado por um acidente é varrido por
o próximo backup, e apenas os diretórios que carregam esse prefixo são removidos - nunca um instantâneo,
nunca um arquivo que alguém colocou lá.
  - Sete snapshots são mantidos e a retenção é executada **somente após** um novo ter sido confirmado, portanto, um
backup que falha nunca custa a proteção já existente no disco.
  - Um instantâneo nunca contém instantâneos anteriores, arquivos temporários ou qualquer coisa alcançada por meio de um
link simbólico — apenas arquivos regulares dos diretórios que foram solicitados a copiar.
  - Um backup que falha nunca bloqueia um salvamento. O erro é relatado e a nota é escrita normalmente.
  - A recuperação é comprovada e não prometida: `a_snapshot_round_trips_into_a_fresh_isolated_store`
copia um instantâneo em uma segunda árvore XDG vazia e a abre. O procedimento manual, incluindo
recuperando uma única nota, está em `docs/storage.md`.
  - **Um backup local não é uma recuperação de desastres.** Esses instantâneos ficam no mesmo disco que as notas
e não são criptografados. Eles protegem contra uma exclusão acidental, uma corrupção lógica, uma edição
para desfazer ou uma versão para a qual voltar - e contra nada de uma unidade morta, uma máquina perdida ou roubada
um.

### Alterado
- O que a Fase 3.8 foi enviada como "AutoPaste" agora é chamado de **Colar URL na seleção** (`ui/src/editor/linkPaste.ts`, `handleLinkPaste`, `ui/tests/link_paste.test.ts`). O comportamento é o mesmo, byte por byte; apenas o nome mudou, então "Clipboard AutoPaste" é gratuito para o modo de captura da área de transferência planejado para a Fase 3.11, que é um recurso totalmente diferente.

### Corrigido
- A pesquisa agora faz o que diz que faz. Quatro correções, nenhum novo comportamento:
  - **Cada nota é pesquisada.** A varredura parou em 5.000 notas, então um armazenamento com uma nota maior continha
observe que nunca foi encontrado e nada teria relatado que foi ignorado. A varredura agora lê
toda o store; a lista de **resultados** ainda está limitada a 100. A listagem de consulta vazia mantém seu
cap, pois mostra no máximo cem notas.
  - **A paleta descarta qualquer resposta a uma pergunta que não está mais sendo feita.** A numeração ficou lenta
resposta chegando depois de uma ordem rápida, mas não a outra ordem: a resposta para `bio` chegando enquanto
`biopsia` ainda estava em voo era mais antigo que a pergunta atual e mais recente que qualquer coisa
aceito, então foi mostrado. Somente a resposta da solicitação pendente poderá alterar a lista.
  - **"Mais recente" é o `updated_at` da própria nota, não a data do arquivo.** Alterando a data de uma nota
cor, papel, intensidade do padrão ou tamanho da fonte reescreve o arquivo sem ser uma edição, então
ordenar pela hora de modificação do arquivo fez com que a repintura de uma nota contasse como escrita nela - em
a troca rápida e em que nota uma invocação foi trazida de volta. Uma nota sem leitura
`updated_at` volta para a data do arquivo, exatamente como antes, e os empates são desfeitos por
identificador. A listagem ainda não escreve nada.
  - **Os limites documentados agora dizem o que eles vinculam.** 512 caracteres de consulta, 100 resultados e
Cerca de 240 caracteres do trecho são limites para a pergunta e a resposta; eles nunca limitaram
o tamanho de uma nota, e a pesquisa lê uma nota até o final porque uma palavra no final deve ser
encontrável. O custo de uma nota grande é medido — uma nota de 2 MB é pesquisada corretamente, os acentos
intacto, sem escrever nada - em vez de ser descrito como limitado.
- O equipamento de teste isolado agora isola o **barramento de sessão**, bem como os diretórios XDG. Note-it é uma instância única `GApplication`: com um daemon já rodando no barramento real, um comando "isolado" foi entregue a esse daemon por D-Bus e o store real fez a escrita, então a substituição de `XDG_*` não protegeu nada. `scripts/note-it-isolated` agora inicia um `dbus-daemon` privado para cada sessão de teste, aponta `DBUS_SESSION_BUS_ADDRESS` para ele e limpa as variáveis ​​iniciais D-Bus, de modo que o processo isolado se torna a instância primária e funciona em seu próprio armazenamento — com o daemon real deixado em execução e intocado.
  - Falha segura: o barramento é iniciado, comprovadamente distinto do real e acessível antes que
Note-it é iniciado e o ambiente do processo iniciado é lido de volta em `/proc`. Saída
os códigos 90–93 nomeiam a garantia que não pôde ser cumprida.
  - `--root DIR` mantém a sessão privada ativa durante as invocações, `--verify` afirma a instância
está nele e `--stop` termina - de forma síncrona e lendo a atividade do processo de `/proc` em vez
do que de `kill -0`, porque onde nada colhe órfãos, um daemon parado permanece como um zumbi
que `kill -0` ainda reporta como vivo.
  - `scripts/test-isolation` reproduz o incidente e é executado em `cargo test`; contra o velho
aproveitá-lo falha com a nota perdida no store ambiente, e contra a nova ele passa.
  - Nenhum código do aplicativo foi alterado: o defeito estava no harness.

### Adicionado
- Pesquise cada nota e as maneiras de chegar ao que encontra:
  - `Ctrl+K` abre uma paleta de pesquisa dentro da nota em que você já está — sem segunda janela, não
segunda aplicação. Não diferencia maiúsculas de minúsculas e não diferencia acentos, então `biopsia` encontra `Biópsia` e
`coracao` encontra `Coração`.
  - Uma consulta vazia lista as notas escritas mais recentemente, portanto, o mesmo controle também é uma consulta rápida.
comutador.
  - Uma nota é um resultado, com um rótulo derivado de sua primeira linha não vazia, um trecho em torno
a primeira partida e uma contagem quando houver várias. Os snippets são renderizados como texto, nunca como
marcação.
  - `Enter` abre o resultado escolhido: uma nota já aberta é ativada, uma fechada é aberta, uma
o recolhido é expandido e a correspondência é rolada e destacada. Nada disso toca
`updated_at`, e nada disso altera a camada Desktop/Overlay.
  - Os resultados são tratados por `note_id`. O WebView não pode nomear um caminho, portanto não pode solicitá-lo.
  - Limites explícitos: 512 caracteres de consulta, 100 resultados, aproximadamente 240 caracteres de snippet. Digitar é
debounce em 120 ms e cada solicitação é numerada, portanto, uma resposta para `bio` nunca pode substituir uma
resposta mais recente para `biopsia`.
  - A pesquisa não grava nada: sem liberação, sem salvamento, sem arquivo de índice, sem entrada `state.json`.
- Encontre e substitua dentro da nota atual:
  - `Ctrl+F` encontra, com uma contagem ao vivo, `Enter`/`Shift+Enter` para percorrer as ocorrências e empacotamento em
ambas as extremidades; `Esc` fecha e devolve o teclado ao editor. Abrindo com um breve
a seleção de linha única semeia o campo a partir dele.
  - `Ctrl+H` adiciona substituição: uma ocorrência ou todas elas. Uma alternância `Aa` faz a pesquisa
maiúsculas e minúsculas.
  - `Replace All` é uma única transação ProseMirror aplicada da última para a primeira, então vinte
as substituições voltam com um `Ctrl+Z`. Marcas, listas, títulos e blocos de código sobrevivem,
porque o documento é editado em vez de serializado novamente.
  - Ao contrário da pesquisa global, localizar e substituir é sensível ao acento**: substituir é destrutivo e
`saude` não deve substituir `saúde`. Um resultado escolhido na paleta carrega, portanto, o
ortografia que realmente corresponde, então `biopsia` ainda cai em `Biópsia`.
  - O realce é uma decoração: encontrar 7 ocorrências não cria nenhuma transação, nenhuma etapa de desfazer e nenhuma
escrever.
- Colar um URL sobre o texto selecionado transforma esse texto em um link — selecione `site oficial`, cole `https://example.com` e a nota contém `[site oficial](https://example.com)`.
  - Ele reutiliza `safeLinkUrl`, a lista de permissões do restante do aplicativo já usado, portanto, há
exatamente uma opinião sobre o que é uma URL. O próprio `linkOnPaste` de Tiptap está desligado porque
usa `linkifyjs` e esquemas aceitos que este aplicativo não usa.
  - Nada é buscado: nenhum título, nenhum favicon, nenhuma visualização, nenhuma rede.
  - O código embutido, os blocos de código e as seleções que abrangem dois blocos são deixados como uma pasta comum e
a coisa toda é uma etapa de desfazer.
- Conversões de unidades, escritas da mesma forma que o resto do mecanismo e mostradas da mesma maneira:
  - `= 10 km em m` mostra `10000 m` ao lado da linha. `em` é a palavra-chave de conversão e a única
um.
  - Oito dimensões, todas as grafias listadas em `docs/features.md`: **comprimento** (`mm`, `cm`,
    `m`, `km`, `in`, `ft`, `yd`, `mi`), **massa** (`mg`, `g`, `kg`, `t`, `oz`, `lb`), **volume**
    (`mL`, `cL`, `dL`, `L`, `cm³`, `m³`), **temperatura** (`°C`, `°F`, `K`), **tempo** (`ms`, `s`,
    `min`, `h`, `dia`, `semana`), **área** (`mm²`, `cm²`, `m²`, `km²`, `ha`), **dados digitais**
    (`B`, `KB`, `MB`, `GB`, `TB`, `KiB`, `MiB`, `GiB`, `TiB`) and **velocidade** (`m/s`, `km/h`,
`mph`), cada um com aliases ASCII e português.
  - O lado esquerdo é uma expressão matemática completa, então `= (10 + 5) km em m`,
`= distancia km em m` e `= x * 2 km em m` todos lidos. A unidade se aplica a toda a expressão.
  - A temperatura é convertida como escalas com zeros diferentes e não como um fator: `= 0 C em F` é
`32 °F` e `= 0 C em K` é `273,15 K`. A área é sua própria unidade, e não um comprimento com um
expoente, então `= 1 m2 em cm2` é `10000 cm²`.
  - Os prefixos SI e IEC permanecem separados: `= 1 GB em MB` é `1000 MB` e `= 1 GiB em MiB` é `1024 MiB`.
  - `= 10 banana em m` says *unidade desconhecida*, `= 10 kg em km` says *unidades incompatíveis*
e `= -300 C em K` diz *conversão inválida* — silenciosamente, fora da linha e nunca no arquivo.
  - Uma quantidade convertida encerra um bloco de agregação, porque `sum`, `avg` e `count` somam-se
números e não sabe nada sobre unidades.
  - As conversões são lidas exatamente onde estão os cálculos: apenas parágrafos simples.
- Cada conversão é local, offline e determinística, e os fatores são os definidos – uma polegada equivale exatamente a 0,0254 m, uma libra equivale exatamente a 453,59237 g. Não foi incluído nada cujo valor dependa da definição que o leitor tinha em mente, por isso não existe `cup` nem `alqueire`.
- As moedas foram deliberadamente **não** implementadas e nenhuma taxa foi codificada. O limite atrás do qual uma fonte de taxa futura deve permanecer está escrito em `ui/src/units/convert.ts` e ADR-025, e um teste afirma que nada no mecanismo pode alcançar a rede.
- Um motor matemático. Uma nota é calculada conforme está escrita, sem nada para pressionar e nenhum modo para entrar:
  - `= 2 + 2` mostra `4` ao lado da linha; `+`, `-`, `*`, `/` e parênteses, com o habitual
precedência. Os decimais podem ser escritos `10.5` ou `10,5`; um número com dois separadores é recusado
em vez de serem lidos como um agrupamento de milhares, e os resultados são impressos sem um para que possam
sempre ser lido de volta.
  - `preco := 120` declara um valor que as linhas abaixo podem usar. Os nomes são ASCII, as variáveis ​​são
local para a nota e resolvido de cima para baixo, então uma variável existe de sua declaração para baixo
e um ciclo não pode ser escrito.
  - Porcentagens nos formulários que as pessoas escrevem: `10% de 200` → `20`, `200 + 10%` → `220`,
`200 - 10%` → `180` e `taxa := 10%` seguido por `= taxa * 200` → `20`. O contextual
a leitura pertence a um `%` escrito na linha, nunca a um valor que já veio de uma.
  - `sum`, `avg` e `count` sobre o bloco de linhas de cálculo consecutivas diretamente acima deles.
Uma prosa, um título, uma declaração ou uma linha falhada encerra o bloco, então um número colocado em um
frase nunca é adicionada a nada.
  - Os resultados são **reativos**: a nota inteira é reavaliada a cada alteração, portanto, editar uma
A declaração move todos os resultados abaixo dela de uma vez, sem nenhum rastreamento de dependência para ficar obsoleto.
  - Um cálculo que não consegue responder diz isso em quatro palavras ao lado da linha — *divisão por zero*,
*variável desconhecida*, *expressão inválida*, *nome inválido* — sem diálogo, sem pop-up e
nada escrito no arquivo.
  - O cálculo é lido apenas em parágrafos simples. Dentro de um bloco de código, um intervalo de código embutido, um
comentário, um título, uma lista, uma tarefa, uma citação ou um texto explicativo, `= 2 + 2` é o texto que é.
- Os resultados são decorações ProseMirror e nunca conteúdo, portanto o `.md` armazenado contém exatamente o que foi digitado: nenhum resultado chega ao arquivo, `updated_at` não se move para um recálculo, abrir uma nota não é uma edição, desfazer e refazer operam apenas no texto e reabrir recomputa tudo.
- O analisador de expressão não tem nenhum avaliador por trás dele — nenhum `eval`, nenhum `Function`, nenhum acesso de propriedade, nenhuma sintaxe de chamada e nenhuma nova dependência. `= window.location` e `= constructor.constructor(...)` não podem ser soletrados na gramática, em vez de serem filtrados dela, e as variáveis ​​residem em um `Map`, portanto, nenhuma nota pode alcançar uma propriedade JavaScript herdada.
- Uma ligação global autoritativa Niri `Ctrl+Shift+Space` apoiada pela `toggle-layer` GAction do aplicativo em execução; o atalho WebView em foco permanece disponível como substituto local.
- Blocos inteligentes, todos os quatro acessíveis a partir de uma seção **Blocos** do menu existente da nota:
  - **Blocos de código** cuja linguagem sobrevive à viagem de ida e volta Markdown exatamente como foi escrita. Uma cerca
sem língua fica sem língua, uma língua desconhecida mantém sua grafia e simplesmente vai
não destacado e um alias permanece como alias. O realce de sintaxe cobre dezesseis gramáticas -
texto simples, bash, javascript, typescript, json, html/xml, css, markdown, python, rust, c, cpp,
java, sql, yaml, toml — e os aliases aos quais cada um já responde. É desenhado como editor
decorações, então a nota armazenada é uma cerca simples, sem marcação, e nunca é adivinhada
para um bloco cujo idioma está ausente ou não é reconhecido.
  - **Chamadas** na sintaxe de alerta de GitHub, que Obsidian também lê: `NOTE`, `TIP`, `IMPORTANT`,
`WARNING` e `CAUTION`. Um texto explicativo contém vários parágrafos, listas e blocos aninhados, e um tipo
esse não é um dos cinco, permanece como a citação que já é, com seu texto intacto.
  - **Comentários** armazenados como `<!-- ... -->`, mostrados como um pequeno bloco rotulado que pode ser lido e editado
e removido, e nunca faz parte do que diz a nota.
- Os blocos de código cercados agora fecham com uma cerca mais longa do que a sequência mais longa de crases dentro deles, portanto, uma nota contendo um exemplo Markdown é escrita inteira em vez de ser cortada no exemplo.
- Tipos de papel por nota: **Liso**, **Pautado**, **Pontilhado**, **Quadriculado pequeno** e **Quadriculado grande**, escolhidos no menu de configurações e aplicados de uma só vez. O papel comum tem a aparência original e não desenha nada.
- Intensidade do padrão por nota — **Suave**, **Normal**, **Forte** — que altera a opacidade do padrão e nada mais: nem a cor do papel, o texto, o conteúdo ou a geometria.
- A tinta do padrão acompanha a cor do papel, por isso fica visível em todos os sete papéis, inclusive no escuro, sem competir com o texto da nota. Seu espaçamento é fixo em pixels, então o zoom dimensiona o texto e deixa o fundo de lado.
- Tema da interface: **Sistema**, **Claro** e **Escuro**, escolhidos no menu de qualquer nota e compartilhados por todas as notas. **Sistema** segue o esquema de cores da área de trabalho enquanto o aplicativo é executado. O tema veste os menus, popovers, bordas e estados de foco do aplicativo; uma nota mantém a cor e o papel que foi fornecido, então uma nota amarela permanece amarela sob o tema escuro.
- `note-it toggle-collapse-all` recolhe todas as notas ainda expandidas e expande todas elas quando todas são recolhidas. `Ctrl+Shift+M` continua a ser aplicado apenas à nota em foco.
- Clicar em uma nota recolhida a expande de volta ao tamanho anterior, e o botão `☰` expande a nota e abre seu menu com um único clique.
- Digitar `->` em prosa torna-se um verdadeiro `➜`. A nota armazena o caractere em si, portanto não depende de uma fonte com ligaduras, e os trechos de código e blocos de código são deixados exatamente como digitados.
- Listas de tarefas Markdown: digitar `- [ ] ` ou `- [x] ` cria uma tarefa real com uma caixa de seleção quadrada, aninhada em qualquer profundidade, com tarefas concluídas marcadas automaticamente.
- Carimbos de data e hora de conclusão por tarefa, mostrados como `Concluído dd/MM/aaaa HH:mm` e armazenados junto com a tarefa em Markdown. A reabertura de uma tarefa limpa sua data; uma tarefa concluída fora de Note-it não mantém nenhuma.
- Zoom de visualização entre 75% e 300% (`Ctrl+=`, `Ctrl+-`, `Ctrl+0` ou menu), persistido por nota sem tocar no documento.
- Tamanho do texto embutido, cor e realce do texto, aplicado a uma seleção ou como marca armazenada, em paletas compactas no menu de configurações.
- `Ctrl+Shift+M` para recolher ou expandir uma nota e `Ctrl+Shift+Space` para alternar entre **Sempre no topo** e **Área de trabalho** — ambos reutilizando as ações existentes.
- `scripts/note-it-isolated`, que executa Note-it em uma árvore XDG descartável e se recusa a iniciar se algum diretório for resolvido no store real.
- O popover de configurações de nota foi aberto a partir de um botão `☰` no cabeçalho, contendo a paleta de cores do papel e a entrada recolher/expandir.
- Recolher e expandir: uma nota pode ser reduzida à sua barra de cabeçalho e restaurada ao seu tamanho anterior na posição onde a barra recolhida foi deixada. O estado recolhido é persistido.
- Datas de criação e modificação mostradas em pt-BR após posicionar o cursor na barra de cabeçalho.
- Base do projeto, documentação de arquitetura e estrutura de construção.
- GTK4 + `gtk4-layer-shell` + WebKitGTK Esqueleto do shell do aplicativo de desktop 6.0.
- Módulo de armazenamento local Markdown com YAML front matter e gravações em disco atômico.
- Estrutura do editor TypeScript + Vite + Tiptap WYSIWYG e interface de ponte IPC.
- Ciclo de vida de instância única e especificação de interface de linha de comando.

### Alterado
- A promoção Desktop-to-Overlay agora é confirmada imediatamente mesmo quando a nota está totalmente coberta, mantém o aplicativo normal em foco ativo e evita chamadas `present()` incondicionais. A persistência do estado da camada é combinada para alternâncias rápidas sem enfraquecer as gravações do estado atômico.
- As citações em bloco são apresentadas como citações em vez de itálico esmaecido: recuadas, pautadas na lateral e definidas na própria cor do texto da nota. Várias linhas de prosa citada costumavam ser mais difíceis de ler do que o parágrafo ao seu redor.
- Os comentários de HTML não são mais excluídos pela higienização. Um comentário é um dado inerte e agora é o conteúdo que a nota mantém, portanto, um comentário escrito à mão - ou por outro editor - sobrevive ao salvamento em vez de desaparecer no primeiro. Um `<!--` interminado escapa em vez de engolir tudo depois dele.
- Como uma nota inalterada não é mais reescrita, a nota que uma invocação traz de volta quando tudo está fechado é a última escrita, e não aquela cuja janela foi fechada por último.
- O menu de configurações ganhou **Tipo de papel**, **Intensidade** e **Tema**, cada um mostrando seu valor atual na linha raiz, ao lado das entradas que já tinham.
- Menus, popovers e estados de foco agora são revestidos pelo tema da interface por meio de um conjunto de tokens `--ui-*`, em vez de emprestar as cores do papel da nota. A cor do texto é visualizada em um fundo claro, e não na própria superfície do popover, porque a paleta é ajustada para ser lida no papel. Um popover colorido do papel não sobreviveria a um tema: sobre uma nota amarela, um popover escuro teria herdado o texto escuro daquele papel. Tudo o que está desenhado no papel – o texto da nota, suas caixas de seleção, seus destaques e os botões do cabeçalho – ainda segue o papel.
- As notas ganham `paper_type` e `paper_intensity` em seu front matter. Uma nota escrita antes deste lançamento não contém nenhum dos dois, abre como papel comum com intensidade normal e os ganha na próxima vez que for salva. Alterar salva a nota sem alterar seu conteúdo ou data de modificação.
- `config.toml` ganhou `theme`. Uma configuração escrita antes desta versão é carregada inalterada e segue o sistema.
- Executar `note-it` agora invoca: restaura as notas e as traz para a frente através da instância já em execução. Quando está na camada da área de trabalho, ele é elevado para ficar genuinamente visível, sem reescrever a preferência da camada armazenada.
- `Ctrl+=` e `Ctrl+-` agora controlam o zoom da visualização em vez do tamanho base da fonte da nota. O tamanho base ainda é lido no front matter da nota quando ela é carregada.
- A cor do papel agora é escolhida no menu de configurações, em vez de um ponto colorido que percorre a paleta ao clicar.
- `updated_at` agora rastreia apenas edições de conteúdo. Alterar a cor do papel, o tamanho da fonte, a geometria da janela ou o estado recolhido não marca mais a nota como modificada.

### Corrigido
- Os atalhos de teclado funcionam dentro de uma nota novamente e `Ctrl+Shift+Space` alterna entre **Área de trabalho** e **Sempre no topo** como deveria. Uma janela de shell de camada é mapeada sem nenhum widget de foco, então GDK recebeu cada pressionamento de tecla e a soltou antes do WebKit: nada chegou à página e todos os atalhos da nota estavam mortos até que um clique aconteceu para focar o WebView por acidente. A troca de camada mapeia novamente a superfície e limpa o foco novamente, e é por isso que o atalho funcionou uma vez e depois parou. A página agora se torna o widget de foco da janela sempre que a superfície mantém o foco do teclado, portanto, uma nota está pronta para o teclado assim que o compositor lhe dá foco e permanece assim durante uma mudança de camada. A entrada do menu e `note-it toggle` nunca foram afetadas — consulte "Voltando da camada da área de trabalho" em `docs/niri.md`.
- Abrir uma nota escrita por outro editor, ou qualquer nota que termine em uma lista, texto explicativo ou bloco de código, não conta mais como edição. Duas coisas colocam novas linhas no final de uma nota e nenhuma delas é conteúdo: a nova linha com a qual um arquivo termina e a linha em branco que o próprio serializador do editor coloca após um documento que termina em um bloco. Comparar essas grafias literalmente fez com que fosse simples abrir e fechar, reescrever o arquivo e mover `updated_at` uma vez. Uma nota agora é comparada e armazenada em uma grafia canônica, e os arquivos armazenados são finalizados da mesma forma que qualquer outra ferramenta os grava. Uma edição real ainda se move `updated_at` exatamente como antes.
- Uma nota criada logo após a invocação de Note-it não fica mais arquivada em todas as janelas. Uma invocação eleva as notas para a sobreposição enquanto mantém deliberadamente a preferência armazenada como estava, de modo que a preferência fosse "desktop" enquanto todas as superfícies estavam na sobreposição - e uma nova nota era aberta a partir da preferência, na camada inferior, invisível momentos depois que o usuário solicitou Note-it. Agora ele abre na camada em que seus irmãos estão realmente.
- `state.json` não é mais relatado como não salvo quando de fato foi escrito. Ele nunca obteve a regra do ponto de commit que as notas foram fornecidas na Fase 3.4R.2: uma falha na sincronização do diretório *após* a renomeação foi relatada como uma falha no salvamento, e cada chamador trata isso como "nada foi escrito" - fechar uma nota reverteu seu estado e deixou a janela aberta, e ocultar recusou-se a fechar as janelas - enquanto o arquivo já mantinha o novo estado. Notas, estado da janela e configuração agora compartilham uma gravação atômica com uma regra de ponto de commit.
- `config.toml` é totalmente substituído ou não é substituído. Ele foi escrito diretamente sobre o arquivo real, que o trunca primeiro, então uma gravação interrompida deixou uma configuração escrita pela metade – e o carregamento volta aos padrões sem uma palavra, redefinindo silenciosamente o tema e todas as outras preferências.
- Uma nota cujo salvamento *com sucesso* não é mais tratada como não salva. A renomeação que substitui o arquivo de notas é o ponto em que a alteração se torna real e a sincronização do diretório de notas acontece depois disso. Uma falha nessa sincronização estava sendo relatada como falha no salvamento, então o aplicativo manteve a nota antiga na memória enquanto o arquivo já continha a nova – a imagem espelhada da divergência acabou de ser corrigida. A falha na sincronização agora é relatada como realmente é: o salvamento aconteceu e pode não sobreviver a uma perda de energia, que o próximo salvamento de qualquer nota repara por conta própria.
- Uma nota cujo salvamento falhou não será mais tratada como salva. O documento mantido na memória foi atualizado antes da gravação ser confirmada, portanto, uma gravação com falha deixou a memória contendo o texto que o arquivo nunca recebeu - e a verificação de conteúdo idêntico adicionada pouco antes comparou a próxima tentativa com aquele estado fantasma e relatou sucesso sem gravação, o que poderia perder a edição silenciosamente ao fechar. As alterações de conteúdo e aparência agora são preparadas em uma cópia e adotadas apenas depois que o arquivo foi realmente gravado, portanto, uma falha no salvamento deixa a nota descrevendo exatamente o que está armazenado e a próxima tentativa grava de verdade. Uma falha ao salvar também não deixa mais seu arquivo temporário no diretório de notas.
- Abrir e fechar uma nota não conta mais como edição. O fechamento e as liberações antes de ocultar e sair enviam tudo o que o editor contém, editado ou não, e cada um deles movido `updated_at`. O caminho único pelo qual eles passam agora compara o texto recebido com o que já está armazenado: conteúdo idêntico não registra nada e não reescreve o arquivo, enquanto uma alteração real é registrada exatamente como antes. `created_at` nunca foi afetado.
- O texto destacado pode ser lido em uma nota escura. A extensão de destaque renderiza um `color: inherit` embutido, que supera a regra da folha de estilo destinada a escurecê-lo, de modo que o texto destacado continua herdando o branco do papel. A marca agora pinta seu próprio primeiro plano escuro em linha. Uma cor de texto explícita ainda é gravada na nota e reaparece quando o realce é removido.
- O menu de configurações não fica mais recortado em uma nota recolhida. A nota se expande primeiro, para que o menu seja aberto em uma superfície alta o suficiente para mantê-la.
- Três cores de texto foram escurecidas para que cada uma delas permaneça legível em todos os realces e em todas as cores de papel.
- Fechar a última nota não a torna mais inacessível. Executar Note-it novamente reabre a nota usada por último em vez de criar uma nota em branco; o conteúdo da nota fechada nunca foi perdido, mas não houve caminho de volta.
- Um redimensionamento rápido não expõe mais uma faixa escura antes da nota ser repintada: a janela é revestida com a cor do papel da própria nota, que é mantida em sintonia quando a cor muda.
- Digitar `- [ ] ` produz um item de tarefa em vez de um marcador contendo o literal `[ ]`.
- Os trechos embutidos aninhados não perdem mais a marca interna quando uma nota é recarregada.
- Os gestos de ponteiro emitem deltas de geometria apenas enquanto exatamente um ponteiro é capturado. Uma captura de ponteiro perdida ou um movimento informando que nenhum botão foi pressionado agora encerra o gesto, e um quadro de animação restante de um gesto concluído não pode mais mover a janela.
- Notas cujo front matter omite `created_at` / `updated_at` continuam abrindo; a data desconhecida é relatada como desconhecida em vez de ser substituída por uma data fabricada.
