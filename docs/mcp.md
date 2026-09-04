# `noteit-mcp` — o servidor MCP local do Note-it

`noteit-mcp` é um servidor **Model Context Protocol** que dá a um agente acesso
tipado às notas do Note-it. Ele é local, headless, iniciado pelo host, fala
apenas por entrada e saída padrão, e grava exclusivamente pelo mesmo caminho que
a CLI e a janela do desktop usam.

Ele existe porque um agente é um **escritor programático**, e um escritor
programático que sobrescreve aquilo que não leu é como o parágrafo de alguém
desaparece. Toda a superfície abaixo é organizada em torno de fechar essa porta.

---

## 1. O que é, e o que não é

| | |
| --- | --- |
| Transporte | stdio, e somente stdio |
| Ciclo de vida | o host inicia o processo e o encerra fechando a entrada padrão |
| Rede | nenhuma: nenhuma porta, nenhum listener, nenhum HTTP, nenhum SSE |
| Configuração | nenhuma: nenhum arquivo lido, nenhum arquivo escrito |
| Superfície MCP | **tools**, e nada além disso |
| Store | o que o ambiente XDG do processo resolver |

Não é um daemon. Não é um servidor de filesystem. Não é uma segunda
implementação do Note-it.

> **Fase 4.2 em diante.** A arquitetura do Segundo Cérebro — a camada de
> recuperação de contexto que uma IA externa usará através deste servidor — está
> em [`docs/second-brain.md`](second-brain.md). Nada dela está implementado; o
> catálogo abaixo continua sendo exatamente o da Fase 4.1.

**Não implementado nesta fase, deliberadamente:** MCP Resources, MCP Prompts,
sampling, elicitation, a extensão MCP Tasks, transporte HTTP/SSE/Streamable HTTP,
OAuth, servidor remoto, IA interna, Segundo Cérebro, embeddings, banco vetorial,
indexação semântica e RAG. Nada disso está presente, e nada disso é preparado
aqui.

### Duas fronteiras, dois protocolos

```text
MCP host  ◄── MCP oficial, negociado pelo SDK ──►  noteit-mcp
noteit-mcp  ◄── protocolo privado do Note-it ──►  a instância desktop que segura o store
```

A primeira é MCP. A segunda é um soquete Unix privado entre dois processos do
Note-it, e não é assunto de ninguém fora deste repositório. As duas nunca são
misturadas: a versão do protocolo privado não é a versão do MCP, e nada do
soquete, do lease, do diretório de runtime ou de qual dos dois caminhos de
gravação foi tomado aparece na superfície MCP.

### Arquitetura

```text
MCP host
   │  spawn
   ▼
noteit-mcp                       (rmcp, stdio)
   │  entradas Rust tipadas
   ▼
ExistingNoteMutation / NoteDraft
   ▼
noteit_core::authority::perform_at
   │
   ├─ lease livre   → grava aqui mesmo, pelo Core
   └─ lease ocupado → pede a quem o segura, pelo soquete privado
                      (inalcançável → fail closed, zero bytes)
   ▼
commit atômico
```

O servidor **nunca** abre um `.md`, **nunca** executa `noteit` e **nunca**
interpreta a saída JSON da CLI. `scripts/check-mcp-boundary` falha o build se
alguma dessas coisas aparecer no código.

---

## 2. Executando

```bash
cargo build --release --workspace
./target/release/noteit-mcp        # não faz nada sozinho: espera um host
```

Ele lê mensagens JSON-RPC, uma por linha, na entrada padrão, e responde na saída
padrão. Rodá-lo em um terminal e não digitar nada é o comportamento correto.

**A saída padrão pertence ao protocolo.** Nenhum banner, nenhuma versão, nenhum
progresso, nenhum aviso. Um único `println!` corromperia o fluxo JSON-RPC.
Diagnósticos, quando inevitáveis, vão para a saída de erro — e mesmo lá nunca
carregam o corpo de uma nota.

### Configuração de host — apenas como documentação

O trecho abaixo é o formato que a maioria dos hosts usa. **Ele é exibido aqui
para ser lido, não aplicado:** este repositório não escreve em `~/.claude`,
`~/.config/Claude`, `~/.cursor`, `~/.vscode` nem em qualquer configuração sua.
Se você quiser usá-lo, a decisão e a edição são suas.

```jsonc
{
  "mcpServers": {
    "note-it": {
      "command": "/caminho/para/target/release/noteit-mcp"
    }
  }
}
```

Não há argumentos, não há flags e não há variável para escolher o store: o
servidor resolve o store do ambiente XDG em que o host o iniciou, exatamente
como qualquer outro programa do Note-it.

### Modelo de execução: uma thread no protocolo, outras no disco

```text
host  ──stdio──►  reactor (current-thread)  ──spawn_blocking──►  Core / arquivos
```

O runtime é *current-thread*, e é suficiente porque essa thread não faz o
trabalho: ela lê a entrada padrão, roteia e escreve respostas. Toda chamada ao
Core — leitura e escrita — vai para o pool de blocking do Tokio. Por isso uma
busca que varre o store inteiro não impede o servidor de responder `ping` nem de
aceitar a requisição seguinte.

Não é uma promessa de documentação. Toda função que abre o store exige um
`OffThread`, cujo construtor é privado ao módulo e só é chamado dentro do fecho
que o `spawn_blocking` executa: uma chamada ao Core na thread do protocolo não
compila. E `noteit-mcp/tests/mcp_concurrency.rs` prova o comportamento em duas
frentes, sem depender de duração — ver `docs/second-brain.md` §22.

O que isto **não** muda: as operações continuam serializadas onde precisam
estar. O lease de escrita é quem decide quem grava, e ele não ficou mais frouxo
porque a espera saiu do reactor.

---

## 3. Catálogo de tools

Dezesseis tools, todas no namespace `noteit_`. `tools/list` as devolve em ordem
determinística.

### Leitura

| Tool | Entrada | Saída |
| --- | --- | --- |
| `noteit_context` | `query?`, `tags?`, `properties?`, `include_tasks?`, `limit?` | `candidates[]`, contadores, `warnings[]` |
| `noteit_list` | `tags?`, `properties?`, `limit?` | `notes[]`, `count`, `warnings[]` |
| `noteit_read` | `note_id` | `note` (com `revision`), `warnings[]` |
| `noteit_search` | `query`, `tags?`, `properties?`, `limit?` | `query`, `results[]`, `count` |
| `noteit_tasks_list` | `state?`, `tags?`, `properties?`, `limit?` | `tasks[]`, `count` |
| `noteit_trash_list` | — | `entries[]`, `count` |

Contra um store inexistente, nenhuma delas cria uma nota, um diretório, um lock,
um soquete, uma configuração ou um estado. Uma leitura não prepara nada.

#### `noteit_context` — recuperação de contexto

Encontra notas que valem a pena ler sobre alguma coisa, e diz por quê. É
recuperação de contexto: casamento de texto e metadados sobre as notas vivas.
**Não** é busca semântica, não há embeddings, não há modelo, e o produto não vai
chamar de semântico o que é casamento de substring.

`query` casa sobre o texto visível com acentos e caixa dobrados, como a busca.
`tags` e `properties` são **sinais**, não filtro: uma nota que carregue uma
delas vira candidata com `shared_tag` ou `property_match` entre os motivos, e
uma que não carregue nenhuma ainda pode entrar por outro sinal. Sem query, sem
tags e sem properties, a resposta é por recência, e cada candidato diz `recent`.

Cada candidato traz `note_id`, `label`, `snippet`, `updated_at`, `reason[]`,
`matched_text?` e — quando `include_tasks` pede — até três tarefas casadas.

O que ela **nunca** devolve, e é metade do contrato:

```text
o corpo da nota          snippet de no máximo 240 caracteres, e só
uma revision             de tipo nenhum, em lugar nenhum
um caminho               nem em candidato, nem em warning, nem em erro
uma mensagem livre       nem de warning, nem de recusa
um score                 os motivos são um conjunto fechado, sem número
```

Tetos, todos do Core e nenhum inventado aqui: query 512 caracteres (acima
disso é recusa e não corte), 10 candidatos por padrão e 50 no máximo, snippet
240, `matched_text` 240, 3 tarefas por candidato com 120 caracteres cada, 20
warnings. Todo corte é contado — `truncated`/`omitted_count`,
`tasks_truncated`/`omitted_task_count`,
`warnings_truncated`/`omitted_warning_count` — e nunca silencioso.

`readOnlyHint: true`, e a propriedade é real: a tool não tem código de escrita,
não aceita `expected_revision`, não aceita caminho e não chama a autoridade.

Um warning de contexto é `code` e `note_id`, sem `message`: a mensagem do Core
nomeia o arquivo, e essa frase não sai por aqui. Uma recusa é `status: error`
mais `code` — `invalid_input` para uma query longa demais, `store_unavailable`
para um store que não pôde ser varrido — e também sem texto livre.

**O que vem depois dela.** `noteit_context` descobre; `noteit_read` abre. Uma
nota encontrada pelo contexto e ainda não lida não tem como ser gravada, porque
nada na resposta serve de `expected_revision` — ver §5.

### Criação

| Tool | Entrada | `revision`? |
| --- | --- | --- |
| `noteit_create` | `content?`, `tags?`, `properties?` | **não** — uma nota que ainda não existe não tem versão anterior |

### Mutação de nota existente

Todas exigem `note_id` **e** `expected_revision`.

| Tool | Campos próprios | Operação do Core |
| --- | --- | --- |
| `noteit_append` | `text` | `NoteMutation::Append` |
| `noteit_edit` | `body` ou `clear: true` | `ReplaceBody` / `ClearBody` |
| `noteit_tag_add` | `tag` | `AddTag` |
| `noteit_tag_remove` | `tag` | `RemoveTag` |
| `noteit_property_set` | `key`, `value` | `SetProperty` |
| `noteit_property_remove` | `key` | `RemoveProperty` |
| `noteit_task_complete` | `task_ref` | `CompleteTask` |
| `noteit_task_reopen` | `task_ref` | `ReopenTask` |

### Lixeira

| Tool | Entrada | `revision`? |
| --- | --- | --- |
| `noteit_trash_restore` | `note_id` | **não** — restaurar é um movimento, não uma edição |

Um restore não inventa uma precondição só para uniformizar a API: não existe
versão viva da nota que alguém pudesse ter lido. A garantia que ele tem é outra
e continua valendo — se uma nota viva já carrega o mesmo identificador, o
restore é recusado com `trash_target_occupied` e nenhum dos dois arquivos é
tocado.

### Como uma nota é endereçada

Sempre por `note_id`: um UUID completo, ou pelo menos oito caracteres
hexadecimais dele. Nunca um nome de arquivo, nunca um caminho. Um seletor que
contenha `/`, `\`, `..` ou qualquer coisa fora de `0-9a-f-` é recusado antes de
tocar o store.

---

## 4. Saídas estruturadas

Toda tool publica um `outputSchema` e responde com `structuredContent`. Os blocos
de texto existem para uma pessoa lendo um log; **nenhuma decisão programática
pode depender deles.**

### Gravações

Uma forma só, para criação, mutação e restore:

```jsonc
// gravou
{ "status": "ok", "commit_state": "committed",
  "note_id": "8c4f1a2b-…", "changed": true, "revision": "a91c…" }

// a nota já dizia exatamente isso
{ "status": "ok", "commit_state": "not_needed",
  "note_id": "8c4f1a2b-…", "changed": false, "revision": "6d2f…" }

// recusou
{ "status": "error", "commit_state": "not_committed",
  "code": "revision_conflict", "note_id": "8c4f1a2b-…",
  "expected_revision": "6d2f…", "current_revision": "a91c…" }

// não se sabe
{ "status": "indeterminate", "commit_state": "unknown", "code": "indeterminate" }
```

As perguntas que um cliente responde por campo tipado:

```text
funcionou?              status
está no disco?          commit_state
mudou alguma coisa?     changed
o que envio a seguir?   revision
por que recusou?        code
```

`commit_state` está presente em toda resposta de gravação, inclusive nas
recusas, porque é o campo que responde “posso repetir?”.

A resposta **não** traz um campo `kind`. A interface de máquina do
`noteit --json` traz, porque um documento por execução precisa dizer qual
comando o produziu; aqui o agente sabe qual tool chamou, e um
`content_appended` ao lado de uma resposta de `noteit_append` não lhe diz nada
de novo. É uma decisão, e está presa por um `match` exaustivo sobre
`WriteOutcomeKind` em `noteit-mcp/tests/mcp_contract_decisions.rs`: um resultado
novo no Core não compila até alguém decidir se ele continua implícito na tool
chamada ou se passou a ser algo que o agente precisa saber.

### Leituras

```jsonc
{ "status": "ok",
  "note": { "note_id": "…", "label": "…", "content": "…Markdown bruto…",
            "tags": [], "properties": [{ "key": "…", "value": "…" }],
            "created_at": "…Z", "updated_at": "…Z", "revision": "a91c…" },
  "warnings": [] }
```

`content` é o Markdown como o Core o contém, sem sanitização: o escape JSON é o
que torna um caractere de controle seguro, e mutilar o corpo entregaria ao
agente um texto que a nota não contém.

`warnings` traz as notas que não puderam ser lidas, ao lado das que puderam. Um
arquivo danificado é um aviso, nunca um store que não responde.

Uma listagem **não** publica `revision`. Um resumo não é uma base sobre a qual
gravar.

---

## 5. `revision` — o contrato central

A `revision` é o SHA-256 dos bytes exatos com que a nota seria persistida:
sessenta e quatro caracteres hexadecimais minúsculos. Ela cobre corpo, tags,
propriedades, cor, papel, tamanho de fonte, carimbos de tempo e o front matter
desconhecido que o Note-it preserva. Não é `mtime`, não é `updated_at`, e não é
segredo nenhum. Ver `noteit-core/src/revision.rs` e `docs/machine-interface.md`
§10.

### O fluxo obrigatório

```text
1. noteit_read                       → revision R1
2. o agente olha o conteúdo e decide
3. mutation(expected_revision = R1)

5a. R1 ainda é a versão atual  → commit, nova revision R2
5b. a nota mudou               → revision_conflict, zero bytes alterados
```

### Por que ela é obrigatória aqui e opcional na CLI

`noteit editar <id>` sem `--if-revision` continua sendo *last writer wins*, e
está certo: a pessoa que digitou o comando está olhando para a nota.

Um agente não está. Por isso, na fronteira MCP, `expected_revision` é um campo
**obrigatório do schema** — não um `Option` com um valor padrão. Uma requisição
sem ele é recusada pela desserialização, antes de qualquer código deste
repositório rodar, antes de um store ser aberto e antes de um lease ser tomado.
Uma revisão malformada — comprimento errado, maiúsculas, não hexadecimal — é um
`invalid_input`, e nunca “sem precondição”.

Não existe gravação MCP incondicional sobre nota existente. Não há flag, campo,
argumento extra ou ordem de chamada que produza uma.

---

## 6. `revision_conflict` — releia, não repita

```jsonc
{ "status": "error", "commit_state": "not_committed",
  "code": "revision_conflict",
  "expected_revision": "6d2f…", "current_revision": "a91c…" }
```

Garantias, quando isso acontece:

- o arquivo é **byte a byte idêntico** ao que era;
- nenhum backup novo;
- nenhum temporário sobrevivente;
- `updated_at` intacto.

**A regra:**

> Um `revision_conflict` exige releitura e uma nova decisão. Nunca uma nova
> tentativa automática.

Nem mesmo usando a `current_revision` que o erro devolveu. Esse valor existe
para o cliente **saber que a nota mudou**, não para gravar por cima de um
conteúdo que ele não olhou — usá-lo assim seria exatamente a sobrescrita
silenciosa que todo esse mecanismo existe para impedir. Por isso a resposta de
conflito deliberadamente **não** traz o campo `revision`, e deliberadamente não
traz o novo conteúdo.

O caminho correto é: `noteit_read` de novo, olhar o que a nota diz agora,
decidir de novo, e mandar uma requisição nova.

Isso vale inclusive quando alguém está digitando na janela aberta e o texto
ainda não foi salvo. A instância que segura o store aplica a precondição contra
a nota **como a janela realmente a tem**, com o texto não salvo incluído — então
uma revisão tirada do arquivo já está velha, e a gravação do agente é recusada
antes de poder apagar o que está na tela da pessoa.

---

## 7. `indeterminate` — nunca repita

```jsonc
{ "status": "indeterminate", "commit_state": "unknown", "code": "indeterminate" }
```

Isso significa que a requisição **foi enviada** e a resposta se perdeu. Pode ter
sido gravada, pode não ter sido, e daqui não há como saber.

Não é uma falha. `commit_state` é `unknown` e nunca `not_committed`, porque
tratar isso como “não gravou” é exatamente como o mesmo parágrafo vai parar duas
vezes dentro de uma nota.

**A regra:**

> Um resultado `indeterminate` nunca é repetido automaticamente.

Leia a nota e diga à pessoa o que encontrou.

---

## 8. Códigos de erro

Uma recusa vem com `isError: true` **e** com `structuredContent` — o sinalizador
para um host que só olha para ele, os campos tipados para um cliente que precisa
decidir o que fazer.

| `code` | `commit_state` | O que significa |
| --- | --- | --- |
| `invalid_input` | `not_committed` | seletor, revisão, referência de tarefa ou argumento inválido |
| `validation` | `not_committed` | uma regra do domínio recusou o valor |
| `not_found` | `not_committed` | nenhuma nota atende ao seletor |
| `ambiguous_selector` | `not_committed` | mais de uma nota atende ao seletor |
| `revision_conflict` | `not_committed` | a nota mudou desde a leitura — releia, §6 |
| `stale_task_ref` | `not_committed` | a referência não nomeia mais uma tarefa desta nota |
| `ambiguous_task_ref` | `not_committed` | a referência casa com mais de uma tarefa |
| `writer_busy` | `not_committed` | outro escritor está usando o store |
| `authority_unavailable` | `not_committed` | o store está seguro e quem o segura não respondeu — *fail closed* |
| `trash_target_occupied` | `not_committed` | uma nota viva já carrega esse identificador |
| `persistence` | `not_committed` | a gravação foi tentada e não aconteceu; o arquivo está intacto |
| `store_unavailable` | `not_committed` | o store não pôde ser lido |
| `read_failed` | — | uma nota ou uma listagem não pôde ser lida |
| `indeterminate` | `unknown` | a resposta se perdeu — §7 |

`authority_unavailable` merece uma frase: quando o store está seguro por outra
instância que não responde, **nada é gravado**. Não existe caminho alternativo,
não existe “só desta vez”, e a recusa é dita em vez de contornada. Um fallback
para gravação direta reintroduziria precisamente a edição perdida que o lease
existe para impedir.

Um campo do schema que falta é recusado pelo SDK durante a desserialização,
antes do corpo da tool. Essa recusa vem como erro de tool nomeando o campo
ausente, e sem `structuredContent` — porque a requisição nunca chegou ao domínio.
Um cliente que respeita o `inputSchema` publicado nunca a encontra.

---

## 9. Segurança

**Não há filesystem genérico.** Não existe `read_file`, `write_file`,
`list_directory`, `delete_file`, `open_path`, `shell`, `exec`, `bash`,
`run_noteit` nem `raw_command`. Cada tool corresponde a uma operação que o
Note-it já sabe fazer. Uma tool genérica destruiria o limite inteiro: todas as
garantias — o lease, a identidade da nota, a precondição da gravação — ficariam a
uma chamada de distância de serem contornadas.

**Nenhum argumento nomeia um caminho.** Não há campo para isso em lugar nenhum
do contrato, então não há o que validar e não há o que escapar. O store não é
escolhível por argumento.

**Nenhum shell.** Nada aqui monta uma linha de comando, e nada aqui inicia um
processo.

**Sem rede, verificado em cinco camadas.** A frase "stdio only" só vale se algo
a impuser, e nenhuma das camadas abaixo basta sozinha:

| Camada | O que fecha | Onde |
| --- | --- | --- |
| Grafo de dependências | nenhum crate de HTTP, TLS, OAuth, SSE, WebSocket ou socket | `scripts/check-mcp-boundary` regra 2 |
| Features resolvidas | `tokio` sem a feature `net`, então `tokio::net` **não existe** neste build | regra 2b |
| Código do `noteit-mcp` | nenhum `std::net`, nenhum tipo de socket, de nenhuma família | regra 5b |
| Código do `noteit-core` | nenhuma API de Internet; o socket **Unix** da autoridade continua permitido, e é exigido que continue existindo | regra 5c |
| Processo em execução | amostragem contínua do que o processo tem aberto, durante toda a operação | `noteit-mcp/tests/mcp_no_network.rs` |

A terceira camada faltava até a auditoria 4.1R1: `std::net` está na biblioteca
padrão, então um `TcpListener::bind` dentro de um handler não aparece em árvore
de dependências nenhuma. A quarta faltava até a 4.1R1.1, e é a que faz a
afirmação cobrir o caminho inteiro em vez de um crate dele — o adaptador MCP é
uma casca fina sobre o `noteit-core`, e "o servidor MCP não tem rede" só é
verdade se o Core que ele chama também não tiver.

### A linha é a família do endereço, não a palavra "socket"

O Note-it **usa** socket, de propósito: é assim que uma gravação chega à
instância que segura o store. Ele é AF_UNIX, é local, e pertence ao
`noteit-core`. Proibir "socket" apagaria a arquitetura em vez de protegê-la.

```text
AF_INET / AF_INET6   proibidos nos dois crates
AF_UNIX              permitido, e apenas no noteit-core
```

O `noteit-mcp` não abre socket algum — alcança a autoridade **chamando** o
Core. E a regra que permite AF_UNIX no Core vem acompanhada de uma asserção de
que o mecanismo ainda está lá, para que ela nunca possa ser satisfeita
apagando justamente aquilo que ela existe para permitir.

### O que a camada dinâmica prova, e o que não prova

Isto está escrito por extenso porque a auditoria 4.1R1.1 encontrou exatamente o
contrário: uma frase mais forte do que o mecanismo.

**Sólido.** Se o processo tem *um socket aberto* é lido direto do symlink em
`/proc/<pid>/fd`, que não pode errar sobre o tipo do descritor. Nos caminhos em
que o Core não precisa de socket — toda leitura, e uma gravação feita
diretamente com o lease livre — a asserção é que **nenhum socket existe em
nenhuma amostra**. A observação é contínua durante toda a operação, com
intervalo médio medido de 14 µs a 76 µs.

**Melhor esforço, e deliberadamente não é o que sustenta a garantia.** A
*família* de um socket é consultada nas tabelas do núcleo. Dois limites foram
medidos, não presumidos:

- um socket criado com `socket(AF_INET, …)` e nunca vinculado **não** aparece
  em `/proc/net/tcp`;
- um socket que fecha entre a leitura do descritor e a leitura da tabela já
  saiu dela. O laço de retry do caminho fail-closed produz exatamente isso,
  dezenas de vezes, com sockets Unix legítimos.

O classificador portanto **não tem falsos positivos** — se disser Internet, o
inode estava numa tabela de Internet — mas pode não ver. Por isso a garantia de
família repousa nas camadas estáticas, e não nesta.

**Controle positivo.** Um observador que não vê nada é indistinguível de um
observador que não está olhando. Então a gravação pela autoridade serve de
controle: o Core abre um socket, entrega a mudança e o fecha dentro da mesma
chamada MCP — exatamente a forma que a prova anterior não enxergava. O monitor
é **obrigado** a vê-lo; se não vir, o teste falha e o resultado limpo ao lado
dele não é aceito.

**Conteúdo é conteúdo.** Aspas, barras invertidas, novas linhas, tabulações,
Unicode, emoji, RTL legítimo, controles bidi, sequências ANSI, caracteres de
controle, Markdown hostil, strings que parecem caminhos e strings que parecem
comandos entram na nota como texto e voltam inalterados. Nenhum deles vira um
caminho, um argumento ou uma instrução.

**Campos não declarados são ignorados.** Um `force`, um `unconditional` ou um
`note_id_override` inventado por um cliente não muda nada: a gravação vai para a
nota que `note_id` nomeia, com a precondição que `expected_revision` carrega.

**Uma identidade física por store.** Aliases do mesmo diretório — link
simbólico, `./`, `..`, barras redundantes — resolvem para a mesma identidade,
o mesmo lease e o mesmo escritor. Um servidor MCP iniciado por uma grafia
diferente não levanta uma segunda autoridade.

**Notas são notas.** Uma mutação de nota não altera `config.toml` nem
`state.json`, e o MCP não inventa estado de sessão dentro do Note-it.

---

## 10. Diferenças entre a CLI humana e o MCP

| | `noteit` (CLI) | `noteit-mcp` |
| --- | --- | --- |
| Quem chama | uma pessoa olhando a nota | um agente |
| Gravação sem precondição | permitida (*last writer wins*) | **impossível** |
| Nome da precondição | `--if-revision` | `expected_revision` |
| Precondição obrigatória | não | **sim**, no schema |
| Saída | texto para pessoa, ou `--json` | `structuredContent` MCP |
| Contrato de versão | `schema_version` do `--json` | negociado pelo SDK MCP |

Os dois contratos são independentes e se movem por razões diferentes. Adicionar
o servidor MCP **não** mudou `schema_version`.

---

## 11. Onde isso é provado

| Área | Suíte |
| --- | --- |
| Catálogo, schemas, stdout, leitura pura | `noteit-mcp/tests/mcp_surface.rs` |
| `revision`, conflito, corrida, `indeterminate` | `noteit-mcp/tests/mcp_revision.rs` |
| Toda variante de `NoteMutation`, por exaustão compilada | `noteit-mcp/tests/mcp_mutation_matrix.rs` |
| Autoridade, lixeira, aliases de store, protocolo privado | `noteit-mcp/tests/mcp_authority.rs` |
| Identidade da nota e conteúdo hostil | `noteit-mcp/tests/mcp_identity_and_content.rs` |
| O protocolo MCP em si | `noteit-mcp/tests/mcp_protocol.rs` |
| Nenhum socket aberto pelo processo real | `noteit-mcp/tests/mcp_no_network.rs` |
| Decisões do contrato, presas por `match` exaustivo | `noteit-mcp/tests/mcp_contract_decisions.rs` |
| Limite headless, sem rede, sem shell | `scripts/check-mcp-boundary` |

Todas usam o binário real, processos reais, soquetes reais e stores descartáveis
em diretórios temporários. Nenhuma toca o store de ninguém.
