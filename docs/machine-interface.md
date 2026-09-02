# Interface da máquina — `noteit --json`

O contrato estável e versionado entre a linha de comando `noteit` e os scripts e agentes que a chamam. Tudo o que um consumidor precisa para decidir o que fazer a seguir é um campo tipado. Nenhuma decisão exige leitura, tradução ou correspondência de padrões (regex) sobre frases.

Este documento é o contrato. Se a implementação e este arquivo divergirem, trata-se de um bug.

---

## 1. Ativação

```bash
noteit --json listar
noteit listar --json
noteit --json ler <ID>
noteit adicionar <ID> "texto" --json
noteit tags adicionar <ID> Medicina --json
```

`--json` é uma opção global: é aceita antes do comando, depois dele e em qualquer nível de um comando agrupado. Funciona com os aliases internacionais exatamente como funciona com a grafia em português, produzindo o mesmo documento em ambos os casos.

**`--json` é uma opção, nunca uma palavra de argumento.** Após o delimitador de escape `--`, tudo é tratado como valor literal:

```bash
noteit adicionar <ID> -- --json      # anexa o texto literal "--json"; saída humana
```

O modo é decidido a partir da opção real e nunca a partir de substrings, da entrada padrão ou de qualquer argumento após `--`.

---

## 2. Um documento por execução

Exatamente um documento JSON é emitido, terminando em um único `\n`:

| resultado | stdout | stderr | exit |
| --- | --- | --- | --- |
| sucesso | o documento | *vazio* | 0 |
| sucesso com aviso (warning) | o documento | *vazio* | 0 |
| erro de execução | *vazio* | o documento | 1 |
| erro de uso | *vazio* | o documento | 2 |
| indeterminado | *vazio* | o documento | 1 |

Nada mais é emitido em nenhum dos canais no modo máquina: sem `Aviso:`, sem `Erro:`, sem texto explicativo de uso, sem progresso, sem caracteres ANSI. Fazer o parsing de um canal completo sempre funciona. Nunca é emitido um segundo documento — NDJSON deliberadamente não faz parte deste contrato.

Sequências de escape ANSI nunca são emitidas no modo máquina, esteja o processo conectado a um terminal ou não. A variável `NO_COLOR` é irrelevante aqui porque não há estilização para desativar.

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

Um número inteiro. Atualmente `1`.

- Novos campos **opcionais** podem ser adicionados sem alterá-lo.
- Renomear um campo, remover um campo ou alterar o significado de um campo exige uma nova versão explícita.
- Os consumidores devem ignorar campos desconhecidos e não devem depender da ordem das chaves.

### `status`

Tokens estáveis de máquina, nunca traduzidos:

| valor | significado |
| --- | --- |
| `ok` | o comando realizou o que foi solicitado e não reportou nenhuma anomalia |
| `warning` | o comando realizou o que foi solicitado e `warnings` não está vazio |
| `error` | o comando não realizou o que foi solicitado |
| `indeterminate` | a solicitação foi enviada e o resultado é genuinamente desconhecido — consulte §8 |

`status` é `warning` se, e somente se, `warnings` não estiver vazio em um comando bem-sucedido.

### `command`

O nome canônico do comando lógico, independentemente de como foi digitado:

```text
welcome   help    version   status
list      read    search    tags    properties   tasks   trash
create    append  edit
tag_add   tag_remove
property_set   property_remove
task_complete  task_reopen
trash_restore
```

Tanto `listar` quanto `list` produzem `"command": "list"`. `command` é `null` apenas quando os argumentos não nomearem nenhum comando reconhecido por esta versão — um erro de parsing que falhou antes que o comando pudesse ser identificado.

### `warnings`

Um array de objetos. Cada objeto contém `code` (um token estável), `message` (texto de diagnóstico) e `note_id` (um UUID completo ou `null`).

```text
unreadable_note              uma nota não pôde ser lida e foi omitida do resultado
corrupted_front_matter       o front matter de uma nota não pôde ser analisado
symlink_refused              o arquivo da nota é um link simbólico e foi recusado
io_error                     o store não pôde ser lido naquele ponto
ui_sync_window_not_confirmed a gravação comitou; a janela aberta não confirmou — consulte §7
```

Um aviso nunca significa perda de dados no resultado: as notas que *puderam* ser lidas permanecem em `data`, e o código de saída continua sendo `0`.

### `error`

```json
{ "code": "not_found", "message": "…", "commit_state": "not_committed" }
```

`commit_state` é `null` para comandos que não poderiam ter realizado commit (qualquer comando de leitura e erros de parsing sem identificação de comando).

---

## 4. Dados por comando

Timestamps são sempre formatados em RFC 3339 em UTC (`2026-09-02T00:35:58Z`) ou `null` quando o store não possuir a informação. Identificadores são sempre UUIDs completos — nunca o prefixo de oito caracteres utilizado na saída humana. Booleanos são booleanos, contagens são números, listas são arrays.

```jsonc
// welcome
{ "version": "0.1.0", "machine_interface": true }

// help
{ "usage": "noteit [--json] <comando> [opções]", "help": "…plain text…" }

// version
{ "version": "0.1.0" }

// status
{ "version": "0.1.0", "cli_ready": true, "core_available": true, "store_exists": true,
  "data_path": "…", "config_path": "…", "state_path": "…" }

// list
{ "notes": [ { "note_id": "…", "label": "…", "snippet": "…", "tags": [],
               "properties": [ { "key": "…", "value": "…" } ],
               "created_at": "…Z", "updated_at": "…Z" } ],
  "count": 1 }

// read
{ "note": { "note_id": "…", "label": "…", "content": "…raw Markdown…", "tags": [],
            "properties": [], "created_at": "…Z", "updated_at": "…Z" } }

// search
{ "query": "biopsia",
  "results": [ { "note_id": "…", "label": "…", "snippet": "…",
                 "match_count": 2, "matched_text": "Biópsia" } ],
  "count": 1 }

// tags
{ "tags": [ { "name": "Medicina", "note_count": 3 } ], "count": 1 }

// properties
{ "properties": [ { "key": "fonte", "note_count": 3 } ], "count": 1 }

// tasks
{ "state": "pending",
  "tasks": [ { "task_ref": "a71bc920", "note_id": "…", "note_label": "…",
               "text": "Revisar noradrenalina", "checked": false,
               "completed_at": null, "depth": 0 } ],
  "count": 1 }

// trash
{ "entries": [ { "note_id": "…", "label": "…", "snippet": "…", "deleted_at": "…Z" } ],
  "count": 1 }
```

`state` assume os valores `pending`, `completed` ou `all`. Um resultado vazio é `[]` com `"count": 0` e `"status": "ok"` — nunca uma frase de texto.

`content` contém o Markdown da nota exatamente como o Core o mantém. O sanitizador de terminal que protege terminais humanos **não** é aplicado a ele: o escape padrão de JSON é o mecanismo que torna caracteres de controle seguros em um documento que ninguém está renderizando como texto puro, e alterar o corpo entregaria ao script um texto que a nota não contém. Aspas, barras invertidas, quebras de linha, tabulações, emojis e sequências de escape realizam a conversão bidirecional (round-trip) sem alterações através de qualquer parser JSON.

`task_ref` é gerado pelo Core e pode ser utilizado diretamente em `tasks complete` e `tasks reopen`. O `note_id` retornado por `trash` pode ser utilizado diretamente em `trash restore`. Nenhum texto precisa ser recortado ou processado para transitar entre listar e agir.

---

## 5. Resultados de gravação

Todo comando de gravação responde com a mesma estrutura:

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
      "ui_sync": { "status": "ok", "code": null, "message": null }
    }
  },
  "error": null,
  "warnings": []
}
```

`kind` assume um dos seguintes valores:

```text
note_created   content_appended   content_replaced   content_cleared
tag_added      tag_removed        property_set       property_removed
task_completed task_reopened      note_restored
```

### `commit_state`

O campo fundamental que um consumidor deve inspecionar antes de decidir se deve executar um comando novamente.

| valor | significado |
| --- | --- |
| `committed` | a alteração está persistida em disco |
| `not_needed` | o store já continha exatamente esse estado; nada precisou ser gravado |
| `not_committed` | nada foi gravado em disco |
| `unknown` | a solicitação foi enviada e não é possível determinar se o commit ocorreu |

Em caso de sucesso, `commit_state` acompanha `changed`: `true` → `committed`, `false` → `not_needed`.
Um resultado `changed: false` é um **sucesso**, não uma falha — solicitar uma tag que a nota já possui é uma requisição válida cujo estado desejado já estava satisfeito.

### Regra de repetição (retry)

| status | commit_state | significado | repetir automaticamente? |
| --- | --- | --- | --- |
| `ok` | `committed` | a alteração foi gravada | **não** |
| `warning` | `committed` | gravado, com aviso a relatar | **não** |
| `ok` | `not_needed` | o estado solicitado já estava presente | desnecessário |
| `error` | `not_committed` | nada foi gravado | somente após corrigir a causa |
| `indeterminate` | `unknown` | a gravação pode ou não ter ocorrido | **nunca** — requer intervenção humana |

`not_committed` não significa "tente novamente agora". Significa que nada foi gravado; se repetir a operação resolverá o problema depende de `error.code` — um erro `not_found` não se transformará em sucesso em uma segunda tentativa idêntica.

---

## 6. Qual nota está aberta na interface não é preocupação do consumidor

A mesma operação produz o mesmo documento público quer o `noteit` tenha gravado o arquivo diretamente, quer uma instância de desktop do Note-it em execução tenha realizado a gravação sob demanda. Qual dos dois cenários ocorreu é um detalhe interno de implementação e deliberadamente não é reportado: não há nada que um consumidor possa fazer de diferente a respeito.

A única diferença legítima é `ui_sync`, pois uma janela só pode estar desatualizada quando houver uma janela aberta.

---

## 7. `ui_sync` — commit realizado, com a janela gráfica desatualizada

```json
"ui_sync": {
  "status": "warning",
  "code": "window_not_confirmed",
  "message": "a nota aberta não conseguiu adotar o documento gravado"
}
```

Quando uma nota está aberta na tela, o Note-it congela seu editor, incorpora qualquer texto não salvo no mesmo commit, grava o arquivo em disco e então devolve o documento com commit para a janela. Se a janela não confirmar que adotou o documento, a gravação **ainda estará confirmada (committed)** — o arquivo em disco contém o novo texto — e apenas a exibição na tela estará desatualizada.

Esse caso é reportado como:

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

Nunca é `status: error`, nunca é `commit_state: not_committed` e nunca retorna código de saída diferente de zero. **Repetir o comando acrescentaria o mesmo texto duas vezes.** Um consumidor que crie ramificações lógicas baseando-se em `ui_sync.status` e `commit_state` não comete esse erro; um que tente interpretar a mensagem em texto corrido poderia cometer.

`ui_sync.status` é `ok` sempre que nenhuma anomalia de sincronização com a janela foi reportada, o que inclui todas as gravações realizadas sem nenhuma janela gráfica envolvida.

---

## 8. `indeterminate` — o resultado é desconhecido

A solicitação alcançou a autoridade de escrita, mas a resposta não retornou: a conexão caiu ou a resposta não correspondeu à requisição. A autoridade pode ter realizado o commit antes disso ter acontecido, e não há como determinar a partir do chamador.

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

O código de saída é diferente de zero, mas isso **não** significa que "a gravação falhou". `commit_state` é `unknown` e nunca `not_committed`, precisamente para impedir que um agente automatizado o trate como uma falha limpa e tente novamente de forma cega.

**Nunca repita uma operação automaticamente após receber `unknown`.** Leia a nota, verifique o estado real do store e tome decisões baseadas no conteúdo existente.

---

## 9. Códigos de erro

Tokens estáveis. O campo `message` ao lado é um diagnóstico em prosa legível por humanos; sua redação e até mesmo seu idioma não fazem parte do contrato rígido.

| code | exit | commit_state em gravação | significado |
| --- | --- | --- | --- |
| `usage_error` | 2 | `not_committed` \* | a requisição não foi bem formada |
| `invalid_input` | 2 / 1 | `not_committed` | um seletor, payload ou referência inválido |
| `validation` | 2 | `not_committed` | uma regra de domínio recusou o valor |
| `not_found` | 1 | `not_committed` | nenhuma nota ou item da lixeira corresponde ao seletor |
| `ambiguous_selector` | 1 | `not_committed` | mais de uma nota corresponde ao seletor |
| `stale_task_ref` | 1 | `not_committed` | a nota foi alterada; a referência não mais a identifica |
| `ambiguous_task_ref` | 1 | `not_committed` | a referência corresponde a mais de uma tarefa |
| `writer_busy` | 1 | `not_committed` | outro escritor Note-it detém o store |
| `authority_unavailable` | 1 | `not_committed` | o store está retido e não foi possível consultar o detentor |
| `trash_target_occupied` | 1 | `not_committed` | uma nota ativa já utiliza esse identificador |
| `persistence` | 1 | `not_committed` | a gravação foi tentada e não ocorreu |
| `store_unavailable` | 1 | `not_committed` | o próprio store não pôde ser lido |
| `indeterminate` | 1 | `unknown` | o resultado é desconhecido — consulte §8 |
| `read_failed` | 1 | `null` | uma nota ou o store não pôde ser lido |
| `internal_error` | 1 | `null` | a resposta em si não pôde ser produzida |

\* `usage_error` carrega `not_committed` quando emitido contra um comando de gravação, e `null` quando o comando era de leitura ou não pôde ser identificado.

`invalid_input` é o único código associado a dois códigos de saída, comportamento herdado e preservado: um seletor malformado fornecido a um comando de **gravação** sempre retornou `2`, enquanto o mesmo seletor fornecido a uma **leitura** sempre retornou `1`. A interface de máquina preserva ambos em vez de renumerar silenciosamente um contrato selado. Faça ramificações lógicas por `error.code`, não pelo código de saída, sempre que os dois puderem diferir.

Cada um desses erros, com exceção de `indeterminate`, garante sob o contrato atual que nada foi gravado em disco.

### Erros de parsing preservam o modo máquina

O modo máquina permanece ativo mesmo quando a lista de argumentos não puder ser lida pelo parser:

```bash
noteit --json batata                  # → usage_error em stderr, saída 2
noteit --json adicionar               # → usage_error em stderr, saída 2
noteit --json --flag-inexistente      # → usage_error em stderr, saída 2
noteit --json buscar                  # → usage_error em stderr, saída 2
```

Um consumidor que solicitou JSON nunca recebe uma mensagem em prosa em linguagem natural no lugar do envelope estruturado.

---

## 10. O que deliberadamente não está aqui

- **O protocolo privado de controle.** Identificadores de requisição, versão do protocolo, caminho do socket, lock de escritor, geração de janela e caminho de gravação executado são uma comunicação interna entre dois processos Note-it. Não fazem parte desta API e nunca são serializados nela, embora ambos utilizem JSON.
- **Caminhos do sistema de arquivos**, exceto no comando `status`, onde são o objetivo direto do comando.
- **Entrada em JSON.** `--json` descreve exclusivamente o formato de saída. Payloads continuam chegando como argumentos ou pela entrada padrão via `--stdin`, sem codificação adicional.
- **Pretty printing, NDJSON, processamento em lote, streaming, modo daemon, MCP.** Um comando, um documento.

---

## 11. Respondendo às perguntas essenciais, sem ler uma única frase

| pergunta | campo |
| --- | --- |
| o comando funcionou? | `status` |
| houve algo a relatar? | `warnings`, `status == "warning"` |
| algo foi efetivamente alterado? | `data.write.changed` |
| o commit ocorreu? | `data.write.commit_state == "committed"` |
| um commit era mesmo necessário? | `commit_state == "not_needed"` |
| o commit definitivamente não ocorreu? | `commit_state == "not_committed"` |
| o resultado do commit é desconhecido? | `status == "indeterminate"`, `commit_state == "unknown"` |
| qual nota foi afetada? | `data.write.note_id` (um UUID completo) |
| qual operação foi executada? | `command`, `data.write.kind` |
| a janela aberta está dessincronizada? | `data.write.ui_sync.status` |
| o que deu errado? | `error.code` |

Os campos `message` existem exclusivamente para seres humanos lendo logs de diagnóstico. Um consumidor automatizado que crie ramificações lógicas baseado neles interpretou incorretamente esta interface.
