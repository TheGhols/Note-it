# Medição da identidade nomeável da nota (Fase 6.0.A)

Evidência da **ADR-063**. Este arquivo registra *como* a medição foi feita e
*o que ela devolveu*. A decisão está na ADR; aqui só há método e número.

Nada abaixo foi estimado, lembrado ou reimplementado. Todo rótulo veio de
`noteit listar --json` e toda dobra semântica veio de um filtro real do Core.
O que é **MEDIDO** e o que é **INFERIDO** está separado em cada seção.

## Reprodução

```text
repositório   328698be54700029104531342d6cffa95ee4b3e1
corpus        docs/naming-corpus.json
sha256        5a9b0799d00fb838194379edcd4d90af542e5237e96ad8ff66c8a696883de18f
binário       target/release/noteit, de `cargo build --locked --release -p noteit-cli`
versão        0.1.3
```

O procedimento, que não depende de nenhum script versionado:

1. Criar um store isolado — `XDG_DATA_HOME`, `XDG_CONFIG_HOME`,
   `XDG_STATE_HOME` e `XDG_RUNTIME_DIR` próprios, fora de
   `~/.local/share/note-it`. O CLI é headless: não precisa de barramento nem
   da instância gráfica.
2. Gravar cada nota de `notas` como `notes/<id>.md`, montando o front matter
   conforme o campo `front_matter` da nota (a tabela está no próprio corpus).
3. Ler os rótulos com `noteit listar --json --limite 100`; o rótulo é o campo
   `label`, que o Core produz por `search::label_for`.
4. Para os cenários de edição, copiar o store e aplicar cada corpo alternativo
   com `noteit editar <id> --stdin` — uma gravação real, pelo caminho real.
5. Para medir `semantic_identity` sem reimplementá-la, usar o próprio Core:
   uma grafia vira tag de uma nota descartável e `noteit listar --tag <grafia>`
   diz quais notas aquela grafia alcança. Duas grafias que alcançam a mesma
   nota têm, para o Note-it, a mesma identidade. Para textos acima do teto de
   64 caracteres de uma tag, a mesma pergunta é feita por valor de propriedade,
   cujo teto é 512 e cuja comparação é a mesma `semantic_identity`.

## 1. O estado atual, confirmado no código

Não aceito do roadmap; lido em `328698be`:

| Afirmação | Onde | Confirmado |
| --- | --- | --- |
| A nota tem `id: Uuid` e nenhum título | `noteit-core/src/model.rs:40-61` | sim — `NoteFrontMatter` tem `version`, `id`, `color`, `paper_type`, `paper_intensity`, `font_size`, `created_at`, `updated_at` |
| O nome legível é derivado do conteúdo | `noteit-core/src/search.rs:199-208` | sim — `label_for(content) = label_of_visible(visible_text(content))` |
| A regra é "primeira linha visível não vazia" | `search.rs:204-208` | sim — primeira linha não vazia da projeção, recortada, truncada em `MAX_LABEL_CHARS = 120` com `…` |
| Nota sem nada visível tem rótulo fixo | `search.rs:41` | sim — `EMPTY_LABEL = "Nota vazia"` |
| A normalização existente é uma só | `metadata.rs:38-40` | sim — `semantic_identity(v) = search::fold(v).text`: minúscula Unicode + dobra de diacrítico latino, marcas combinantes U+0300–U+036F descartadas |
| `properties` é chave → **String única** | `metadata.rs:188-189` | sim — `NoteProperties(Vec<NoteProperty>)`, `NoteProperty { key: String, value: String }` |
| YAML de terceiro é preservado | `model.rs:88-92` | sim **no topo** — `NoteFrontMatterWrapper` tem `#[serde(default, flatten)] unknown`. `NoteFrontMatter` **não tem** equivalente: uma chave desconhecida dentro de `note_it:` não tem onde ser guardada |

**MEDIDO — identidade canônica.** O nome do arquivo manda. Um front matter cujo
`id` discorda do `<uuid>.md` faz a leitura **falhar**, em vez de redirecionar:

```text
rc=1  read_failed
"conflito de identidade da nota: o arquivo `08672db6-….md` possui front
 matter com id `6a0000ff-…`"
```

E uma nota sem front matter nenhum continua sendo uma nota: `parse_with_id`
ancora metadados padrão no UUID do arquivo (`model.rs:241-258`), sem inventar
carimbos. **INFERIDO:** a identidade canônica de hoje já é o UUID do arquivo,
verificado contra a cópia redundante do front matter — não é uma escolha nova
que a 6.0.A precise fazer, é um contrato existente que ela precisa declarar.

## 2. O corpus

`docs/naming-corpus.json` — 36 notas sintéticas, nenhuma linha vinda de nota
real. Cobre as classes exigidas: primeira linha única; primeira linha idêntica;
grafias que dobram para a mesma identidade; caixa; acento; Unicode composto e
decomposto; nota vazia; nota só com espaços; nota sem texto visível; forma
visível divergente da fonte; nome muito longo; colisão fabricada pelo
truncamento; nomes parecidos e distintos; property candidata a nome; YAML de
terceiro; nota sem front matter; nota com front matter mínimo; UUIDs distintos
com conteúdo idêntico; primeira linha dentro de bloco de código; comentário
antes do título; nome que é um UUID; nome só com pontuação; espaçamento interno.

Doze notas carregam um corpo alternativo **E1** (reescreve a primeira linha
visível, preserva o resto) e dez carregam **E2** (preserva a primeira linha
visível — ou, nos casos marcados, a primeira linha da *fonte* — e mexe no resto).

## 3. MEDIDO — o rótulo derivado sobre o store base

```text
total_de_notas                          36
rotulos_distintos_exatos                31
nomes_distintos_apos_semantic_identity  28
notas_em_colisao_exata                   9   (25%)
notas_em_colisao_normalizada            14   (39%)
notas_sem_nome_humano                    3   (rótulo == "Nota vazia")
notas_com_nome_unico_e_util             22   (61%)
```

Colisões exatas:

```text
2x  "Checklist de alta"
2x  "Consulta de retorno"
3x  "Nota vazia"
2x  "Protocolo institucional de abordagem inicial do paciente com dor
     torácica aguda no pronto-socorro, versão revisada pela …"
```

Grupos que só colidem depois da dobra do Core:

```text
["HIPERTENSÃO", "Hipertensão", "hipertensao"]
["Pré-operatório" (NFC), "Pré-operatório" (NFD)]
```

A quarta colisão exata merece nome próprio: **o truncamento fabrica colisão**.
As notas `6a000010` e `6a000011` têm primeiras linhas *diferentes* — uma diz
"revisada pela comissão de emergência em setembro", a outra "revisada pela
diretoria clínica em dezembro" — e compartilham os primeiros 120 caracteres.
O rótulo corta em 120 e as duas passam a ter o mesmo nome.

## 4. MEDIDO — mutabilidade do rótulo

### Cenário E1 — a primeira linha visível é reescrita

```text
notas_editadas             12
rotulos_que_mudaram        11
rotulos_que_permaneceram    1
```

A única que permaneceu é `6a000010`, a nota de primeira linha longa: a edição
acrescentou " (revisão 2)" **depois** do caractere 120, e o rótulo truncado não
registrou a mudança. **INFERIDO:** o truncamento não é estabilidade — é a mesma
cegueira que fabricou a colisão do item anterior, vista do outro lado.

### Cenário E2 — a edição não toca a primeira linha

```text
notas_editadas             10
rotulos_que_mudaram         4
rotulos_que_permaneceram    6
```

Os quatro que mudaram são o achado:

```text
6a00000a  "Nota vazia"                      -> "Comprar filtro de café e pilhas."
6a00000b  "Nota vazia"                      -> "Rascunho que virou texto."
6a00000e  "Dose máxima excedida"            -> "Dose máxima excedida em 20%"
6a00001f  "def dose_por_peso(mg_kg, peso):" -> "def dose_por_kg(mg_kg, peso):"
```

Em `6a00000e` a primeira linha do arquivo é `> [!WARNING]`, que é marcador e
não texto; em `6a00001f` é ```` ```python ````, que é cerca e não texto. As duas
edições preservaram a primeira linha da fonte **byte a byte** e ainda assim
mudaram o nome derivado.

E o inverso, na mesma rodada: `6a000020` começa com
`<!-- rascunho, revisar com a preceptora -->` e tem `# Fibrilação atrial` na
linha três. A edição E2 reescreveu esse título para "Fibrilação atrial de
início recente" — e o rótulo **não mudou**, porque continua sendo o texto do
comentário.

**INFERIDO:** "editar a primeira linha" não é uma regra que um usuário consiga
prever. Em três notas do corpus a linha que vira nome não é a linha que o
usuário chamaria de título, nos dois sentidos: ele muda o que julga ser o
título e o nome não muda; ele mexe em outro lugar e o nome muda.

## 5. MEDIDO — o contrato de normalização

Pelo filtro real do Core, por duas vias, com resultado idêntico nas duas.

A via da tag não serve para todo nome: `MAX_TAG_CHARS` é 64 (`metadata.rs:14`),
então o rótulo truncado de 120 caracteres é **recusado** como tag e não pode ser
medido por ali. Esse caso foi medido pela segunda via — valor de propriedade,
cujo teto é 512 (`MAX_PROPERTY_VALUE_CHARS`) e cuja comparação passa pela mesma
`semantic_identity`. Quem repetir a medição precisa das duas vias para cobrir a
tabela inteira:

| Entrada | Alcança a mesma nota? |
| --- | --- |
| `Hipertensão` | sim (referência) |
| `hipertensão` | **sim** — caixa dobra |
| `HIPERTENSÃO` | **sim** — caixa dobra |
| `Hipertensao` | **sim** — acento dobra |
| `Hipertensão` em NFD | **sim** — decomposto dobra igual ao composto |
| `␣␣Hipertensão␣␣` | **sim** — o recorte acontece na validação, antes da dobra |
| `Hiper tensão` | **não** — espaço interno é significativo |
| `""` | **recusado pelo Core**, não é uma identidade vazia |
| `…␣␣inicial␣␣…` (espaço interno dobrado) | **não** — espaço interno não é colapsado |

**INFERIDO:** `semantic_identity` cobre caixa, acento e forma Unicode, que são
exatamente os eixos em que um humano digita o mesmo nome de outro jeito. Ela
**não** colapsa espaço interno, então `Consulta de retorno` e
`Consulta␣␣␣de␣␣␣retorno` são nomes diferentes — o corpus tem esse par
(`6a000003`/`6a000004` contra `6a00001d`). Isso vale igualmente para tags
hoje, e é registrado aqui como limitação conhecida, não corrigida nesta fase.

## 6. MEDIDO — onde um campo de nome sobrevive

Esta é a medição que decidiu **onde** a identidade nomeável mora, e ela foi
feita depois que a revisão adversarial derrubou o argumento da primeira versão
da ADR-063. A pergunta: um campo de nome escrito por uma versão futura do
Note-it sobrevive a uma gravação feita por um binário que não conhece o campo?

Procedimento, reproduzível sem nenhum script: gravar uma nota cujo front matter
carrega o campo na posição testada, mais uma chave de terceiro para controle;
abrir com `noteit ler`; regravar com `noteit editar`; reler o arquivo.

```yaml
---
note_it:
  version: 1
  id: "7b000000-7b0b-4000-8000-000000000000"
  color: "yellow"
  paper_type: "blank"
  paper_intensity: "normal"
  font_size: 15
title: "Hipertensão arterial"     # <- a posição sob teste
other_tool: keep-me               # <- controle
---

# Corpo da nota

texto
```

Resultado, com o binário 0.1.3:

| Campo testado | A nota abre? | Sobrevive a uma gravação real? |
| --- | --- | --- |
| `title: "Hipertensão arterial"` no topo | sim | **PRESERVADO** |
| `title: 42` no topo | sim | **PRESERVADO** |
| `title: [um, dois]` no topo | sim | **PRESERVADO** |
| `title: {pt: …, en: …}` no topo | sim | **PRESERVADO** |
| `title: ""` no topo | sim | **PRESERVADO** |
| `title:` (nulo) no topo | sim | **PRESERVADO** |
| `note_it.title: "Hipertensão arterial"` | sim | **PERDIDO** |

A chave de controle `other_tool` sobreviveu em todos os sete casos.

**INFERIDO:** a diferença é estrutural e está no modelo.
`NoteFrontMatterWrapper` tem `#[serde(default, flatten)] unknown`
(`model.rs:92`), então qualquer chave de topo que o binário não conheça é
guardada e reescrita intacta. `NoteFrontMatter` não tem equivalente
(`model.rs:40`), então uma chave desconhecida dentro de `note_it` não tem onde
ser guardada e desaparece na reserialização. Consequência prática: um nome no
topo é seguro em downgrade **hoje**, sem nenhuma mudança de código; um nome
dentro do bloco reservado exigiria acrescentar essa preservação antes de
qualquer versão gravá-lo.

Nenhuma das sete linhas acima é um `title` de verdade, porque o campo não
existe: todas medem como o binário atual trata uma chave que ele **não conhece**
em cada posição — que é exatamente o que `title` será para toda versão anterior
a ele.

## 7. MEDIDO — o espaço de `properties` como portador de nome

```text
properties sobrevivem a uma edição de corpo:
  [E1] 6a000014 -> [{"key":"fonte","value":"comissão"},
                     {"key":"name","value":"Protocolo de sepse"}]

dois `name` distintos que colidem após a dobra do Core:
  ["Protocolo de sepse", "protocolo de SEPSE"]

ciclo de vida de `name`, como o usuário o vê:
  criar  name=Nome declarado   -> [{"key":"name","value":"Nome declarado"}]
  remover name                 -> rc=0, []            (o usuário apaga o nome)
  definir name=                -> rc=0, value=""      (nome vazio é aceito)
  definir name=␣␣␣             -> rc=0, value=""      (só espaços vira vazio)
  definir Name=Outro           -> rc=0, value="Outro" (substitui, sem avisar)
  definir name=Primeiro, Segundo -> rc=0, value="Primeiro, Segundo"
```

**INFERIDO:** a última linha é o teto da Opção C para aliases — o valor é uma
String só, então "Primeiro, Segundo" é **um** nome que contém vírgula, não dois
nomes. E a penúltima mostra que a chave pertence ao usuário: ele remove, esvazia
e sobrescreve à vontade, porque `properties` é, por contrato documentado em
`docs/markdown-format.md`, metadado de autoria dele.

## 8. O que esta medição não mediu

Dito explicitamente, para que ninguém a leia como mais do que é:

- **nenhum resolvedor foi medido**, porque nenhum existe. Tudo acima mede o
  rótulo derivado atual, a normalização atual e o que o formato de arquivo
  atual preserva;
- **nenhum `title` real foi medido**, porque o campo não existe. A seção 6 mede
  o tratamento de uma chave desconhecida em cada posição do front matter, que é
  o que `title` será para um binário anterior a ele;
- **nenhuma colisão de nome declarado foi medida.** Os 25% e 39% da seção 3 são
  colisão de **rótulo derivado**, e o rótulo derivado é justamente o que a
  decisão descarta como fonte de nome. Esses números eliminam a Opção A e não
  dizem nada sobre com que frequência dois `title` declarados colidiriam — o
  corpus não tem como dizer, porque nenhuma nota pode ter um;
- **nenhum dado real foi tocado.** O corpus é sintético, os stores são
  descartáveis e ficaram fora de `~/.local/share/note-it`;
- **nada de desempenho foi medido.** A 6.0.A decide semântica.
