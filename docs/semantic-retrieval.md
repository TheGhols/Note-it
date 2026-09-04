# Recuperação semântica — especificação

Decidido na Fase 4.3A. Nada disto está implementado: este documento é a
especificação que as subfases de implementação consomem, e a régua contra a
qual elas serão medidas. Justificativa e medições na ADR-056.

O corpus de avaliação está em [`retrieval-corpus.json`](retrieval-corpus.json).

---

## 1. O problema, medido

O Context Engine de hoje (`noteit-core/src/context.rs`) casa **a consulta
inteira como substring** do texto dobrado da nota. Não há casamento por termo,
não há pontuação e não há ranking além de (mais motivos → recência → `note_id`).

A consequência foi medida contra o binário real, por stdio, sobre um store
sintético de 30 notas e 32 consultas:

```
19 das 30 consultas com resposta voltam VAZIAS
R@1 0,333   R@3 0,367   R@5 0,367   MRR 0,350
```

"hipertensão arterial" não encontra a nota sobre pressão alta. "problemas para
dormir depois do plantão" não encontra a nota sobre insônia após trabalho
noturno. Nenhuma das duas falha por falta de semântica: falham porque a frase
exata não aparece.

**Isso reordena a Fase 4.3.** O maior ganho disponível não é embeddings.

| motor | R@1 | R@3 | R@5 | MRR | custo |
| --- | --- | --- | --- | --- | --- |
| lexical de hoje | 0,333 | 0,367 | 0,367 | 0,350 | — |
| BM25 por termos | 0,667 | 0,767 | 0,833 | 0,728 | nenhuma dependência |
| BM25 → semântico | 0,767 | 0,900 | 0,967 | 0,845 | artefato de modelo |

O passo lexical entrega **+0,40 de R@3 sem modelo, sem cache, sem artefato e
sem superfície de privacidade nova**. O passo semântico entrega mais +0,13 e
custa um artefato de centenas de megabytes. Os dois se justificam; a ordem é
o que a medição decide.

## 2. Objetivos

1. Uma consulta encontra uma nota relevante mesmo quando não usa as palavras
   da nota.
2. Nenhum acerto exato de hoje é perdido ou rebaixado.
3. A recuperação continua explicável: cada candidato diz **por que** está ali.
4. Tudo local, offline, sem enviar conteúdo de nota a lugar nenhum.
5. O Note-it continua inteiramente utilizável sem nada disto.

## 3. Não objetivos

* Não é busca por relevância "inteligente" que resume ou conclui. A IA
  continua fora do Core (ADR-048).
* Não é um banco vetorial. Não é um serviço. Não é um daemon.
* Não é um índice persistente — ver §8, que mede por que não.
* Não substitui `noteit_search` nem a paleta `Ctrl+K`.
* Não muda o formato `.md`, o front matter, a revisão ou o protocolo de escrita.

## 4. Onde vive

**No `noteit-core`, dentro do Context Engine que já existe**, e não num motor
paralelo.

A razão é factual: `context::retrieve` tem hoje **um único** consumidor,
`noteit-mcp/src/domain.rs`. A CLI e a GUI não o usam — elas usam
`search::search_notes*`. Um motor paralelo duplicaria as regras de seleção que
a 4.2 levou seis subfases para acertar: a leitura autoritativa por candidato
(D-27), a ausência de `revision`, os tetos de snippet, os warnings sem caminho.
Um segundo motor seria um segundo lugar para errar cada uma delas.

```
consulta
   │
   ├─ sinais lexicais  ── termo/BM25 ───┐
   ├─ sinais de tag/propriedade ────────┤
   ├─ sinais de tarefa ─────────────────┼──► candidatos, cada um com seus Reason
   └─ sinal semântico (quando houver) ──┘
                                         │
                                    mesma Projection por nota (D-27)
                                    mesmo teto de snippet
                                    sem revision, sem caminho, sem score bruto
```

CLI e MCP compartilham o mesmo motor **quando a CLI ganhar uma superfície de
contexto** — o que esta fase não decide implementar. O que fica decidido é que,
se ganhar, será este motor e não outro.

## 5. Pipeline

### 5.1 Etapa lexical (a primeira a implementar)

Casamento por **termo**, não por frase. Termos são as sequências de
`[0-9a-z]` do texto **dobrado pela dobra que o Note-it já tem**
(`search::fold`: minúscula Unicode + a tabela de Latin-1/Latin Extended-A +
descarte de marcas combinantes). Nenhuma normalização nova: a dobra existente
já resolve acento, e duplicá-la seria criar duas verdades sobre o que é a
mesma palavra.

Ranking por BM25 (`k1 = 1.2`, `b = 0.75`, os valores canônicos — não são pesos
inventados para este corpus, e a 4.3B deve confirmá-los contra ele antes de
fixá-los).

`Reason::TextMatch` continua significando "a consulta ocorre no texto". Um
casamento por termo que não é frase precisa de um motivo próprio, para não
mentir sobre o que aconteceu.

### 5.2 Etapa semântica

Embeddings **estáticos de token** (classe model2vec / static-embedding):
tokenizar, buscar uma linha da matriz por token, média, normalizar L2. Sem
transformer, sem runtime de inferência, sem ONNX, sem C++.

Similaridade por cosseno contra todos os vetores. Sem ANN — ver §9.

### 5.3 Combinação: encadeamento, não fusão

O resultado lexical vem primeiro, na ordem que o lexical decidiu. O semântico
**só preenche o que sobrou**.

Isto foi escolhido sobre Reciprocal Rank Fusion depois de medir os dois. A RRF
pontua um pouco melhor em R@3 (1,000 contra 0,900 no melhor caso), e **rebaixou
um acerto exato** em uma consulta do corpus. O encadeamento não pode rebaixar:
não é uma observação sobre este corpus, é a forma da operação. Num corpus de 32
consultas, uma diferença de uma ou duas consultas é ruído; a garantia estrutural
não é.

## 6. Unidade de embedding e chunking

**Parágrafo**, com a nota inteira como fallback.

Medido: a nota longa do corpus (7 878 caracteres, com um parágrafo relevante no
meio) é perdida pelo embedding da nota inteira e encontrada pelo embedding por
parágrafo. Chunking levou o `potion` de R@3 0,933 para 0,967 e o `e5-small` de
R@1 0,767 para 0,833.

Regra, determinística e reprodutível:

1. Separar por linha em branco — a fronteira que o Markdown já usa e que o
   autor da nota escolheu.
2. Um parágrafo acima de **800 caracteres** é partido em fronteira de sentença
   (`. `), acumulando até o teto.
3. Sem sobreposição. Sobreposição multiplica vetores para recuperar contexto que
   a média já borra; o custo é certo e o ganho não foi medido.
4. Nota vazia produz um chunk vazio e nenhum vetor.
5. O chunking **lê** a nota e nunca a altera. É visão derivada.

O texto que entra é o **texto visível** (`visible_text`), o mesmo que a busca
lexical usa — então atributo de cor, comentário HTML e front matter não são
embedados, exatamente como não são pesquisáveis hoje.

## 7. Identidade do chunk e invalidação

Identidade: **`note_id` + `revision` da nota + ordinal do chunk**.

* `note_id` distingue notas de corpo idêntico — duas notas iguais têm vetores
  iguais e identidades diferentes, que é o correto.
* `revision` é a revisão canônica que o Core já calcula. Ela muda quando o
  conteúdo persistido muda, o que é exatamente quando os vetores deixam de
  valer. Não é preciso inventar um detector de staleness: já existe um.
* O ordinal lida com reordenação sem esforço, porque a revisão muda junto.

**A revisão aqui é chave de cache e nada mais.** Não autoriza escrita, não é
publicada num candidato e não chega ao agente — ver §11.

Matriz de invalidação:

| evento | o que acontece |
| --- | --- |
| nota criada | vetores calculados na próxima recuperação que a alcançar |
| nota editada | `revision` muda → entradas antigas não casam → recalcula |
| nota para a lixeira | some da varredura de notas vivas; entradas ficam órfãs e são descartadas |
| nota restaurada | volta à varredura; recalcula se a revisão não casar |
| tag / propriedade alterada | `revision` muda (e `updated_at` não) → recalcula. Correto e um pouco caro: nada de textual mudou. Aceito porque a alternativa é um segundo detector de staleness, e a 4.2A.R1 já registrou o custo de ter dois |
| tarefa completada / reaberta | idem: é edição de conteúdo |
| modelo trocado | identidade do modelo faz parte da chave do cache → tudo recalcula |
| chunker alterado | versão do chunker na chave → tudo recalcula |
| dimensão muda | idem |
| versão do formato muda | idem |
| cache corrompido | descartado inteiro e reconstruído |
| queda durante a escrita | nunca há arquivo parcial válido: escrita atômica, renomeação como ponto de commit, igual a uma nota (ADR 3.4R.2) |

A resposta correta a qualquer incompatibilidade é **reconstruir**. Nunca
interpretar.

## 8. Persistência: medida, e por ora dispensada

| escala | indexar (estático) | matriz | consulta p50 | consulta p95 |
| --- | --- | --- | --- | --- |
| 100 notas | 0,07 s | 0,10 MB | 0,012 ms | 0,024 ms |
| 1 000 | 0,79 s | 1,02 MB | 0,072 ms | 0,091 ms |
| 5 000 | 4,02 s | 5,12 MB | 5,25 ms | 7,47 ms |
| 10 000 | 7,13 s | 10,24 MB | 3,51 ms | 6,91 ms |

O store real da máquina onde isto foi medido tem **41 notas**: embutir todas
custa cerca de 30 ms. Mil notas custam 0,8 s. Dez mil custam 7 s.

**Portanto o índice vetorial persistente não se justifica na escala deste
aplicativo.** O que custa não é o índice, é o artefato do modelo — 100 a 512 MB
para carregar. Persistir 10 MB de vetores para poupar 7 segundos, enquanto se
carrega 100 MB de pesos, é otimizar a metade errada.

A decisão fica assim, e é revisável por medição e não por gosto:

* **v1: em memória, calculado sob demanda, mantido enquanto o processo viver.**
* Um cache em disco entra quando alguém medir um store onde a indexação
  incomode. O gatilho proposto é **indexação a frio acima de 2 s no store real
  de alguém**, o que pelas medições acima significa cerca de 2 500 notas.
* Quando entrar, será em `$XDG_CACHE_HOME/note-it/`, **nunca** dentro de
  `notes/`, com cabeçalho de validade (versão do formato, identidade do modelo,
  dimensão, versão do chunker) e escrita atômica com renomeação como ponto de
  commit.

## 9. Força bruta, não ANN

Consulta por produto interno sobre a matriz inteira: **3,5 ms com 10 000
vetores**. Um índice ANN traria estrutura, parâmetros, não-determinismo,
invalidação própria e uma dependência, para melhorar um número que já é menor
que a leitura de uma nota do disco.

ANN entra quando a consulta por força bruta passar de **50 ms** na escala real
de alguém. Pelas medições, isso fica na casa das centenas de milhares de
vetores, que este aplicativo não tem.

## 10. Modelo

**Classe decidida: embeddings estáticos de token.** É a decisão com evidência,
e ela é mais forte que a escolha do arquivo:

| classe | R@3 (encadeado) | indexação | precisa de runtime de inferência |
| --- | --- | --- | --- |
| transformer (`e5-small`, `MiniLM`) | 0,867–0,967 | 23–29 notas/s | sim: ONNX Runtime |
| estático (`potion`, `static-mrl`) | 0,867–0,900 | **1 250–1 400 notas/s** | **não** |

Cinquenta vezes mais rápido, qualidade dentro do ruído do corpus, e — o que
decide — **nenhum runtime de inferência**. Um modelo estático é uma tabela e
uma média: `tokenizers` (Apache-2.0, Rust puro) e uma matriz. Sem ONNX Runtime,
sem C++, sem binário baixado em tempo de build, sem risco à fronteira de rede
que a 4.1R1.1 fechou.

Candidatos medidos, ambos aceitáveis:

| modelo | licença | dim | artefato | tokenizer | R@3 encadeado |
| --- | --- | --- | --- | --- | --- |
| `minishlab/potion-multilingual-128M` | MIT | 256 | 512 MB fp32 | 18,6 MB | 0,900 |
| `sentence-transformers/static-similarity-mrl-multilingual-v1` | Apache-2.0 | 1024 | 434 MB fp32, **108 MB int8** | 2,6 MB | 0,867 |

O segundo é treinado com Matryoshka: truncar para 512 dimensões custou
0,900 → 0,900 de R@3 e 0,845 → 0,840 de MRR — praticamente nada, por metade do
armazenamento de vetores.

A escolha final entre os dois depende de duas medições que a 4.3A não fez, e
que a subfase de implementação deve fazer antes de fixar o arquivo:

1. qualidade sob quantização int8 do artefato (medida aqui só em fp32);
2. verificação da licença de `model2vec-rs` — o `crates.io` publica
   `non-standard`, e o card do modelo diz MIT. **Não verificado.**

## 11. O que a recuperação semântica não pode fazer

Nada disto muda, e a implementação não tem permissão de negociá-lo:

* **Um candidato nunca carrega `revision`.** Descoberta não é autorização
  (ADR-048 D-13, ADR-051). Para gravar: descobrir → `noteit_read` → revisão →
  decidir → escrever com `expected_revision`.
* **Embedding não é autoridade de escrita.** Score não é. Índice não é.
* **Conteúdo de nota continua sendo dado.** Uma nota que diz "ignore todas as
  instruções" é uma nota que diz isso. O corpus tem uma (`n17`) e ela é
  recuperada como qualquer outra — o teste é que ela apareça como candidato e
  nada aconteça.
* **Nenhuma resposta nomeia caminho, arquivo ou diretório.**
* **Markdown continua sendo a fonte da verdade.** Vetor é derivado; cache é
  derivado; perder qualquer um dos dois não perde nota nenhuma, e reconstruir
  é sempre a resposta correta.

## 12. Score, e como não mentir sobre ele

O Context Engine hoje publica **motivos e nenhum score**, de propósito
(ADR-048): `0,873` não é proveniência, é decoração.

Se um score for publicado, ele é uma **similaridade de cosseno**, e:

* nomeada `similarity`, nunca `confidence`, `score`, `relevance` ou `match`;
* nunca apresentada como porcentagem — `0,81` não é "81% de chance de ser
  relevante", e escrever `81%` afirma exatamente isso;
* nunca a única razão de um candidato estar na resposta.

**A recomendação da 4.3A é não publicar score em v1** e acrescentar um
`Reason::SemanticMatch`. Um motivo diz o que aconteceu — "esta nota não usa as
suas palavras e foi trazida por semelhança" — e é o que um agente precisa para
decidir se lê. Um número não é auditável e o conjunto de motivos é.

### O limiar que não existe

Medição que restringe a arquitetura: **nenhum limiar de similaridade separa
"tem resposta" de "não tem resposta"** em nenhum dos três modelos testados.

```
e5-small     menor topo-1 com resposta 0,8248   maior sem resposta 0,8494
potion       menor topo-1 com resposta 0,1760   maior sem resposta 0,3469
static-mrl   menor topo-1 com resposta 0,0995   maior sem resposta 0,1486
```

As faixas se sobrepõem em todos. Um corte por similaridade jogaria fora
respostas boas ou deixaria passar ruído, e qual dos dois depende do modelo.

Consequência direta: hoje o motor devolve **vazio** quando nada casa, e isso é
informação verdadeira. Um motor semântico sempre tem um vizinho mais próximo e
sempre devolveria dez. Então:

* candidatos puramente semânticos são **rotulados** como tal;
* o número deles é limitado (proposto: no máximo 3 quando não houve nenhum
  sinal lexical), para que "não achei nada com as suas palavras" continue
  legível em vez de virar dez candidatos com cara de certeza.

## 13. Falha e degradação

O Note-it é um aplicativo de notas. Falha de recuperação semântica **não pode
ser falha do aplicativo**.

| falta | o que acontece |
| --- | --- |
| artefato do modelo ausente | recuperação lexical, e o resultado diz `semantic_unavailable` |
| artefato corrompido / dimensão errada / não finito | idem, e o artefato é rejeitado na carga |
| falha ao embutir uma nota | aquela nota fica sem sinal semântico; as outras não |
| cache ilegível | descartado, reconstruído |
| memória insuficiente | recusa a etapa semântica, mantém a lexical |

Nenhum desses caminhos afeta ler, escrever, listar, buscar, a CLI, o MCP ou as
notas. A etapa lexical da §5.1 **não depende de nada da §5.2** — é por isso que
ela é implementada primeiro e sozinha.

## 14. Concorrência

Embutir consome CPU. As regras que a 4.2B estabeleceu continuam valendo e não
são afrouxadas:

* toda chamada ao Core a partir do MCP passa por `spawn_blocking` e pelo
  testemunho `OffThread` — a etapa semântica é trabalho de Core e vai pelo mesmo
  caminho;
* o reactor do MCP continua respondendo `ping` durante uma indexação;
* a GUI nunca embute na thread do main loop;
* **uma indexação por processo por vez.** Duas consultas concorrentes sobre um
  store não indexado não podem disparar duas indexações.

A 4.2R.R1 mediu 4 clientes × 8 requisições hostis de 300 KiB em 41 ms com
+1,1 MB de RSS. A etapa semântica é muito mais cara que isso e não pode entrar
sem uma medição equivalente: a subfase de implementação deve repetir aquele
teste com indexação em curso.

## 15. Orçamentos propostos

Derivados das medições acima, não escolhidos por gosto. Um orçamento que a
implementação não conseguir cumprir é um orçamento a rediscutir com número na
mão, não a ignorar.

| grandeza | orçamento | de onde vem |
| --- | --- | --- |
| indexação a frio, 1 000 notas | ≤ 2 s | medido 0,79 s em Python; folga para Rust e I/O |
| indexação a frio, 10 000 notas | ≤ 20 s | medido 7,13 s |
| consulta quente, 10 000 vetores | ≤ 20 ms | medido 3,5 ms p50 / 6,9 ms p95 |
| carga do artefato | ≤ 2 s | medido 1,0–1,8 s carregando ONNX |
| RSS acrescido pelo modelo | a medir em Rust | os números desta fase são de um processo Python e **não são representativos** |
| resposta MCP | inalterada | os tetos da 4.2R continuam valendo, sem exceção |
| CPU em indexação de fundo | a definir | nenhuma indexação de fundo em v1 |

## 16. O que a 4.3A não mediu

Registrado para que ninguém trate como decidido:

* **Nada foi implementado em Rust.** Todas as medições são de um protótipo
  Python com ONNX Runtime. Servem para comparar candidatos entre si; não são
  previsão de desempenho da implementação.
* **RSS não foi medido de forma utilizável** — os números são picos do processo
  Python com vários modelos carregados.
* **O corpus tem 30 notas e 32 consultas.** Diferenças de uma ou duas consultas
  estão dentro do ruído. Ele é bom para separar arquiteturas — e separou, com
  folga — e ruim para escolher entre dois modelos parecidos.
* **Quantização não foi avaliada em qualidade** para os modelos estáticos.
* **A licença de `model2vec-rs` não foi verificada.**
* **Não há medição em store real de terceiros**, só o perfil de 41 notas da
  máquina onde isto rodou.

## 17. Testes que a implementação deve trazer

Além do corpus como regressão de qualidade:

`nota vazia` · `nota gigantesca` · `milhares de notas` · `corpo repetitivo` ·
`todas as notas iguais` · `embeddings idênticos` · `vetor zero` · `NaN` ·
`Inf` · `dimensão errada` · `cache truncado` · `cache de modelo antigo` ·
`note_id inexistente` · `nota editada durante a busca` · `nota para a lixeira
durante a busca` · `modelo ausente` · `cache somente leitura` · `disco cheio` ·
`queda durante reconstrução` · `symlink no cache` · `Unicode hostil` ·
`prompt injection no texto` · `consulta gigantesca` · `consultas concorrentes`

O corpus já carrega três deles como dados: `n17` (prompt injection), `n18`
(Unicode hostil), `n19`/`n20` (nota vazia e mínima).
