# Medição da semântica de alias (Fase 6.0.A.2)

Evidência da **ADR-064**. Este arquivo registra *como* a medição foi feita e
*o que ela devolveu*. A decisão está na ADR; aqui só há método e número.

Cada afirmação é marcada. **MEDIDO** saiu de uma invocação real do binário.
**LIDO NO CÓDIGO** é um fato do fonte na baseline. **INFERIDO** é conclusão.
**DECIDIDO** é escolha desta fase. **NÃO MEDIDO** é o que fica de fora.

## Reprodução

```text
repositório   91a70d20db5bb186b096b8570744920f0122a335
corpus        docs/alias-corpus.json
sha256        77153bd790699d45b233bb70bebe25702830e37c64e5de18d0ec7949de4e76a8
casos         46 notas (45 vivas, 1 na lixeira)
binário       target/release/noteit, de `cargo build --locked --release -p noteit-cli`
versão        0.1.3
```

O procedimento não depende de nenhum script versionado:

1. Criar três stores isolados, um por opção — `XDG_DATA_HOME`,
   `XDG_CONFIG_HOME`, `XDG_STATE_HOME` e `XDG_RUNTIME_DIR` próprios, fora de
   `~/.local/share/note-it`.
2. Gravar cada nota do corpus como `notes/<id>.md` (ou `trash/<id>.md` quando
   `na_lixeira`), renderizando o campo de aliases na forma da opção sob teste.
3. Abrir cada nota com `noteit ler --json`.
4. Forçar uma gravação real com `noteit editar --stdin`, que passa por
   `NoteDocument::serialize()`, e reler os bytes do arquivo.
5. Medir a dobra semântica pelo próprio Core: cada grafia vira tag de uma nota
   descartável e `noteit listar --tag <grafia>` diz quais notas ela alcança.

### Isolamento, e por que ele é suficiente aqui

**LIDO NO CÓDIGO.** `scripts/note-it-isolated` existe porque a GUI é uma
`GApplication` de instância única: sem barramento privado, um comando é
encaminhado ao daemon real e escreve no store real. Essa armadilha é do
processo gráfico.

A medição desta fase usa **apenas o binário `noteit`**, que é headless.
`cargo tree --locked -p noteit-cli -e normal` não contém nenhuma ocorrência de
`zbus`, `dbus`, `gtk`, `gio` ou `glib`: não há cliente de barramento no grafo,
logo não existe caminho pelo qual o CLI encaminhe trabalho a outro processo. O
campo `ui_sync` da saída JSON não é IPC — `with_ui_sync_warning` só é chamado
por `src/write_authority.rs`, que é o adaptador de desktop; no CLI o valor é
sempre `in_step()`.

**MEDIDO.** Fingerprint dos dados reais antes e depois da fase, read-only:

```text
notes     b4a9c63bf2409d2f4b994abacdabc555   42 arquivos
trash     0fb5ccc858d474ffc983c7fc8195870c    2 arquivos
assets    df53d1eb553c8cefd356d2659afe7f17    1 arquivo
backups   936a2c1b80c772bdca81a5db0e52664f  260 arquivos
```

## 1. O estado atual, lido no código

Conferido em `91a70d2`, não herdado de relatório anterior:

| Fato | Onde | Estado |
| --- | --- | --- |
| `NoteProperty` tem valor `String` único | `metadata.rs:149` | `pub struct NoteProperty { pub key: String, pub value: String }` |
| `NoteProperties` é chaveado por identidade semântica | `metadata.rs:189` | `Vec<NoteProperty>`, indexado por `semantic_identity(key)` |
| Chave de property semanticamente duplicada é **erro** | `metadata.rs:199` | `"a nota não pode ter chaves de propriedade semanticamente duplicadas"` |
| Tag duplicada é **silenciosamente deduplicada** | `metadata.rs:84` | primeira grafia vence, sem erro |
| Limites | `metadata.rs:13-17` | `MAX_TAGS=32`, `MAX_TAG_CHARS=64`, `MAX_PROPERTIES=32`, `MAX_PROPERTY_KEY_CHARS=64`, `MAX_PROPERTY_VALUE_CHARS=512` |
| Normalização única | `metadata.rs:38` | `semantic_identity(v) = search::fold(v).text` |
| Topo desconhecido é preservado | `model.rs:92` | `#[serde(default, flatten)] unknown: BTreeMap<String, serde_yaml::Value>` |
| `note_it` **não** preserva chave desconhecida | `model.rs:40` | sem catch-all equivalente |
| Toda gravação passa por um ponto só | `storage.rs:371,401` | `save_note_atomic_with_id` → `doc.serialize()` |
| `backup.rs` não desserializa front matter | `backup.rs` | zero ocorrências de `NoteDocument::parse`/`NoteFrontMatter`; copia diretórios (`copy_directory`) |
| A chave de topo `aliases` está livre | `model.rs:85-93` | `NoteFrontMatterWrapper` tem exatamente três campos nomeados — `note_it`, `tags`, `properties` — e o `unknown` com `flatten`: qualquer outra chave de topo cai em `unknown` **por construção**, não por coincidência |

**INFERIDO.** Como `serialize()` remonta o wrapper a partir de
`unknown_front_matter`, qualquer chave de topo desconhecida é reescrita por
**todas** as superfícies de escrita, sem exceção — não há caminho de gravação
que a contorne.

## 2. As três opções, como ficam em disco

A mesma nota (`a1000004`, título "Acidente vascular cerebral", três aliases)
renderizada nas três formas avaliadas:

```yaml
# Opção A — properties com valor delimitado
properties:
  aliases: "AVC, Derrame, Acidente vascular encefálico"

# Opção B — properties com lista
properties:
  aliases:
    - AVC
    - Derrame
    - Acidente vascular encefálico

# Opção C — campo próprio de primeiro nível
aliases:
  - AVC
  - Derrame
  - Acidente vascular encefálico
```

## 3. MEDIDO — a nota abre?

45 notas vivas do corpus, lidas com `noteit ler --json`:

| Opção | Tentadas | Abrem | Falham | Não representáveis |
| --- | --- | --- | --- | --- |
| A — properties delimitada | 40 | 37 | **3** | 5 |
| B — properties com lista | 45 | 10 | **35** | 0 |
| C — campo de primeiro nível | 45 | **45** | **0** | 0 |

Os denominadores diferem de propósito e a coluna "tentadas" existe para que isso
não passe despercebido: as cinco "não representáveis" da opção A são as notas
cujo corpus define um formato de *lista* inválido, e uma String não tem como
carregá-lo — o caso não existe naquela forma e não foi tentado. As opções B e C
foram medidas sobre as 45 notas vivas inteiras.

As falhas, com a mensagem exata do binário:

```text
[B] qualquer nota com properties.aliases como lista
    "Failed to parse YAML front matter: properties.aliases:
     invalid type: sequence, expected a string"

[A] nota cujo usuário já tinha uma property chamada `aliases`
    "Failed to parse YAML front matter: properties: a nota não pode ter
     chaves de propriedade semanticamente duplicadas"

[A] alias contendo quebra de linha, e alias contendo caractere de controle
    "Failed to parse YAML front matter: properties: o valor da propriedade
     não pode conter quebras de linha ou caracteres de controle"
```

**INFERIDO.** As três falhas não perdem só os aliases: elas tornam **a nota
inteira ilegível**, porque a desserialização do front matter falha e
`NoteDocument::parse` devolve `Err`. Sob a opção A, um alias com um caractere
inválido custa o acesso ao texto da nota. Sob a opção B, isso vale para
praticamente toda nota que tenha aliases.

## 4. MEDIDO — downgrade

Uma gravação real (`noteit editar`) feita pelo binário 0.1.3, que **não conhece
nem `title` nem `aliases`**, sobre a nota de três aliases:

| Opção | Chave sobrevive | Os três nomes | `title` |
| --- | --- | --- | --- |
| A | presente | preservados | preservado |
| B | — | **a gravação falha**: `properties.aliases: invalid type: sequence` | — |
| C | presente | preservados | preservado |

O front matter da opção C depois da gravação, byte a byte do arquivo:

```yaml
note_it:
  version: 1
  id: a1000004-a11a-4000-8000-000000000000
  color: yellow
  paper_type: blank
  paper_intensity: normal
  font_size: 15
  created_at: 2026-09-01T09:00:00Z
  updated_at: 2026-09-15T10:43:11.304974168Z
aliases:
- AVC
- Derrame
- Acidente vascular encefálico
title: Acidente vascular cerebral
```

**MEDIDO.** A ordem dos itens da lista é preservada. A ordem das *chaves* de
topo é alfabética, porque `unknown` é um `BTreeMap` — normalização de
formatação que `docs/markdown-format.md` já documenta e que não é regressão
desta decisão.

**Atenção à leitura deste quadro:** `updated_at` se moveu porque o experimento
foi uma edição de **corpo**. Isso não diz nada sobre o efeito de alterar um
alias, que é uma gravação diferente e está tratado na ADR.

## 5. MEDIDO — a vírgula

O teste que separa a opção A das demais. Nota `a100000b`, cujos aliases são:

```text
["Choque, abordagem inicial", "Estado de choque"]
```

Sob a opção A, o binário devolveu a property:

```text
properties["aliases"] = "Choque, abordagem inicial, Estado de choque"
```

e a divisão por vírgula produziu:

```text
["Choque", "abordagem inicial", "Estado de choque"]
```

**Três itens, não dois. Reconstrói os originais? NÃO.**

Sob a opção C o arquivo contém dois escalares YAML independentes:

```yaml
aliases:
  - "Choque, abordagem inicial"
  - "Estado de choque"
```

A vírgula é conteúdo, não separador, e nenhuma gramática adicional é necessária.

## 6. MEDIDO — a opção C diante de YAML estranho

Nove formas malformadas do campo, sob a opção C. Todas foram gravadas, lidas e
regravadas pelo binário real:

| Campo | A nota abre | Sobrevive à gravação |
| --- | --- | --- |
| lista com item vazio | sim | preservado |
| lista com item só de espaços | sim | preservado |
| item com quebra de linha | sim | preservado |
| item com caractere de controle | sim | preservado |
| lista mista (texto + número) | sim | preservado |
| mapa no lugar de lista | sim | preservado |
| scalar no lugar de lista | sim | preservado |
| número no lugar de lista | sim | preservado |
| item acima de 512 caracteres | sim | preservado |

**INFERIDO.** Enquanto o campo é desconhecido, ele é um `serde_yaml::Value`
opaco: nada o valida e nada o descarta. É exatamente a assimetria que a
ADR-063 fixou para o `title` — na leitura o valor degrada, nunca derruba a nota.

## 7. MEDIDO — `properties.aliases` ao lado do campo canônico

Nota `a100003f`, que tem as duas coisas ao mesmo tempo. Sob a opção C ela abre
normalmente, e o binário devolve:

```text
properties = [{"key": "aliases", "value": "Concorrente, Outro"}]
```

com o arquivo em disco carregando os dois campos em níveis diferentes:

```yaml
title: "Concorrência"
properties:
  "aliases": "Concorrente, Outro"
aliases:
  - "Canônico"
```

**INFERIDO.** Os dois não competem porque não estão no mesmo nível: um é uma
property comum chamada `aliases`, o outro é o campo de topo. Sob a opção A a
mesma nota **não abre** — as duas chaves caem no mesmo mapa e a duplicata
semântica é fatal.

## 8. MEDIDO — notas antigas

| Nota | A | B | C |
| --- | --- | --- | --- |
| front matter mínimo (só `note_it.id`) | abre | abre | abre |
| sem front matter nenhum | abre | abre | abre |

Nenhuma das três opções exige migração para que uma nota antiga continue
abrindo — a ausência do campo já é um estado válido hoje.

## 9. MEDIDO — a lixeira

Sob a opção C, com duas notas carregando o mesmo `title` e o mesmo alias, uma
viva e uma na lixeira:

```text
nota viva  (a1000032) aparece em `noteit listar`   sim
nota morta (a1000033) aparece em `noteit listar`   não
`noteit lixeira` contém                            a1000033
```

**INFERIDO.** O espaço de nomes já é o das notas vivas por construção da
listagem, sem que nada precise ser acrescentado.

## 10. MEDIDO — a dobra semântica

Pelo filtro real do Core, sobre grafias retiradas do corpus:

```text
mesma identidade:  "Sinônimo"  "sinonimo"  "SINÔNIMO"
mesma identidade:  "Neonatologia" em NFC   "Neonatologia" em NFD
mesma identidade:  "   Traumatologia   "   "Traumatologia"
mesma identidade:  "Präoperativ"           "praoperativ"
identidade só sua: "Terapia intensiva"
identidade só sua: "Terapia  intensiva"    (espaço interno dobrado)
identidade só sua: "血圧"
```

**INFERIDO.** Caixa, acento e forma Unicode dobram; o recorte externo acontece
antes da identidade; espaço interno **não** é colapsado; um script fora da
tabela latina atravessa intacto e é sua própria identidade. É o mesmo contrato
que a 6.0.A registrou para `title`, reconfirmado aqui sobre dados de alias.

## 11. MEDIDO — o que o produto já faz com lista estrangeira

Duas sondas feitas depois da primeira revisão adversarial, porque duas regras da
ADR dependiam de saber o comportamento atual em vez de supô-lo.

**Cardinalidade acima do teto, hoje, com `tags`:**

```text
32 tags  -> a nota ABRE, o Core devolve 32
33 tags  -> a nota NÃO ABRE
40 tags  -> a nota NÃO ABRE
```

**INFERIDO.** O teto de `MAX_TAGS` é aplicado na desserialização e transforma
"nomes demais" em "nota ilegível". É a falha que esta família de ADRs recusa, e
é a razão pela qual o teto de aliases é declarado como teto de **gravação**.

**Duplicatas estrangeiras, hoje, com `tags`:**

```text
arquivo antes:   - "Sinônimo"   - "sinonimo"   - "SINÔNIMO"
o Core lê:       ["Sinônimo"]
depois de uma edição de CORPO, o arquivo contém:   - Sinônimo
```

**MEDIDO.** Uma edição que não tocou nas tags apagou duas linhas do front matter.
`NoteTags::try_new` roda na desserialização e `serialize()` reescreve o campo
inteiro a cada gravação.

**O mesmo caso, com uma chave desconhecida de topo — que é o que `aliases` é
hoje:**

```text
43 aliases, três deles grafias do mesmo nome -> a nota ABRE
depois de uma edição de corpo: 43 itens preservados, duplicatas incluídas
```

**INFERIDO.** Enquanto o campo é opaco, a lista estrangeira atravessa intacta.
A ADR precisa dizer se esse comportamento continua depois de o campo passar a ser
conhecido — e ela diz: continua, salvo numa edição de aliases pedida pelo
usuário.

## 12. O que esta medição NÃO mediu

- **NÃO MEDIDO: nenhuma resolução.** Não existe resolvedor, e esta fase não o
  escreve. Toda a seção de colisões da ADR é **DECIDIDO**, não medido: ela
  descreve o contrato que a 6.A.3 implementará.
- **NÃO MEDIDO: nenhum alias real.** O campo não existe no modelo. Tudo acima
  mede como o binário atual trata uma chave que ele **não conhece**, que é o
  que `aliases` será para toda versão anterior a ele.
- **NÃO MEDIDO: convenções de outras ferramentas.** A escolha de `aliases` como
  nome de chave de topo coincide com a convenção difundida no ecossistema
  Markdown, mas isso não foi verificado contra documentação oficial nesta fase
  e **não é premissa da decisão** — a evidência que decide é a das seções 3 a 7.
- **NÃO MEDIDO: desempenho.** A 6.0.A.2 decide semântica.
