# Recuperação semântica — especificação

Decidido na Fase 4.3A, corrigido nas R1, R1.1 e R1.2, e **parcialmente
implementado na 4.3B**. Este documento é a especificação que as subfases de
implementação consomem, e a régua contra a qual elas são medidas. Justificativa e
medições nas ADR-056 e ADR-057.

O corpus de avaliação está em [`retrieval-corpus.json`](retrieval-corpus.json), e
a posição, consulta por consulta, do motor de antes do BM25 está congelada em
[`retrieval-baseline.json`](retrieval-baseline.json).

## Estado: o que existe em código, e o que ainda é especificação

A distinção importa, porque um documento que descreve tudo no presente vira uma
promessa que ninguém fez. Em 2026-09-05:

| | estado | onde |
| --- | --- | --- |
| recuperação por termo, BM25 `k1=1.2` `b=0.75` | **implementado, em produção** | `noteit-core/src/lexical.rs`, `context.rs` |
| `Reason::TermMatch`, publicado como `term_match` | **implementado, em produção** | `context.rs`, `noteit-mcp/src/contract.rs` |
| classes de precedência (§13) | **implementado** | `context.rs` |
| `EmbeddingRole`, `EmbeddingSpaceId`, `ArtifactManifestV1` (§5, §5.1) | **implementado** | `noteit-core/src/embedding.rs` |
| chunker versionado e `ChunkId` (§14) | **implementado** | `noteit-core/src/chunking.rs` |
| `EmbeddingProvider`, `EmbeddingRecord`, índice em memória (§4, §6, §15) | **implementado** | `noteit-core/src/semantic.rs` |
| validação de proveniência e invalidação (§7) | **implementado** | `context.rs` |
| `Reason::SemanticMatch`, publicado como `semantic_match` | **implementado no motor, inalcançável pelo produto** | — |
| provider local, modelo, artefato distribuído | **especificação** | 4.3C |
| providers remotos, credenciais, `noteit-embed` (§8, §9, §10) | **especificação** | 4.3D |
| cache em disco (§15.1), configuração de usuário (§12) | **especificação** | 4.3C/4.3D |

"Inalcançável pelo produto" é literal e estrutural: o modo de recuperação que o
`noteit_context` usa é uma variante de enum **sem campo onde um provider caiba**.
Não há configuração ausente e não há flag desligada — não existe o que ligar. O
motivo está no esquema porque um motivo que o Core sabe produzir e o esquema não
declara é um contrato de fio que mente.

O que o Note-it está construindo não é uma IA local, nem um cliente da OpenAI,
nem um cliente do Gemini. É **uma memória semântica independente de fornecedor,
cuja fonte da verdade são as notas do usuário e cuja recuperação usa o mecanismo
de embeddings que o usuário escolher**. A IA que raciocina pode mudar. O provider
de embeddings pode mudar. O modelo pode mudar. O índice pode ser apagado e
reconstruído. As notas continuam sendo as notas.

---

## 1. A cadeia, e a direção em que ela corre

```text
NOTA  (arquivo Markdown — a única fonte da verdade)
  │
  ├─ estado canônico: note_id + revision
  │
  ▼
CHUNKER  (determinístico, versionado — só lê)
  │
  ▼
CHUNKS   chunk_id = note_id + revision + ordinal + hash do texto do chunk
  │
  ▼
EmbeddingProvider   local  │  remoto (opt-in, fora do processo)
  │
  ▼
EMBEDDING  + EmbeddingSpaceId (provider, modelo, artefato, dimensão, receita)
  │
  ▼
ÍNDICE DERIVADO  (cache; perder não perde nota nenhuma)
  │
  ▼
CANDIDATO PRELIMINAR   note_id + o canal que o admitiu; nada publicável ainda
  │
  ▼
UMA LEITURA AUTORITATIVA DO NoteDocument   ← a que o motor já faz por candidato
  │
  ▼
NoteRevision::for_document(&documento)     ← a revisão atual só nasce daqui
  │
  ▼
source_revision == revisão atual ?
  │
  ├── não → DESCARTAR o candidato e marcar para reindexação
  │
  └── sim
        │
        ▼
   snippet, motivos e tarefas — TODOS desta mesma leitura, nunca do cache
        │
        ▼
   Context Engine → MCP / CLI → o agente
```

**A leitura vem antes da validação, e não depois.** É a única ordem possível: a
revisão canônica atual é `sha256` do `NoteDocument` serializado, então não há o
que comparar antes de carregá-lo. A 4.3A desenhava o contrário e a 4.3A.R1.1
corrigiu — esta é a **única** ordem oficial, e §7 a repete com a evidência de
código.

**A cadeia nunca se inverte.** O vetor serve para *encontrar* a nota. O vetor não
substitui a nota. Um candidato é sempre um `note_id` que se volta a ler pelos
mecanismos normais do Core, e um vetor órfão — sem nota correspondente — não é
resultado.

Em qualquer conflito entre nota e derivado, **a nota vence**.

## 2. O problema, medido

O Context Engine de hoje casa **a consulta inteira como substring** do texto
dobrado. Não há casamento por termo, não há pontuação e não há ranking além de
(mais motivos → recência → `note_id`).

Medido contra o binário real, por stdio, sobre store sintético:

```text
19 das 30 consultas com resposta voltam VAZIAS
R@1 0,333   R@3 0,367   R@5 0,367   MRR 0,350
```

"hipertensão arterial" não acha a nota sobre pressão alta. Nenhuma dessas falhas
é por falta de semântica.

| motor | R@1 | R@3 | R@5 | MRR | custo |
| --- | --- | --- | --- | --- | --- |
| lexical de hoje | 0,333 | 0,367 | 0,367 | 0,350 | — |
| BM25 por termos | 0,667 | 0,767 | 0,833 | 0,728 | nenhuma dependência |
| BM25 → semântico local | 0,767 | 0,900 | 0,967 | 0,845 | artefato de modelo |

O passo lexical entrega **+0,40 de R@3 sem modelo, sem cache, sem artefato e sem
superfície de privacidade nova**. O semântico acrescenta mais +0,13. Os dois se
justificam; a ordem passou a ser decidida por número, e é por isso que o lexical
é a primeira coisa a ser implementada — ele também é o piso para onde tudo
degrada.

## 3. Objetivos e não objetivos

**Objetivos.** Encontrar a nota certa mesmo quando a consulta não usa as palavras
dela; nunca perder nem rebaixar um acerto exato de hoje; manter cada candidato
explicável; funcionar por padrão sem enviar nada para lugar nenhum; e continuar
inteiramente utilizável sem nada disto.

**Não objetivos.** Não é uma IA que resume ou conclui — a IA continua fora do
Core (ADR-048). Não é um banco vetorial. Não é um serviço. Não substitui
`noteit_search` nem a paleta `Ctrl+K`. Não muda o `.md`, o front matter, a
revisão nem o protocolo de escrita. E **não é um cliente de uma nuvem
específica**: nenhum provider remoto pode virar requisito.

## 4. `EmbeddingProvider`

Uma interface, e nenhuma lógica de fornecedor espalhada pelo chunker, pelo
índice, pelo ranking ou pelo Context Engine.

```text
EmbeddingProvider
    identidade      -> EmbeddingSpaceId
    embed_document(textos) -> vetores        (lote)
    embed_query(texto)     -> vetor
    limites         -> tamanho de lote, tokens por chamada, dimensões aceitas
```

`embed_document` e `embed_query` são **funções distintas** e não uma só. Não é
simetria estética: `intfloat/multilingual-e5-*` exige os prefixos `passage: ` e
`query: `; o Voyage prepende instruções diferentes conforme `input_type`; o
Gemini 001 tem `RETRIEVAL_DOCUMENT` e `RETRIEVAL_QUERY`. Uma função única
obrigaria cada chamador a saber disso — que é exatamente a lógica de fornecedor
que a interface existe para conter.

```text
EmbeddingProvider
    ├── LocalProvider          embeddings estáticos, em processo
    ├── OpenAIProvider         ┐
    ├── GeminiProvider         ├─ remotos, fora do processo, opt-in
    ├── VoyageProvider         ┘
    └── (futuros)
```

Não existe `AnthropicProvider`: em 2026-09-04 a documentação oficial diz, com
todas as letras, *"Anthropic does not offer its own embedding model"*, e aponta a
Voyage AI. Registrar um provider que não existe seria inventar API.

## 5. `EmbeddingSpaceId`

Responde uma pergunta e só ela: **estes dois vetores podem ser comparados?**

```text
EmbeddingSpaceId {
    provider            "local" | "openai" | "gemini" | "voyage" | …
    model               identificador do modelo, como o provider o nomeia
    artifact_identity   §5.1 — o que prova que os pesos são os mesmos pesos
    dimension           a dimensão efetivamente usada, não a máxima do modelo
    embedding_recipe    versão do PAR de receitas (documento e consulta juntas)
    normalization       versão da normalização de texto do Note-it
}

EmbeddingRole { Document, Query }        ← papel da entrada, NÃO parte do espaço
```

Dois vetores só entram na mesma busca se os `EmbeddingSpaceId` forem iguais.
Não "compatíveis o suficiente": iguais.

**O papel não entra na identidade do espaço, e essa é a correção da 4.3A.R1.**
A 4.3A punha `task = document/query` dentro do `EmbeddingSpaceId` e ao mesmo
tempo exigia igualdade exata para comparar — o que se contradizia, porque uma
busca compara justamente o vetor de uma consulta com os vetores de documentos.
Sob aquela regra, nenhuma busca seria válida.

A separação correta tem três coisas em vez de duas:

| conceito | o que é | onde vive |
| --- | --- | --- |
| espaço | onde os vetores são comparáveis | `EmbeddingSpaceId` |
| papel | esta entrada é documento ou consulta | `EmbeddingRole` |
| receita | como cada papel é preparado antes de embutir | `embedding_recipe`, uma versão para **o par** |

`embed_document` e `embed_query` **podem** preparar a entrada de forma diferente
— e devem, porque o `e5` exige `passage: ` e `query: `, a Voyage prepende
instruções distintas por `input_type` e o `gemini-embedding-001` tem
`RETRIEVAL_DOCUMENT` e `RETRIEVAL_QUERY`. É exatamente **para** produzirem
vetores comparáveis que as receitas diferem: elas são as duas metades de um
mesmo protocolo de recuperação. Por isso a versão é do par, e não de cada
metade: mudar só a receita de consulta invalida a comparação com documentos já
indexados tanto quanto mudar a de documento.

Um provider é obrigado a declarar **o mesmo `EmbeddingSpaceId`** em
`embed_document` e `embed_query` sempre que seus vetores forem validamente
comparáveis. Um provider que não puder fazer isso não é um provider deste
sistema.

### Asserções de contrato que a implementação deve trazer

1. `embed_document(x).space == embed_query(y).space` para o mesmo provider,
   modelo e receita — e a busca entre eles é permitida.
2. Espaços realmente diferentes — outro provider, outro modelo, outra receita,
   outra identidade de artefato — são **recusados** na comparação, e o erro é
   `DimensionMismatch` ou `SpaceMismatch`, nunca um número.
3. **Igualdade de dimensão sozinha nunca basta.** A regressão é a medição da
   4.3A: dois espaços truncados para a mesma dimensão produzem cosseno
   perfeitamente calculável e R@3 de 0,133 contra 0,933. O teste deve falhar se
   alguma versão futura aceitar a comparação só porque as formas batem.

**Dimensão igual não é compatibilidade**, e isso foi medido em vez de suposto.
Truncando os vetores de um modelo para a dimensão de outro — o que produz números
perfeitamente calculáveis:

| busca | R@1 | R@3 | R@5 | MRR |
| --- | --- | --- | --- | --- |
| mesmo espaço (consulta e documentos do mesmo modelo) | 0,700 | 0,933 | 0,967 | 0,812 |
| espaços cruzados (consulta de um modelo, documentos de outro) | 0,033 | 0,133 | 0,133 | 0,094 |

O ranking colapsa e **nada no cálculo avisa**. É por isso que a identidade nomeia
provider e modelo, e não apenas a dimensão.

Mesmo provider com modelo novo é espaço novo até prova explícita em contrário.
`model-v1` e `model-v2` não são o mesmo espaço porque o fornecedor é o mesmo.

### 5.1 Identidade do artefato e da receita

O nome do modelo não basta. A classe de defeito que a 4.3A mediu — números
válidos, ranking inválido, nenhum erro estrutural — reaparece inteira se os pesos,
o tokenizer, a normalização ou a receita mudarem mantendo o mesmo nome e a mesma
dimensão. O `EmbeddingSpaceId` tem de tornar isso impossível de passar em
silêncio.

**Provider local — identidade verificável, e é obrigatória.**

A identidade é o hash de um **manifesto**, e não de componentes concatenados:

```text
ArtifactManifestV1 {
    weights_sha256              hex, 64 caracteres
    tokenizer_sha256            hex, 64 caracteres
    embedding_recipe_version    inteiro
    normalization_version       inteiro
}

artifact_identity = sha256( "noteit.artifact.v1\n" ‖ canonical_json(ArtifactManifestV1) )
```

onde `canonical_json` é JSON com chaves em ordem lexicográfica, sem espaços
supérfluos e em UTF-8 — a mesma disciplina de "uma grafia canônica" que o
`NoteDocument` já segue para que uma nota comparada consigo mesma não pareça
diferente.

**Por que um manifesto e não `sha256(a ‖ b ‖ c ‖ d)`.** A concatenação simples de
componentes de comprimento variável é ambígua: duas decomposições diferentes
podem produzir a mesma cadeia de bytes, e então dois artefatos distintos recebem
a mesma identidade. É a mesma classe de defeito que o resto desta especificação
existe para evitar — números válidos, garantia inválida, nenhum erro estrutural.
O manifesto resolve por construção, porque tem:

* **separador de domínio com versão** (`noteit.artifact.v1\n`), para que a mesma
  cadeia de bytes não possa ser lida como identidade de outro domínio ou de outra
  versão deste formato, e para que o formato possa mudar sem ambiguidade. Ele
  **não promete ausência de colisão** — isso continua apoiado na resistência a
  colisão e a pré-imagem do SHA-256, e nenhum prefixo altera essa propriedade. O
  que o separador dá é separação **semântica** entre domínios, que é o que estava
  faltando;
* **campos nomeados**, em vez de posições — não há concatenação a desambiguar;
* **comprimentos fixos** nos dois componentes variáveis, porque cada um entra
  como um digest hexadecimal de 64 caracteres e não como os bytes do arquivo;
* **ordem fixa e representação canônica**, para que o mesmo artefato produza a
  mesma identidade em qualquer máquina e em qualquer execução.

A propriedade exigida é a que segue disso: **trocar qualquer componente muda a
identidade, e nenhuma ambiguidade de codificação pode fazer dois conjuntos
diferentes de componentes produzirem a mesma entrada para o hash.**

Calculada na carga do artefato, sobre os bytes que foram efetivamente carregados,
e gravada no cabeçalho do cache. Trocar o arquivo de pesos por outro do mesmo
tamanho, atualizar o tokenizer, ou mudar a receita, muda o digest e invalida o
índice. Não é uma promessa de que ninguém vai trocar: é uma verificação que
falha.

**Provider remoto — o que dá para garantir, e o que não dá.**

| o provider oferece | o que o Note-it faz | garantia |
| --- | --- | --- |
| versão/snapshot imutável (`modelo-001`, `modelo@2026-01-15`) | grava o identificador no espaço | **forte**: o servidor não pode trocar o modelo sob o mesmo nome |
| apenas alias mutável (`modelo-latest`) | grava o alias e **marca o espaço como não verificável** | **fraca** |

Com alias mutável **não há como detectar** que o fornecedor trocou o modelo do
outro lado: os vetores antigos continuam com a dimensão certa e o ranking passa a
ser lixo, exatamente a falha medida. A resposta honesta não é fingir garantia, é:

1. registrar no cabeçalho do cache que aquele espaço é **não verificável**;
2. expor isso ao usuário junto do estado do índice;
3. dar um caminho de reconstrução manual sempre disponível;
4. **preferir**, na configuração padrão de cada provider, o identificador
   versionado quando o provider publicar um.

Nenhuma heurística de detecção é proposta — comparar amostras contra vetores
guardados seria inventar um teste estatístico cujo falso negativo é justamente o
caso perigoso.

## 6. Proveniência: `EmbeddingRecord`

```text
EmbeddingRecord {
    note_id
    source_revision           a revisão canônica da nota de onde este vetor veio
    chunk_id                  note_id + revision + ordinal + hash do chunk
    chunker_version
    space                     EmbeddingSpaceId
    vector
}
```

`source_revision` é a **revisão canônica que o Core já calcula**. Não se inventa
um segundo detector de estado: a 4.2A.R1 já registrou o custo de ter dois. A
revisão muda quando o conteúdo persistido muda, que é exatamente quando o vetor
deixa de valer.

`chunk_id` inclui o hash do texto do chunk além da posição, para que dois chunks
de texto igual em notas diferentes não colidam, e para que reordenação dentro de
uma nota não confunda — embora a revisão já mude junto.

**`source_revision` é chave de cache e mais nada.** Ela nunca é publicada num
candidato, nunca chega ao agente e nunca autoriza escrita. O atalho

```text
embedding → revision → write
```

é proibido. A cadeia da Fase 4.2 continua sendo a única:

```text
descobrir → noteit_read → revisão atual → decidir → escrever com expected_revision
```

## 7. Vetor obsoleto não pode mentir

Medido. Uma nota indexada com o texto A é editada para o texto B; o índice ainda
tem o vetor de A. Consultando o assunto de A:

```text
sem validação de proveniência:
    candidato nX  sim=0,5954   ← vetor de uma revisão que não existe mais
com validação de proveniência (descarta source_revision != revisão atual):
    candidato n12 sim=0,5639   ← nX desapareceu, corretamente
```

### A validação exige a leitura — e isso foi verificado no código

A 4.3A afirmava que a comparação detectava o vetor obsoleto "sem ler a nota".
**Está errado**, e a 4.3A.R1 corrigiu depois de conferir a implementação em vez
de supor:

```text
noteit-core/src/revision.rs
    NoteRevision::for_document(&NoteDocument)  -> sha256(document.serialize())
    NoteRevision::parse(&str)                  -> lê uma que o chamador mandou
noteit-core/src/write.rs
    revision_of(&NoteDocument)                 -> invólucro de for_document
```

São as **únicas** formas de obter uma `NoteRevision` no crate. A primeira serializa
o documento canônico inteiro — front matter incluído. A segunda não calcula nada.
E o que a varredura devolve não ajuda: `list_notes_by_recency_with_warnings` lê o
front matter só para o horário de edição e cai no `mtime` do arquivo, e
`NoteSummary` **não tem campo `revision`**.

Portanto **não existe caminho autoritativo para a revisão atual sem carregar o
`NoteDocument`**, e a 4.3A.R1 recusa-se a criar um: uma segunda definição de
estado é exatamente o defeito que a 4.2A.R1 registrou, e um detector barato que
discordasse do canônico seria pior que nenhum.

Em particular, `updated_at` **não serve**: ele move com o texto e fica parado
quando muda uma tag, uma propriedade, uma cor ou um tamanho de fonte — enquanto a
revisão move. Usá-lo como pré-filtro deixaria passar exatamente as edições que a
revisão existe para pegar.

**O fluxo correto, e ele é compatível com D-27:**

```text
índice → note_id
       → UMA leitura autoritativa do NoteDocument      (a que o motor já faz)
       → NoteRevision::for_document(&documento)
       → source_revision == revisão atual ?
            não → descartar o candidato e marcar para reindexação
            sim → snippet, motivos e tarefas saem DESSA MESMA leitura
```

**O custo real é zero em I/O**, e é aí que a afirmação corrigida fica melhor que
a original: o Context Engine já faz exatamente uma leitura autoritativa por
candidato, porque é disso que a coerência do candidato depende (D-27). A
validação de proveniência pega carona nessa leitura. O que ela não é, é gratuita
no sentido de dispensar a leitura — e a especificação não vai mais dizer que é.

A ordem importa: valida-se **antes** de publicar qualquer coisa daquele
candidato. Um resultado obsoleto pode custar uma recuperação pior; nunca pode
custar uma resposta com conteúdo velho apresentado como atual. E o snippet nasce
da leitura, nunca do cache.

Um registro obsoleto é descartado da resposta e agendado para reindexação.

### O que o índice guarda de texto

Três opções foram consideradas:

| | conteúdo | risco de dado velho | privacidade | tamanho |
| --- | --- | --- | --- | --- |
| A | só vetor + metadados | nenhum | melhor | menor |
| B | vetor + snippet derivado | o snippet pode envelhecer | pior | médio |
| C | vetor + texto do chunk | idem, ampliado | pior | maior |

**Escolhida a A.** O snippet sai da leitura da nota atual, que o Context Engine
já faz por candidato (D-27). Guardar texto no cache só pouparia essa leitura, e
compraria com isso um segundo lugar onde conteúdo de nota vive em disco e uma
segunda maneira de publicar texto velho. A leitura é o passo que o motor já tem e
que já é coerente.

## 8. Local e remoto

### Modo LOCAL

Nenhum texto sai da máquina. Funciona offline. Sem chave, sem cobrança, sem
conta. Custa CPU, RAM e disco locais. Em processo, porque um modelo estático é
uma tabela e uma média — não há runtime de inferência para isolar.

### Modo REMOTO — opt-in explícito, nunca por atualização

O usuário escolhe um provider remoto de propósito. Nada é enviado para a internet
porque uma versão nova habilitou recuperação semântica.

A distinção precisa ser legível, não deduzível:

```text
LOCAL    O conteúdo não sai desta máquina.
REMOTO   Trechos das suas notas são enviados para <provider> para gerar
         embeddings.
```

Índice local não significa privacidade: se o provider é remoto, o texto saiu
para ser embedado, mesmo que o vetor volte e fique aqui. Dizer o contrário seria
criar falsa sensação de localidade.

### O LLM e o provider são independentes

O host MCP não decide o provider. Todas estas combinações são válidas e nenhuma
regra as amarra:

```text
Claude + local        ChatGPT + local        Gemini + local
Claude + OpenAI       ChatGPT + Gemini       Gemini + Voyage
Claude + Voyage       ChatGPT + Voyage       …
```

Não existe `ChatGPT → embeddings da OpenAI` nem `Claude → Voyage`. A escolha é da
configuração do Note-it.

## 9. A fronteira de rede

O `noteit-mcp` e o `noteit-core` não têm acesso arbitrário à internet, e
`scripts/check-mcp-boundary` reprova quem tentar: nenhuma crate HTTP/TLS/socket
no grafo `--edges normal`, `tokio` sem a feature `net`, nenhum `std::net` no
código, e no Core só AF_UNIX — a família do endereço, não a palavra "socket"
(ADR-047).

**Essa fronteira não é afrouxada para caber provider remoto.** Ela é o motivo de
o desenho ser este:

```text
noteit-mcp ─────► noteit-core ─────► EmbeddingProvider (trait)
  sem rede           sem rede            │
                                         ├── LocalProvider — em processo, sem rede
                                         │
                                         └── RemoteProvider — cliente do worker
                                                  │  AF_UNIX, já permitido
                                                  ▼
                                          noteit-embed   ← processo separado
                                                  │  o ÚNICO com cliente HTTP
                                                  │  o ÚNICO que vê a credencial
                                                  ▼
                                          api.openai.com / …
```

O worker separado não está aqui por elegância — a 4.1R1.1 e a 4.2B ensinaram a
desconfiar disso. Ele está porque é a única forma de ter provider remoto **sem**:

1. colocar `reqwest`/`hyper` no grafo do `noteit-mcp`, o que reprova o gate;
2. colocar a credencial no processo que fala com o agente;
3. dar ao MCP a capacidade genérica de fazer requisições HTTP.

O canal é o mesmo padrão AF_UNIX que a autoridade de escrita já usa e que o gate
já permite por nome. O worker **só existe quando um provider remoto está
configurado**: no modo local o processo não é iniciado, e não há custo nenhum.

A subfase que implementar isto deve estender o gate, não relaxá-lo: `noteit-embed`
ganha regras próprias — é o único lugar onde uma crate HTTP é permitida, e ele
não pode ganhar acesso ao store.

## 10. Credenciais

Uma chave de API **nunca** pode estar em: nota, front matter, índice, embedding,
log, resposta MCP, stdout, stderr de produção sem redação, Git, ou documentação
gerada.

Separação:

* **configuração não secreta** — provider, modelo, dimensão, modo, política de
  fallback. Vai onde a configuração do Note-it já vai (`config.toml`, sob a
  mesma escrita atômica das demais).
* **credencial** — não vai ali.

Ordem de resolução proposta, a confirmar na subfase que implementar:

1. variável de ambiente do processo `noteit-embed` (`OPENAI_API_KEY` e
   equivalentes — a convenção que os próprios providers documentam);
2. Secret Service / keyring do sistema, quando disponível;
3. arquivo com permissão restrita, como último recurso e dito como tal.

Nunca implementar armazenamento inseguro "só para o protótipo".

**O cliente MCP não precisa saber qual credencial o Note-it usa**, e não há tool
que a devolva.

## 11. Erros de provider

Erros tipados no Core, mensagens públicas escolhidas pelo Note-it:

```text
ProviderError::Unavailable        ProviderError::InvalidResponse
ProviderError::Authentication     ProviderError::ModelUnavailable
ProviderError::RateLimited        ProviderError::DimensionMismatch
```

Nunca `format!("{external_error}")` numa resposta MCP. É a lição da 4.2R.R1
aplicada antes do defeito existir: **a biblioteca ou o fornecedor não escreve a
mensagem pública do Note-it**. Uma API remota devolve request IDs, mensagens e
fragmentos que ninguém controla, e uma resposta de erro que os ecoa é o mesmo
vazamento que o `check-mcp-boundary` já proíbe por `format!`.

Comportamento exigido para timeout, 429, quota, API fora do ar, chave inválida,
modelo removido, resposta malformada, dimensão inesperada, NaN/Inf, resposta
parcial, lote parcial, cancelamento e queda de internet: **nenhum deles quebra
uma nota**, e nenhum deles vira mensagem livre no fio.

## 12. Fallback

```text
provider semântico falhou
        │
        ├─ existe índice semântico válido e compatível? → usar
        │
        └─ não → recuperação lexical, e a resposta diz `semantic_unavailable`
```

Modos de configuração:

| modo | comportamento |
| --- | --- |
| `automatic` | degrada para lexical em silêncio informado (o campo diz que degradou) |
| `semantic_required` | falha em vez de degradar — quem pediu semântico saber que não teve |
| `lexical_only` | nem tenta |

`automatic` é o padrão. `semantic_required` existe porque mascarar a falha de
quem pediu explicitamente semântica é mentir sobre o que foi feito.

Nada disto afeta ler, escrever, listar, buscar, a CLI, o MCP ou as notas.

## 13. Pipeline de recuperação

```text
consulta
  ↓ normalização (a dobra que search::fold já faz, para o lado lexical)
  ↓
  ├── lexical: termos + BM25 ────────────────┐
  └── semântico: embed_query → cosseno ──────┤
                                             ↓
                            camadas concatenadas (§13.1)
                                             ↓
                            candidatos preliminares: note_id + canal
                                             ↓
                            UMA leitura autoritativa por candidato
                                             ↓
                            NoteRevision::for_document → validar
                                             ↓
                            stale → descartar e reindexar
                            válido → snippet, motivos e tarefas DESSA leitura
                                             ↓
                            tetos aplicados, sem revision publicada
                                             ↓
                                       noteit_context
```

### A etapa lexical, e os parâmetros congelados

Casamento por **termo** e não por frase. Termos são as sequências de `[0-9a-z]`
do texto dobrado pela dobra que o Note-it já tem (`search::fold`: minúscula
Unicode, tabela de Latin-1 e Latin Extended-A, marcas combinantes descartadas).
**Nenhuma normalização nova**: a dobra existente já resolve acento, e duplicá-la
criaria duas verdades sobre a mesma palavra.

Ranking por BM25 com **`k1 = 1.2` e `b = 0.75`, congelados**.

São os valores canônicos da literatura, e a 4.3A.R1 os congela **antes** da
medição final, em vez de mandar a 4.3B "confirmá-los contra o corpus", que era o
que a 4.3A dizia. Ajustar parâmetros num conjunto e depois apresentar a métrica
desse mesmo conjunto como validação é medir o próprio ajuste — e com 32 consultas
o ajuste caberia inteiro dentro do ruído.

A metodologia, explícita:

* **os parâmetros são congelados antes** e não se movem para melhorar o número;
* **o corpus é régua de regressão, não conjunto de validação independente.** Ele
  responde "esta mudança piorou alguma coisa que funcionava?", que é a pergunta
  para a qual foi construído. Ele não responde "este motor é bom", e nenhum
  número dele deve ser citado como se respondesse;
* se alguma subfase quiser de fato ajustar `k1`, `b` ou qualquer peso, precisa
  **antes** de um conjunto de tuning separado e um de avaliação que não seja
  usado no ajuste — e o corpus de hoje é pequeno demais para ser partido em dois
  de forma útil, o que significa escrever mais casos primeiro;
* qualquer ajuste feito sem isso é registrado como ajuste, nunca como melhoria.

A limitação não é escondida: **30 notas e 32 consultas separam arquiteturas com
folga e não separam dois parâmetros parecidos.**

### Admissão e ranking: a política completa

Esta é a política inteira do motor, e não só a parte nova. Ela foi derivada do
comportamento **medido** de `noteit-core/src/context.rs` contra o binário real, e
não do que seria conveniente implementar — porque o motor de hoje já admite
candidatos por quatro sinais que a primeira redação desta especificação deixou de
fora, e a 4.3B não pode inventar em Rust onde eles entram.

#### Quais sinais admitem, e quais só acrescentam motivo

| sinal | admite? | condição, exatamente como hoje |
| --- | --- | --- |
| `TextMatch` | **sim** | há consulta **e** ela ocorre no texto visível |
| `SharedTag` | **sim** | a nota carrega uma tag pedida — **com ou sem consulta** |
| `PropertyMatch` | **sim** | a nota carrega a propriedade pedida — **com ou sem consulta** |
| `TaskMatch` | **sim** | há consulta **e** uma tarefa da nota casa com ela |
| `TermMatch` | **sim** (4.3B) | há consulta e um termo dela ocorre na nota |
| `SemanticMatch` | **sim** (4.3C) | há consulta e a nota está entre as mais próximas |
| `Recent` | **sim**, e só ele | a requisição **não tem consulta nem filtro** |

`include_tasks` **não admite e não deixa de admitir**: `TaskMatch` vira motivo de
qualquer jeito, e a bandeira decide apenas se as tarefas viajam junto do
candidato. Medido: com `include_tasks` verdadeiro ou falso, os motivos e a ordem
são os mesmos.

Nada mais admite. Uma nota sem nenhum motivo **não é candidata**, e nesse caso
`Recent` não a resgata — `Recent` só existe quando a requisição inteira não tinha
sinal nenhum.

#### As quatro classes de precedência

```text
classe 1  SINAIS DECLARADOS   TextMatch · SharedTag · PropertyMatch · TaskMatch
classe 2  TERMOS              TermMatch                                  (4.3B)
classe 3  SEMÂNTICA           SemanticMatch                              (4.3C)
classe 4  RECÊNCIA            Recent   — exclusiva: só existe sozinha
```

A ordem final é a concatenação das classes, cada uma ordenada internamente, **sem
reordenação entre elas**.

**Por que os quatro sinais de hoje ficam na mesma classe, e não em fila.** Foi
medido, contra o binário real: hoje `TextMatch` **não** tem precedência sobre
`SharedTag` nem `PropertyMatch`. A ordenação é por contagem de motivos, e uma
nota com `shared_tag` + `property_match` fica **acima** de uma nota com
`text_match` sozinho:

```text
consulta "hipertensao", tags=[cardio], fonte=diretriz
   A (tag + propriedade, 2 motivos)   ← primeira
   B (text_match, 1 motivo)           ← segunda
```

Pôr `TextMatch` numa camada acima das outras três seria mudar esse
comportamento — silenciosamente, e sem que nenhuma auditoria tivesse pedido.
Então a classe 1 é o **conjunto de admissão que o motor já tem**, ordenado pela
regra que ele já usa, e as classes 2 e 3 são **estritamente aditivas**: elas
acrescentam candidatos abaixo de tudo o que já existia e não movem nada.

A consequência é a propriedade que a 4.3A.R1 pediu, na forma exata em que ela é
verdadeira: **um candidato admitido por `TextMatch` nunca é rebaixado por
`TermMatch` nem por `SemanticMatch`** — porque estes vivem em classes inferiores.
Ele continua podendo ficar atrás de um candidato de `SharedTag` com mais motivos,
exatamente como hoje, e isso é preservação e não regressão.

#### Como um candidato com vários sinais escolhe a classe

Pela **classe mais alta** entre os canais que o admitiram, e ele aparece **uma
única vez**, carregando **todos** os motivos aplicáveis. Uma nota que casa a
frase, carrega a tag e também é semanticamente próxima é um candidato de classe 1
com `text_match`, `shared_tag` e `semantic_match` nos motivos.

Corolário: assim que um candidato entra na classe 1, o BM25 e a similaridade
dele deixam de influenciar sua posição — dentro da classe 1 a ordenação é a de
hoje. É isso que torna a proteção estrutural em vez de dependente de escala
numérica.

#### Ordenação dentro de cada classe

| classe | 1º | 2º | 3º | 4º |
| --- | --- | --- | --- | --- |
| 1 sinais declarados | mais motivos distintos | `updated_at` mais recente, ausente por último | `note_id` | — |
| 2 termos | BM25 decrescente | mais motivos distintos | `updated_at` | `note_id` |
| 3 semântica | similaridade decrescente | `updated_at` | `note_id` | — |
| 4 recência | `updated_at` mais recente | `note_id` | — | — |

A classe 1 e a classe 4 reproduzem `context.rs::order` sem alteração. `note_id`
fecha as quatro: sem ele, duas notas escritas no mesmo segundo cairiam na ordem
que o sistema de arquivos entregou, e a mesma pergunta responderia diferente no
mesmo store. **Não existe candidato sem classe**, e a ordenação é total.

#### O que cada forma de requisição produz

Medido contra o binário real, e é o comportamento a preservar:

| requisição | hoje | depois da 4.3B/4.3C |
| --- | --- | --- |
| só consulta textual | classe 1 por `TextMatch`/`TaskMatch` | igual, **mais** classes 2 e 3 abaixo |
| consulta + tags | classe 1, tag e texto como pares | igual, mais classes 2 e 3 |
| consulta + propriedades | idem | idem |
| **só filtro, sem consulta** | classe 1 por `SharedTag`/`PropertyMatch` | **igual — e sem classes 2 e 3**, porque não há consulta para casar termo nem para embutir |
| **requisição vazia** | classe 4, todas as notas com `Recent` | **igual**, e só classe 4 |
| consulta que não casa nada | vazio, **sem** `Recent` | classes 2 e 3 podem agora trazer candidatos, rotulados |
| consulta que dobra para nada | vazio | vazio — ver abaixo |

**Sem consulta não há classe 2 nem classe 3.** Não é uma economia: não há termo
para pontuar e embutir uma consulta vazia não significa nada. Filtro sozinho
continua sendo classe 1 e só.

**A consulta que dobra para vazio.** Uma consulta feita só de marcas
combinantes tem `trim()` não vazio — então a requisição *conta* como tendo sinal
e `Recent` não é oferecido — mas dobra para a cadeia vazia, então nada casa. O
resultado é vazio, e foi medido. É o comportamento atual, fica registrado como
tal, e a 4.3B não deve alterá-lo por acidente; mudá-lo é decisão própria com
justificativa.

#### A regressão obrigatória da 4.3B

Para **toda** consulta do corpus em que o motor de hoje devolve um acerto, esse
acerto continua na mesma posição ou mais acima depois do BM25, e depois do
semântico. Consulta por consulta, não em média.

O semântico **só preenche o que sobrou**.

Medido contra Reciprocal Rank Fusion: a RRF pontua um pouco melhor em R@3 (1,000
contra 0,900 no melhor caso) e **rebaixou um acerto exato** numa consulta. O
encadeamento não pode rebaixar — não é uma observação sobre este corpus, é a
forma da operação. Num corpus de 32 consultas uma diferença de uma ou duas está
dentro do ruído; a garantia estrutural não está.

Isso também resolve um problema que só aparece com múltiplos providers: um
ranking que dependa da escala numérica de um fornecedor muda de comportamento
quando o fornecedor muda. O encadeamento depende de **ordem**, não de escala.

### Nada de limiar universal

Medição que restringe a arquitetura: **nenhum limiar de similaridade separa "tem
resposta" de "não tem resposta"**.

```text
e5-small     menor topo-1 com resposta 0,8248   maior sem resposta 0,8494
potion       menor topo-1 com resposta 0,1760   maior sem resposta 0,3469
static-mrl   menor topo-1 com resposta 0,0995   maior sem resposta 0,1486
```

As faixas se sobrepõem nos três. E as escalas não são comparáveis entre modelos:
`0,82` num não é `0,82` no outro. Nenhum limiar universal, e qualquer limiar por
espaço só depois de calibrado contra o corpus.

Consequência: hoje o motor devolve **vazio** quando nada casa, e isso é
informação verdadeira. Um motor semântico sempre tem vizinho mais próximo.
Então candidatos puramente semânticos são **rotulados** e **limitados** (proposto:
no máximo 3 quando não houve nenhum sinal lexical), para que "não achei nada com
as suas palavras" continue legível em vez de virar dez candidatos com cara de
certeza.

## 14. Chunking

**Parágrafo**, com a nota inteira como fallback. Medido: a nota longa do corpus
(7 878 caracteres, com o trecho relevante no meio) é perdida pelo embedding da
nota inteira e encontrada pelo embedding por parágrafo.

1. Separar por linha em branco — a fronteira que o Markdown já usa e que o autor
   escolheu.
2. Parágrafo acima de **800 caracteres** é partido em fronteira de sentença.
3. Sem sobreposição: multiplica vetores para recuperar contexto que a média já
   borra; custo certo, ganho não medido.
4. Nota vazia produz nenhum vetor.
5. O chunker **lê** e nunca altera. É visão derivada, e tem versão própria, que
   entra na identidade do chunk e na validade do cache.

O texto que entra é o **texto visível** (`visible_text`), o mesmo que a busca
lexical usa: cor, comentário HTML e front matter não são embedados, exatamente
como não são pesquisáveis hoje.

## 15. Índice: derivado, e por ora em memória

| escala | indexar (local estático) | matriz | consulta p50 | p95 |
| --- | --- | --- | --- | --- |
| 100 notas | 0,07 s | 0,10 MB | 0,012 ms | 0,024 ms |
| 1 000 | 0,79 s | 1,02 MB | 0,072 ms | 0,091 ms |
| 5 000 | 4,02 s | 5,12 MB | 5,25 ms | 7,47 ms |
| 10 000 | 7,13 s | 10,24 MB | 3,51 ms | 6,91 ms |

O store real da máquina onde isto foi medido tem **41 notas**: cerca de 30 ms.

**Sem ANN.** Força bruta custa 3,5 ms com 10 000 vetores. ANN entra se passar de
**50 ms**, o que fica em centenas de milhares de vetores.

**Sem persistência em v1, no modo local.** O que custa não é o índice, é o
artefato do modelo. Gatilho para reavaliar: indexação a frio acima de **2 s** num
store real, o que pelas medições é por volta de 2 500 notas.

**O modo remoto inverte isso.** Ali cada reindexação custa dinheiro e latência de
rede, não CPU ociosa — então persistir passa a valer desde a primeira nota. A
decisão não é a mesma para os dois modos, e a especificação não finge que é:

| | local | remoto |
| --- | --- | --- |
| custo de reindexar | CPU local, grátis | tokens pagos + latência |
| persistência em v1 | não | **sim** |

### Quando houver cache em disco

* Em `$XDG_CACHE_HOME/note-it/`, **nunca** dentro de `notes/`.
* Cabeçalho de validade que se autoidentifica: versão do formato,
  `EmbeddingSpaceId` inteiro, versão do chunker. Incompatível → **reconstruir**,
  jamais reinterpretar.
* Escrita atômica com renomeação como ponto de commit, a mesma regra de uma nota
  (Fase 3.4R.2): construir em temporário, validar, publicar atomicamente. Queda
  antes do commit deixa o índice anterior válido; queda depois deixa o novo
  reconhecível. **Nunca meia-indexação que pareça completa.**
* Permissões restritas ao usuário. Um vetor é dado derivado de nota privada e
  não é "não sensível" por não ser texto.
* Validar na carga: dimensão, finitude, contagem, espaço. Um cache é entrada
  não confiável como qualquer outra.

### Um índice ou vários

Trocar de provider não pode comparar vetores de espaços diferentes — e a §5 mede
o que acontece se comparar. Três desenhos:

| | disco | complexidade | trocar de provider |
| --- | --- | --- | --- |
| um índice ativo, rebuild na troca | menor | menor | recalcula tudo |
| um diretório por `EmbeddingSpaceId` | N espaços | média | volta instantâneo |
| índices independentes com política de expiração | maior | maior | idem + limpeza |

**Recomendado: um índice ativo por vez em v1**, com o diretório nomeado pelo
`EmbeddingSpaceId` — de modo que guardar mais de um seja uma mudança de política
de limpeza e não de formato. No modo local rebuild custa 7 s para 10 000 notas e
não justifica guardar espaços mortos; no modo remoto custa dinheiro, e é ali que
guardar o espaço anterior passa a compensar. A decisão final é da subfase que
implementar o modo remoto, com número na mão.

## 16. Ciclo de vida de uma nota

```text
CRIAR      nota R1 → chunks R1 → vetores R1

EDITAR     nota R2
           vetores R1 tornam-se STALE — detectáveis por source_revision != R2
           chunks/vetores R2 são produzidos
           R2 passa a ser o ativo quando a publicação atômica completar

LIXEIRA    a nota sai da varredura de notas vivas
           candidatos vivos desaparecem
           vetores viram órfãos e são recolhidos

RESTAURAR  volta à varredura; reindexa se a revisão não casar

PERDER O CACHE
           as notas continuam intactas
           o índice é reconstruído — sempre a resposta correta
```

**Indexar é leitura.** Não altera conteúdo, front matter, `updated_at`,
`created_at`, `revision` nem o `mtime` do arquivo. Ler uma nota para gerar um
embedding não pode parecer edição — a Fase 3.4R levou uma fase inteira para que
abrir uma nota não movesse `updated_at`, e isto não vai desfazer aquilo.

### Incremental contra global

| gatilho | escopo |
| --- | --- |
| uma nota mudou | só aquela nota e seus chunks |
| chunker mudou de versão | global |
| provider, modelo, dimensão ou task mudaram | global |
| versão do formato do cache mudou | global |

Uma edição numa nota **não** pode recalcular dez mil.

### Órfãos

Recolher: vetor cuja nota sumiu, chunk de revisão antiga, índice de provider que
não se usa mais, cache de modelo removido. Sem crescimento ilimitado — e a
limpeza **nunca** apaga nota.

## 17. O que o MCP vê

* **Nunca vetores.** `noteit_context` não devolve arrays de float. Gasta tokens,
  aumenta a resposta, expõe representação derivada, não ajuda o agente e acopla
  o protocolo a um detalhe interno. `"vector[182] = -0.18472"` não é útil;
  `"esta nota foi recuperada por semelhança semântica"` é.
* **Nunca `revision`** por descoberta. Nem a da nota, nem a `source_revision` do
  registro.
* **Nunca** o caminho do cache, o nome interno do índice, o ID de requisição do
  provider ou a credencial.
* Continua valendo tudo da 4.2: tetos de snippet, warnings sem texto livre,
  lixeira limitada, conteúdo de nota como dado.

O que passa a existir é **o canal de recuperação**, como motivo:

```text
Reason::TextMatch        a consulta ocorre no texto        (existe)
Reason::TermMatch        termos da consulta ocorrem        (4.3B)
Reason::SemanticMatch    admitido pelo canal semântico     (semântico)
Reason::SharedTag / PropertyMatch / TaskMatch / Recent      (existem)
```

Um agente que recebe `semantic_match` sabe que aquela nota **não** usa as
palavras dele e pode decidir se lê. Isso é o que um motivo dá e um número não.

**Sem score publicado em v1.** Se um dia houver, será uma similaridade de
cosseno, nomeada `similarity`, nunca `confidence`, `score`, `relevance` nem
`probability`, e nunca apresentada como porcentagem: `0,81` não é "81% de chance
de ser relevante", e escrever `81%` afirma exatamente isso. E nunca comparável
entre espaços diferentes.

### Embedding não valida fato

Embedding mede **proximidade representacional**. Não valida verdade. Score alto
não torna um texto verdadeiro; score baixo não o torna falso. O embedding decide
*o que talvez valha a pena ler*, não *o que é verdade*.

Daí a forma do fluxo: o agente recebe **texto da nota atual**, não vetores, e é a
leitura do estado atual que vira evidência. Uma arquitetura em que o LLM recebe
números não tem como ser verificada por ninguém.

## 18. Concorrência

As regras da 4.2B continuam e não são afrouxadas: toda chamada ao Core a partir
do MCP passa por `spawn_blocking` e pelo testemunho `OffThread`; o reactor
continua respondendo `ping` durante uma indexação; a GUI nunca embute na thread
do main loop.

Acrescenta-se: **uma indexação por processo por vez** — duas consultas
concorrentes sobre um store não indexado não podem disparar duas indexações — e,
no modo remoto, controle de admissão com limite de chamadas em voo, respeito a
429 e cancelamento.

A 4.2R.R1 mediu 4 clientes × 8 requisições hostis de 300 KiB em 41 ms com
+1,1 MB de RSS. A subfase que implementar a etapa semântica repete aquele teste
**com indexação em curso**.

## 19. Providers remotos — o que a documentação oficial diz

Verificado em **2026-09-04**, nas fontes primárias. Nenhum destes foi
**medido**: sem credencial disponível nesta sessão, e §41 do enunciado é
explícita — melhor não medido que inventado.

### OpenAI — `developers.openai.com/api/docs/guides/embeddings`

| modelo | dimensão | máx. entrada | preço /1M tokens |
| --- | --- | --- | --- |
| `text-embedding-3-small` | 1536, reduzível por `dimensions` | 8 192 tokens | US$ 0,02 |
| `text-embedding-3-large` | 3072, reduzível por `dimensions` | 8 192 tokens | US$ 0,13 |
| `text-embedding-ada-002` | 1536, fixa | 8 192 tokens | US$ 0,10 |

Lote: uma requisição aceita um array de entradas. Batch API com desconto de
cerca de 50%. Suporte multilíngue não é declarado explicitamente na página.

### Google Gemini — `ai.google.dev/gemini-api/docs/embeddings`

| modelo | dimensão | máx. entrada | preço /1M tokens |
| --- | --- | --- | --- |
| `gemini-embedding-2` | 128–3072, normalização automática | 8 192 tokens | US$ 0,20 (texto) |
| `gemini-embedding-001` | 128–3072, normalização manual fora de 3072 | 2 048 tokens | US$ 0,15 |

Mais de 100 idiomas, português incluído. `gemini-embedding-001` tem tipos de
tarefa por parâmetro (`RETRIEVAL_DOCUMENT`, `RETRIEVAL_QUERY`, e outros); o
`gemini-embedding-2` os expressa por instrução no prompt. Batch API a 50%. Há
faixa gratuita.

### Voyage AI — `docs.voyageai.com`

| modelo | dimensão | contexto | preço /1M tokens |
| --- | --- | --- | --- |
| `voyage-4-large` | 1024 (256/512/2048) | 32 000 | US$ 0,12 |
| `voyage-4` | 1024 (256/512/2048) | 32 000 | US$ 0,06 |
| `voyage-4-lite` | 1024 (256/512/2048) | 32 000 | US$ 0,02 |
| `voyage-4-nano` | 1024 (256/512/2048) | 32 000 | pesos abertos, Apache-2.0 |

Multilíngue na série 4. `input_type` `query`/`document` — o provider prepende
instruções diferentes. Quantização `float`/`int8`/`binary` na resposta. Lote de
até 1 000 textos por requisição. Faixa gratuita de 200 milhões de tokens por
conta na série 4. Embeddings Matryoshka: truncar mantendo o prefixo.

`voyage-4-nano` tem **pesos abertos sob Apache-2.0** — é o único candidato que
poderia, no futuro, ser um provider *local* e um provider *remoto* com o mesmo
espaço vetorial. Não avaliado nesta fase.

### Anthropic

Não tem modelo de embeddings próprio. A documentação oficial diz, verbatim:
*"Anthropic does not offer its own embedding model"*, e recomenda a Voyage AI.
**Claude usa o Segundo Cérebro normalmente** — como qualquer host MCP — e o
embedding remoto, se o usuário quiser um, vem da Voyage ou de outro provider.

## 20. Custo remoto

O corpus real de referência é o store desta máquina: **41 notas**, mediana de
402 caracteres. Estimando 4 caracteres por token e chunk por parágrafo:

| operação | tokens | `text-embedding-3-small` | `voyage-4-lite` | `gemini-embedding-001` |
| --- | --- | --- | --- | --- |
| indexar 41 notas | ~4 mil | < US$ 0,001 | < US$ 0,001 | < US$ 0,001 |
| indexar 1 000 notas | ~100 mil | ~US$ 0,002 | ~US$ 0,002 | ~US$ 0,015 |
| indexar 10 000 notas | ~1 milhão | ~US$ 0,02 | ~US$ 0,02 | ~US$ 0,15 |
| uma consulta | ~20 | desprezível | desprezível | desprezível |

Ordens de grandeza a partir dos preços publicados, não medições.

A conclusão que importa não é o valor, é a **forma**: indexação é o custo, e ele
é pago uma vez por revisão de nota. **O usuário não pode pagar para embedar todas
as notas a cada busca** — o que torna cache persistente obrigatório no modo
remoto, e reindexação incremental um requisito e não uma otimização.

## 21. A matriz

`MEASURED` mede este corpus. `DOCUMENTED` vem de fonte oficial em 2026-09-04.
`UNKNOWN` não foi medido e não é estimado.

| dimensão | Local (estático) | OpenAI | Gemini | Voyage |
| --- | --- | --- | --- | --- |
| qualidade PT-BR | MEASURED: bem no corpus | UNKNOWN | UNKNOWN | UNKNOWN |
| R@3 (encadeado) | MEASURED 0,900 | UNKNOWN | UNKNOWN | UNKNOWN |
| MRR (encadeado) | MEASURED 0,845 | UNKNOWN | UNKNOWN | UNKNOWN |
| latência de indexação | MEASURED 1 250–1 400 notas/s | UNKNOWN (rede) | UNKNOWN | UNKNOWN |
| latência de consulta | MEASURED 3,5 ms @ 10 k | UNKNOWN (rede) | UNKNOWN | UNKNOWN |
| CPU local | alta na indexação | mínima | mínima | mínima |
| RAM local | UNKNOWN em Rust; o artefato domina | mínima | mínima | mínima |
| disco | 108–512 MB de artefato + vetores | só vetores | só vetores | só vetores |
| custo de API | N/A | DOCUMENTED US$ 0,02–0,13 /1M | DOCUMENTED US$ 0,15–0,20 /1M | DOCUMENTED US$ 0,02–0,12 /1M |
| offline | **SIM** | NÃO | NÃO | NÃO |
| conteúdo sai da máquina | **NÃO** | SIM | SIM | SIM |
| lote | N/A | DOCUMENTED array + Batch −50% | DOCUMENTED Batch −50% | DOCUMENTED 1 000 textos |
| dimensões | 256 / 1024 (MRL) | 1536 / 3072, reduzíveis | 128–3072 | 1024 (256/512/2048) |
| licença / termos | MIT / Apache-2.0 | termos da OpenAI | termos do Google | termos da Voyage |
| integração | em processo | worker + credencial | worker + credencial | worker + credencial |

Nenhum vencedor remoto é declarado. Declarar sem medir seria transformar
documentação de fornecedor em benchmark interno, que é exatamente o que §77 do
enunciado proíbe.

## 22. Padrão recomendado

```text
DEFAULT              lexical por termos (BM25) — sem modelo, sem chave, sem download
PRIVACIDADE/OFFLINE  local estático, quando o usuário habilitar
MELHOR QUALIDADE     por medir; provavelmente remoto, e sem evidência ainda
MENOR CUSTO LOCAL    lexical
REMOTO OPCIONAL      OpenAI / Gemini / Voyage, sempre opt-in
```

### O padrão, sem ambiguidade

A 4.3A dizia "LOCAL — o padrão" numa seção e "DEFAULT lexical" em outra. São
coisas diferentes e as duas não podem ser o padrão. A 4.3A.R1 fixa **três
níveis**, e só o primeiro é o padrão de fábrica:

```text
semantic_retrieval:
    mode:     lexical_only          ← PADRÃO DE FÁBRICA
              | semantic            ← o usuário liga explicitamente
    provider: local                 ← o padrão QUANDO mode = semantic
              | openai | gemini | voyage   ← sempre opt-in explícito
    fallback: automatic | semantic_required | lexical_only
```

| nível | o que acontece | exige ação do usuário |
| --- | --- | --- |
| `lexical_only` | BM25 e mais nada. Sem download, sem modelo, sem credencial, sem rede | **nenhuma — é o padrão** |
| `semantic` + `provider: local` | baixa/usa o artefato local; nada sai da máquina | ligar a recuperação semântica |
| `semantic` + provider remoto | trechos das notas vão para o provider | escolher o provider **e** a credencial |

**Nenhuma leitura desta configuração permite que uma atualização passe a enviar
conteúdo ou a baixar modelo.** Uma instalação nova, e uma instalação atualizada
que nunca foi configurada, ficam em `lexical_only`: o padrão de fábrica não muda
por versão nova, e mudar `mode` é sempre um ato do usuário. Se o padrão de
fábrica algum dia mudar, isso é decisão de produto com ADR própria e não efeito
colateral de uma release.

`provider: local` é o padrão **apenas dentro de `mode: semantic`** — quer dizer,
quem liga a semântica sem dizer mais nada recebe a local e não a remota. Um
provider remoto nunca é alcançado sem que o usuário o nomeie.

**Nenhuma chave remota é requisito do primeiro uso**, e nem sequer o modelo local
é: o padrão de fábrica é o lexical, que já leva R@3 de 0,367 a 0,767 e não baixa
nada.

Isto é recomendação de padrão, não exclusividade: mesmo que o local ganhe, os
remotos continuam; mesmo que um remoto ganhe em qualidade, o local continua.
Benchmark não é lock-in.

## 23. Portabilidade e ausência de lock-in

* Trocar `OpenAI → local`, `Gemini → Voyage`, `Voyage → local` **não migra
  notas**. No máximo, reindexa.
* **Nenhum metadado de provider entra na nota.** Provider pertence à
  configuração e ao cache. O Markdown continua portável e não sabe que
  embeddings existem.
* O índice é do Note-it. Serviços gerenciados de vetores dos fornecedores
  mudariam a arquitetura de *"o Note-it é dono do seu índice"* para *"o
  fornecedor é dono do estado da recuperação"*. Não são adotados por
  conveniência; seriam uma decisão separada, de alto impacto, e a preferência
  desta fase é explícita: o Note-it continua dono do seu índice derivado.
* Mesmo com provider remoto, os vetores voltam e ficam aqui. A API é necessária
  para *gerar* embeddings, não para hospedar o índice.

## 24. Privacidade, dita em uma frase por item

Conteúdo de nota é dado privado. Sem telemetria. Sem analytics. Sem upload que
não seja a geração de embedding do provider que o usuário escolheu, para o
endpoint daquele provider. O modo local não envia nada. O usuário deve conseguir
ver, a qualquer momento: provider atual, modelo atual, local ou remoto, quando
foi a última indexação, e o estado do índice.

## 25. Orçamentos propostos

| grandeza | orçamento | de onde vem |
| --- | --- | --- |
| indexação a frio, 1 000 notas (local) | ≤ 2 s | medido 0,79 s em Python |
| indexação a frio, 10 000 notas (local) | ≤ 20 s | medido 7,13 s |
| consulta quente, 10 000 vetores | ≤ 20 ms | medido 3,5 ms p50 / 6,9 ms p95 |
| carga do artefato local | ≤ 2 s | medido 1,0–1,8 s |
| RSS acrescido pelo modelo | a medir em Rust | os números desta fase são de processo Python e **não são representativos** |
| consulta com provider remoto | a medir | latência de rede domina |
| resposta MCP | inalterada | os tetos da 4.2R continuam valendo |

## 26. O que a 4.3A não mediu

* **Nada foi implementado em Rust.** As medições vêm de um protótipo Python com
  ONNX Runtime. Comparam candidatos entre si; não preveem o desempenho da
  implementação.
* **Nenhum provider remoto foi medido** — sem credencial nesta sessão. Todos os
  números remotos são `DOCUMENTED`, nunca `MEASURED`.
* **RSS não foi medido de forma utilizável.**
* **O corpus tem 30 notas e 32 consultas.** Separa arquiteturas com folga; não
  separa dois modelos parecidos.
* **Quantização não foi avaliada em qualidade** para os modelos estáticos.
* **A licença de `model2vec-rs` não foi verificada** — o `crates.io` publica
  `non-standard`, o card do modelo diz MIT.
* **`voyage-4-nano`, de pesos abertos, não foi avaliado** como provider local.

## 27. Testes que a implementação deve trazer

Além do corpus como regressão de qualidade:

`nota vazia` · `nota gigantesca` · `milhares de notas` · `corpo repetitivo` ·
`todas as notas iguais` · `embeddings idênticos` · `vetor zero` · `NaN` · `Inf` ·
`dimensão errada` · `cache truncado` · `cache de espaço vetorial antigo` ·
`índice de provider trocado` · `modelo trocado dentro do mesmo provider` ·
`note_id inexistente` · `vetor órfão` · `nota editada durante a busca` ·
`nota para a lixeira durante a busca` · `modelo ausente` · `cache somente
leitura` · `disco cheio` · `queda durante reconstrução` · `symlink no cache` ·
`Unicode hostil` · `prompt injection no texto` · `consulta gigantesca` ·
`consultas concorrentes` · `resposta remota hostil` · `429 e timeout` ·
`chave inválida` · `credencial em log` · `orçamento de resposta MCP` ·
`query e document do mesmo espaço são comparáveis` · `espaços diferentes são
recusados` · `dimensão igual não basta` · `artefato trocado com mesmo nome e
dimensão` · `alias mutável marcado como não verificável` · `acerto exato nunca
rebaixado pelo BM25` · `acerto exato nunca rebaixado pelo semântico` ·
`padrão de fábrica não baixa nem envia nada`

### A matriz que a 4.3B congela ANTES de escrever o BM25

Estes testes descrevem o motor **de hoje** e devem passar antes de qualquer
mudança de ranking, para que a mudança seja medida contra eles e não contra a
memória de ninguém:

| # | cenário | o que fica congelado |
| --- | --- | --- |
| 1 | casamento de frase exata | admitido, `text_match` nos motivos |
| 2 | tag sem consulta | admitido, `shared_tag`, **sem** consulta nenhuma |
| 3 | propriedade sem consulta | admitido, `property_match` |
| 4 | consulta + tag | ambos admitidos; ordem por contagem de motivos |
| 5 | consulta + propriedade | idem |
| 6 | tarefa com `include_tasks = false` | `task_match` presente, `tasks` vazio |
| 7 | tarefa com `include_tasks = true` | `task_match` presente, `tasks` preenchido, mesma ordem |
| 8 | requisição vazia | todas as notas com `recent`, ordem por recência |
| 9 | vários motivos na mesma nota | um único candidato, motivos acumulados |
| 10 | empate final | resolvido por `note_id`, estável entre execuções |

E, sobre o corpus: **para cada consulta** em que o motor de hoje encontra o
ground truth, registrar a posição do acerto; depois do BM25, nenhum desses
acertos pode estar abaixo da posição registrada. Consulta por consulta, nunca
por média.

Testes sintéticos de ranking, com valores construídos para atacar a fronteira:

* pontuação BM25 arbitrariamente alta **não** atravessa candidato de classe
  superior;
* similaridade arbitrariamente alta **não** atravessa um `TextMatch`;
* candidato com vários motivos aparece **uma vez**;
* candidato admitido só por sinal não textual tem posição definida — não fica
  "sem classe";
* nenhuma ordenação depende da ordem em que o sistema de arquivos entregou as
  notas.

O corpus já carrega quatro deles como dados: `n17` (prompt injection), `n18`
(Unicode hostil), `n19` e `n20` (nota vazia e mínima).
