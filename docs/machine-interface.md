# Interface da máquina — `noteit --json`

O contrato estável e versionado entre a linha de comando `noteit` e os scripts e agentes que o chamam. Tudo o que um consumidor precisa para decidir o que fazer a seguir é um campo digitado. Nenhuma decisão requer leitura, tradução ou correspondência de padrões de uma frase.

Este documento é o contrato. Se a implementação e este arquivo discordarem, isso é um bug.

---

## 1. Ativação

```bash
noteit --json listar
noteit listar --json
noteit --json ler <ID>
noteit adicionar <ID> "texto" --json
noteit tags adicionar <ID> Medicina --json
```

`--json` é uma opção global: é aceita antes do comando, depois dele e em todos os níveis de um comando agrupado. Funciona com os aliases internacionais exatamente como funciona com a grafia portuguesa e produz o mesmo documento de qualquer maneira.

**`--json` é uma opção, nunca uma palavra.** Após o escape `--` tudo é um valor:

```bash
noteit adicionar <ID> -- --json      # acrescenta o texto literal "--json"; saída para pessoas
```

O modo é decidido a partir da opção real e nunca a partir de uma substring, nunca a partir da entrada padrão e nunca a partir de nada após `--`.

---

## 2. Um documento por execução

Exatamente um documento JSON é escrito, terminando em um único `\n`:

| resultado                     | stdout            | stderr            | saída |
| -------------------------- | ----------------- | ----------------- | ---- |
| sucesso                    | o documento      | *vazio*           | 0    |
| sucesso com aviso            | o documento      | *vazio*           | 0    |
| erro de execução            | *vazio*           | o documento      | 1    |
| erro de uso                | *vazio*           | o documento      | 2    |
| indeterminado              | *vazio*           | o documento      | 1    |

Nada mais é gravado em nenhum dos canais no modo máquina: nenhum `Aviso:`, nenhum `Erro:`, nenhuma prosa de uso, nenhum progresso, nenhum ANSI. Analisar um canal inteiro sempre funciona. Nunca existe um segundo documento – NDJSON deliberadamente não faz parte deste contrato.

ANSI nunca é emitido no modo máquina, esteja o processo conectado a um terminal ou não. `NO_COLOR` é irrelevante aqui porque não há nada para desligar.

A apresentação que a Fase 4.0G deu a `noteit` sem argumentos também não alcança este contrato. `noteit --json` responde com o documento `welcome` e nada mais: nenhum logotipo, nenhuma cor, nenhuma dica humana, nenhum "Comece por:", antes ou depois do documento. O mesmo vale para toda a matriz de terminal — a largura da janela, `TERM=dumb`, `COLUMNS` — que não altera um único byte de nenhum documento. É afirmado comparando, byte a byte, a mesma execução sobre um terminal real e dentro de um cano.

---

## 3. O envelope

```json
{
  "schema_version": 1,
  "status": "ok",
  "command": "append",
  "data": { "...": "..." },
  "error": null,
  "warnings": []
}
```

Todas as seis chaves estão sempre presentes. `data` é `null` em caso de falha e `error` é `null` em caso de sucesso. A ordem das chaves não faz parte do contrato.

### `schema_version`

Um número inteiro. `1` hoje.

- Novos campos **opcionais** podem ser adicionados sem alterá-los.
- Renomear um campo, removê-lo ou alterar seu significado requer uma nova versão explícita.
- Os consumidores devem ignorar os campos que não conhecem e não devem depender da ordem das chaves.

### `status`

Tokens de máquina estáveis, nunca traduzidos:

| valor           | significado                                                                 |
| --------------- | ----------------------------------------------------------------------- |
| `ok`            | o comando fez o que foi solicitado e não relatou mais nada                |
| `warning`       | o comando fez o que foi pedido e `warnings` não está vazio              |
| `error`         | o comando não fez o que foi pedido                                   |
| `indeterminate` | o pedido foi enviado e o resultado é genuinamente desconhecido — ver §8       |

`status` é `warning` se e somente se `warnings` não estiver vazio em um comando bem-sucedido.

### `command`

O nome canônico do comando lógico, independente de como foi escrito:

```text
welcome   help    version   status
list      read    search    tags    properties   tasks   trash
create    append  edit
tag_add   tag_remove
property_set   property_remove
task_complete  task_reopen
trash_restore
```

`listar` e `list` produzem `"command": "list"`. `command` é `null` somente quando os argumentos nunca nomearam um comando que esta compilação reconhece — um erro de análise que falhou antes de um comando ser identificado.

### `warnings`

Uma matriz de objetos. Cada um tem `code` (um token estável), `message` (prosa de diagnóstico) e `note_id` (um UUID ou `null` completo).

```text
unreadable_note              uma nota não pôde ser lida e foi omitida do resultado
corrupted_front_matter       não foi possível analisar o front matter de uma nota
symlink_refused              o arquivo de uma nota é um link simbólico e foi recusado
io_error                     o store não pôde ser lido naquele ponto
ui_sync_window_not_confirmed a gravação foi confirmada; a janela aberta não a confirmou — consulte §7
```

Um aviso nunca significa que os dados foram perdidos do resultado: as notas que *poderiam* ser lidas ainda estão em `data` e o código de saída ainda é `0`.

### `error`

```json
{ "code": "not_found", "message": "…", "commit_state": "not_committed" }
```

`commit_state` é `null` para um comando que não poderia ter confirmado nada (qualquer leitura e um erro de análise que não nomeou nenhum comando).

Em um `revision_conflict`, e somente nele, o erro carrega mais dois campos, para que
nenhum consumidor precise ler prosa para descobrir a versão atual:

```json
{
  "code": "revision_conflict",
  "message": "…",
  "commit_state": "not_committed",
  "expected_revision": "6d2f…",
  "current_revision": "a91c…"
}
```

---

## 4. Dados por comando

Os carimbos de data e hora são sempre RFC 3339 em UTC (`2026-09-02T00:35:58Z`) ou `null` quando o store não possui nenhum. Os identificadores são sempre UUIDs completos - nunca o prefixo de oito caracteres para o qual a saída humana é abreviada. Booleanos são booleanos, contagens são números, listas são matrizes.

```jsonc
// boas-vindas
{ "version": "0.1.0", "machine_interface": true }

// ajuda
{ "usage": "noteit [--json] <comando> [opções]", "help": "…texto simples…" }

// versão
{ "version": "0.1.0" }

// status
{ "version": "0.1.0", "cli_ready": true, "core_available": true, "store_exists": true,
  "data_path": "…", "config_path": "…", "state_path": "…" }

// listagem
{ "notes": [ { "note_id": "…", "label": "…", "snippet": "…", "tags": [],
               "properties": [ { "key": "…", "value": "…" } ],
               "created_at": "…Z", "updated_at": "…Z" } ],
  "count": 1 }

// leitura
{ "note": { "note_id": "…", "label": "…", "content": "…Markdown bruto…", "tags": [],
            "properties": [], "created_at": "…Z", "updated_at": "…Z" } }

// pesquisa
{ "query": "biopsia",
  "results": [ { "note_id": "…", "label": "…", "snippet": "…",
                 "match_count": 2, "matched_text": "Biópsia" } ],
  "count": 1 }

// tags
{ "tags": [ { "name": "Medicina", "note_count": 3 } ], "count": 1 }

// propriedades
{ "properties": [ { "key": "fonte", "note_count": 3 } ], "count": 1 }

// tarefas
{ "state": "pending",
  "tasks": [ { "task_ref": "a71bc920", "note_id": "…", "note_label": "…",
               "text": "Revisar noradrenalina", "checked": false,
               "completed_at": null, "depth": 0 } ],
  "count": 1 }

// lixeira
{ "entries": [ { "note_id": "…", "label": "…", "snippet": "…", "deleted_at": "…Z" } ],
  "count": 1 }
```

`state` é `pending`, `completed` ou `all`. Um resultado vazio é `[]` com `"count": 0` e `"status": "ok"` — nunca uma frase.

`content` é o Markdown da nota exatamente como o Core o contém. O sanitizador de terminal que protege o terminal de uma pessoa **não** é aplicado a ele: o escape JSON é o que torna um caractere de controle seguro em um documento que ninguém está renderizando como texto, e mutilar o corpo entregaria um texto de script que a nota não contém. Aspas, barras invertidas, novas linhas, tabulações, emoji e sequências de escape passam de ida e volta inalterados por qualquer analisador JSON.

`task_ref` é produzido por Core e pode ser usado diretamente em `tasks complete` e `tasks reopen`. `note_id` de `trash` pode ser usado diretamente em `trash restore`. Nenhum texto precisa ser analisado para alternar entre listagem e atuação.

---

## 5. Resultados das gravações

Cada comando de gravação responde com a mesma forma:

```json
{
  "schema_version": 1,
  "status": "ok",
  "command": "append",
  "data": {
    "write": {
      "note_id": "8c4f1a2b-1111-2222-3333-444444444444",
      "kind": "content_appended",
      "changed": true,
      "commit_state": "committed",
      "revision": "a91c…",
      "ui_sync": { "status": "ok", "code": null, "message": null }
    }
  },
  "error": null,
  "warnings": []
}
```

`kind` é um dos:

```text
note_created   content_appended   content_replaced   content_cleared
tag_added      tag_removed        property_set       property_removed
task_completed task_reopened      note_restored
```

### `commit_state`

O único campo que um consumidor deve ler antes de decidir se deseja executar um comando novamente.

| valor           | significado                                                            |
| --------------- | ------------------------------------------------------------------ |
| `committed`     | a mudança está no disco                                              |
| `not_needed`    | o store já estava exatamente nesse estado; nada foi gravado  |
| `not_committed` | nada foi escrito                                                |
| `unknown`       | a solicitação foi enviada e se ela foi confirmada não pode ser determinada |

Em caso de sucesso, `commit_state` segue `changed`: `true` → `committed`, `false` → `not_needed`. Um resultado `changed: false` é um **sucesso**, não um fracasso — solicitar uma tag que uma nota já possui é uma solicitação válida cujo estado desejado já foi mantido.

### Regra de nova tentativa

| status          | commit_state    | significado                             | repetir automaticamente?           |
| --------------- | --------------- | ----------------------------------- | ------------------------------- |
| `ok`            | `committed`     | a mudança foi gravada              | **não**                         |
| `warning`       | `committed`     | gravada, com algo adicional a relatar | **não**                       |
| `ok`            | `not_needed`    | o estado solicitado já existia     | desnecessário                   |
| `error`         | `not_committed` | nada foi escrito                 | só depois de consertar a causa     |
| `error`         | `not_committed` | `revision_conflict`: a base mudou   | **nunca sem reler** — ver §10   |
| `indeterminate` | `unknown`       | pode ou não ter sido escrito | **nunca** — uma pessoa deve olhar  |

`not_committed` não significa "tentar novamente agora". Significa que nada foi escrito; se a repetição ajuda depende de `error.code` — um `not_found` não se tornará um `found` na segunda tentativa.

---

## 6. Qual nota está aberta não é problema do consumidor

A mesma operação produz o mesmo documento público, quer `noteit` tenha escrito o arquivo sozinho ou uma instância de desktop Note-it em execução o tenha escrito mediante solicitação. Qual dos dois aconteceu é um detalhe de implementação e não é deliberadamente relatado: não há nada que um consumidor possa fazer de diferente a respeito.

A única diferença legítima é `ui_sync`, porque uma janela só pode estar descompassada quando há uma janela.

---

## 7. `ui_sync` — gravação confirmada, janela desatualizada

```json
"ui_sync": {
  "status": "warning",
  "code": "window_not_confirmed",
  "message": "a nota aberta não conseguiu adotar o documento gravado"
}
```

Quando uma nota é aberta na tela, Note-it congela seu editor, dobra qualquer texto não salvo no mesmo commit, grava o arquivo e então devolve o documento confirmado para a janela. Se a janela não confirmar que pegou o documento, a gravação **ainda será confirmada** — o arquivo no disco contém o novo texto — e apenas a tela ficará para trás.

Esse caso é relatado como:

```text
status              warning
data.write.changed  true
commit_state        committed
ui_sync.status      warning
ui_sync.code        window_not_confirmed
warnings[]          contém ui_sync_window_not_confirmed
código de saída     0
stderr              vazio
```

Nunca é `status: error`, nunca `commit_state: not_committed` e nunca é uma saída diferente de zero. **Repetir o comando acrescentaria o mesmo texto duas vezes.** Um consumidor que ramifica em `ui_sync.status` e `commit_state` não pode cometer esse erro; aquele que lê a mensagem pode.

`ui_sync.status` é `ok` sempre que nada reporta a janela como fora de sintonia, o que inclui cada gravação feita sem nenhuma janela envolvida.

---

## 8. `indeterminate` — o resultado é desconhecido

A solicitação chegou à autoridade, mas a resposta não retornou: a conexão caiu ou a resposta não pertencia à solicitação. A autoridade pode ter confirmado a gravação antes disso, e o chamador não tem como saber.

```json
{
  "schema_version": 1,
  "status": "indeterminate",
  "command": "append",
  "data": null,
  "error": { "code": "indeterminate", "message": "…", "commit_state": "unknown" },
  "warnings": []
}
```

O código de saída é diferente de zero, mas **não** "a gravação falhou". `commit_state` é `unknown` e nunca `not_committed`, precisamente para que um agente não possa tratá-lo como uma falha limpa e tentar novamente.

**Nunca repita uma operação automaticamente após `unknown`.** Leia a nota, decida o que o store realmente possui e aja de acordo.

---

## 9. Códigos de erro

Tokens estáveis. O `message` ao lado deles é uma prosa de diagnóstico legível por humanos; a sua redação, e mesmo a sua linguagem, não fazem parte do contrato.

| código                    | saída  | commit_state em uma gravação | significado                                              |
| ----------------------- | ----- | ----------------------- | ---------------------------------------------------- |
| `usage_error`           | 2     | `not_committed` \*      | o pedido não foi bem formulado                      |
| `invalid_input`         | 2 / 1 | `not_committed`         | seletor, conteúdo ou referência inválidos             |
| `validation`            | 2     | `not_committed`         | uma regra de domínio recusou o valor                      |
| `not_found`             | 1     | `not_committed`         | nenhuma nota ou entrada de lixo responde a esse seletor      |
| `ambiguous_selector`    | 1     | `not_committed`         | mais de uma nota responde a esse seletor          |
| `revision_conflict`     | 1     | `not_committed`         | a nota mudou desde a leitura — ver §10                |
| `stale_task_ref`        | 1     | `not_committed`         | a nota mudou; essa referência já não nomeia a tarefa  |
| `ambiguous_task_ref`    | 1     | `not_committed`         | a referência corresponde a mais de uma tarefa             |
| `writer_busy`           | 1     | `not_committed`         | outro gravador Note-it mantém o store               |
| `authority_unavailable` | 1     | `not_committed`         | o store está ocupado e não foi possível solicitar ao detentor |
| `trash_target_occupied` | 1     | `not_committed`         | uma nota ativa já carrega esse identificador          |
| `persistence`           | 1     | `not_committed`         | a gravação foi tentada e não aconteceu           |
| `store_unavailable`     | 1     | `not_committed`         | o próprio store não pôde ser lido                   |
| `indeterminate`         | 1     | `unknown`               | o resultado é desconhecido — ver §8                       |
| `read_failed`           | 1     | `null`                  | uma nota ou o store não pôde ser lida                |
| `internal_error`        | 1     | `null`                  | a resposta em si não pôde ser produzida              |

\* `usage_error` carrega `not_committed` quando foi gerado contra um comando que grava e `null` quando o comando foi lido ou não pôde ser identificado.

`invalid_input` é o único código associado a dois códigos de saída, comportamento herdado em vez de introduzido aqui: um seletor malformado fornecido a uma **gravação** sempre retornou `2`, enquanto o mesmo seletor fornecido a uma **leitura** sempre retornou `1`. A interface da máquina preserva ambos, em vez de renumerar silenciosamente um contrato selado. Ramifique por `error.code`, não pelo código de saída, sempre que os dois puderem ser diferentes.

Cada um deles, exceto `indeterminate`, significa, sob o contrato atual, que nada foi escrito.

### Erros de análise mantêm o modo máquina

O modo máquina sobrevive a uma lista de argumentos que o analisador não conseguiu ler:

```bash
noteit --json batata                  # → usage_error em stderr, saída 2
noteit --json adicionar               # → usage_error em stderr, saída 2
noteit --json --flag-inexistente      # → usage_error em stderr, saída 2
noteit --json buscar                  # → usage_error em stderr, saída 2
```

Um consumidor que solicitou JSON nunca recebe um parágrafo em português.

---

## 10. `revision` — escrita condicional e concorrência otimista

O lease de escrita responde *quem pode gravar agora*. Ele serializa os gravadores e
não deixa dois se intercalarem. A pergunta que ele não faz é a outra:

> **a nota ainda é a versão que eu li?**

Um cliente programático lê num instante, decide, e grava depois. Entre os dois, uma
pessoa pode ter digitado na janela aberta e outro comando pode ter gravado. Sem uma
precondição, essa gravação apaga o que aconteceu no meio — as duas operações
respondem `committed` e nada falha. É exatamente essa perda que a `revision` fecha.

### O que é

Um token **opaco**: 64 caracteres hexadecimais minúsculos, o SHA-256 da forma
canônica exata em que a nota seria persistida. Cobre tudo que uma gravação
posterior poderia sobrescrever — identificador, corpo, tags, propriedades, cor,
papel, tamanho de fonte, carimbos de tempo e o front matter de terceiros que o
Note-it preserva.

Não recalcule. Guarde o valor que recebeu e devolva-o.

Deliberadamente **não** é `mtime` (resolução variável, alterável por qualquer um,
duas gravações no mesmo tique) nem `updated_at` (informação de domínio: move com o
texto e fica parado numa mudança de tag, então uma escrita obsoleta passaria).

### Como obtê-la

`noteit ler <id> --json` publica `data.note.revision`. Toda gravação bem-sucedida
também devolve `data.write.revision`: a revisão do que ficou no disco quando
`changed` é `true`, e a que a nota já tinha quando é `false`. Encadear operações
não exige uma leitura extra.

### Como usar

```bash
REV=$(noteit ler "$ID" --json | jq -r .data.note.revision)
# … o cliente decide o que gravar …
noteit editar "$ID" "$NOVO_CORPO" --if-revision "$REV" --json
```

No protocolo o campo se chama `expected_revision`; na linha de comando,
`--if-revision`. Aceitam-no todas as mutações de nota existente: `adicionar`,
`editar`, `tags adicionar/remover`, `propriedades definir/remover` e
`tarefas concluir/reabrir`. `criar` não tem base anterior; `lixeira restaurar` é um
movimento e não uma edição, e por isso também não a aceita.

Uma revisão malformada é `usage_error` e **nunca** é tratada como "sem precondição".

### A regra para agentes

> **Toda gravação construída a partir de uma leitura anterior deve enviar a
> `revision` daquela leitura.**

Sem `--if-revision` a gravação é incondicional e continua sendo *last writer wins* —
que é o que uma pessoa digitando `noteit editar` está pedindo, e por isso continua
sendo o padrão.

### Conflito

```json
{
  "status": "error",
  "command": "edit",
  "data": null,
  "error": {
    "code": "revision_conflict",
    "commit_state": "not_committed",
    "expected_revision": "6d2f…",
    "current_revision": "a91c…"
  }
}
```

Um conflito significa, sem exceção: **nenhum byte da nota mudou**, nenhum backup foi
criado, nenhum `updated_at` se moveu, nenhum temporário sobreviveu.

### Nova tentativa

Um conflito **não** pode ser reexecutado automaticamente. Nem com a
`current_revision` que ele devolveu: esse valor existe para o cliente saber que a
nota se moveu, não para reenviar a mesma escrita por cima do que ele ainda não
olhou. Repetir com ela seria a sobrescrita silenciosa que a precondição existe para
impedir.

O caminho é sempre o mesmo:

```text
conflito → reler → reconciliar conscientemente → gravar de novo
```

Não há merge automático. Um conflito continua um conflito até alguém decidir.

### Nota aberta com texto ainda não salvo

Quando a nota está aberta e o editor tem texto que o autosave ainda não gravou, a
precondição é comparada contra **esse** estado vivo, não contra o arquivo. Qualquer
revisão obtida por leitura é, por definição, antiga em relação a ele, então a
gravação é recusada — que é o resultado correto: um agente não pode apagar o
parágrafo que a pessoa acabou de digitar. Nenhuma leitura publica a revisão do
estado vivo, e tentar de novo com a `current_revision` devolvida conflita de novo.

### Escritores fora do Note-it

A garantia é sobre **escritores cooperativos** — tudo que passa pelo lease do
Note-it. Um editor externo que grava direto no `.md` não coopera com nada, e um
sistema de arquivos comum não oferece compare-and-swap de conteúdo. Uma gravação
condicional pelo CLI ainda o protege, porque relê o arquivo antes de mutar e recusa
um cliente obsoleto; a janela aberta do Note-it, porém, ainda pode sobrescrever uma
edição externa no seu próximo autosave. Ver `docs/storage.md`.

---

## 11. O que deliberadamente não está aqui

- **O protocolo de controle privado.** Os identificadores de solicitação, a versão do protocolo, o caminho do soquete, o bloqueio do gravador, a geração da janela e o caminho de gravação executado são uma conversa entre dois processos Note-it. Eles não fazem parte desta API e nunca são serializados nela, embora ambos usem JSON.
- **Caminhos do sistema de arquivos**, exceto em `status`, onde são o ponto do comando.
- **Entrada em JSON.** `--json` descreve apenas a saída. As cargas ainda chegam como argumentos ou na entrada padrão via `--stdin`, inalteradas e não codificadas.
- **Impressão bonita, NDJSON, lote, streaming, um daemon, MCP.** Um comando, um documento.

---

## 12. Responder às perguntas importantes, sem ler uma palavra

| pergunta                             | campo                                             |
| ------------------------------------ | ------------------------------------------------- |
| o comando funcionou?                | `status`                                          |
| havia algo a relatar?       | `warnings`, `status == "warning"`                 |
| alguma coisa realmente mudou?       | `data.write.changed`                              |
| o commit aconteceu?               | `data.write.commit_state == "committed"`          |
| um commit era mesmo necessário?            | `commit_state == "not_needed"`                    |
| o commit definitivamente não aconteceu?| `commit_state == "not_committed"`                 |
| o resultado do commit é desconhecido?        | `status == "indeterminate"`, `commit_state == "unknown"` |
| qual nota foi afetada?             | `data.write.note_id` (um UUID completo)                |
| qual operação aconteceu?            | `command`, `data.write.kind`                      |
| a janela aberta está descompassada?      | `data.write.ui_sync.status`                       |
| o que deu errado?                     | `error.code`                                      |

Existem campos `message` para uma pessoa que lê um registro. Um consumidor que ramifica em uma delas interpretou mal essa interface.
