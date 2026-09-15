# Medição do índice de relações — Fase 6.0.C

Data: 15/09/2026. Baseline: `6e36cdf8359c6e604305073ae7c2c11a6d54bd15`.

Esta medição decide arquitetura; ela não implementa o parser nem o índice do
produto. O protótipo ficou em `/tmp/noteit_relation_bench.py`, usou somente a
biblioteca padrão do Python e não entrou no workspace ou no commit. Todos os
stores foram criados sob um `TemporaryDirectory`; nenhum caminho XDG do Note-it
foi resolvido ou acessado.

## Ambiente e protocolo

- CPU: Intel Core i5-9300H, 4 cores/8 threads, 2,40–4,10 GHz, cache L3 de 8 MiB.
- RAM: 23 GiB; kernel Linux 7.2.4-arch1-2 x86_64.
- Runtime/perfil: CPython 3.14.7, execução normal, uma thread, biblioteca padrão.
- Protótipo descartável: SHA-256
  `708efdc4539ca8dafe8e8684bf4b7d1327f0258be863382e7499abf3ecd96d72`;
  comando: `python /tmp/noteit_relation_bench.py --corpus
  docs/link-syntax-corpus.json --output /tmp/noteit-relation-benchmark.json`.
- Corpus normativo: `docs/link-syntax-corpus.json`, SHA-256
  `96324d124d2b0557daa7c4ab69adc912327fe2fc5528f31f8a397a00e14a2f36`.
- Gate do extrator: 20 casos relevantes à carga passaram antes do benchmark:
  link simples/Unicode, seção, bloco, display, embed, múltiplos links, escape,
  vazios, code span, fences, front matter, comentário HTML, link/imagem
  Markdown, matemática, quebra de linha e barra desconhecida. Isso não o torna
  o parser da 6.A.2; somente evita medir uma regex que conte os contextos
  proibidos mais óbvios.
- Stores: 100, 1.000, 5.000 e 20.000 arquivos Markdown determinísticos, média
  de aproximadamente 2.208 bytes por nota. A distribuição repete 0, 2, 6 e 16
  referências, média 6 por nota; alvos dão a volta no próprio store. O mix
  contém link normal, seção, bloco, display e embed.
- Fórmula: nota `i` usa `count = (0,2,6,16)[i mod 4]`; referência `j` aponta a
  `(17*i + 97*j + 11) mod N`; `j mod 5` escolhe display, seção, bloco, embed ou
  link. Texto determinístico completa 2.048 caracteres de corpo; o front matter
  traz UUID derivado de `i` e `title: Nota i`.
- Rodadas: 2 warmups e 9 rodadas medidas por operação. A tabela publica mediana
  e p95 (nona/pior amostra). Cada preparação incremental reconstruiu uma cópia
  independente antes de medir somente a remoção/adição da origem.
- Full scan inclui enumerar e ler todos os arquivos, calcular proveniência
  SHA-256, extrair referências e construir origem→destinos, destino→origens e
  contexto mínimo. Rebuild apaga as estruturas conceituais e repete o mesmo
  caminho. A consulta sob demanda lê/extrai todo o store para um destino.

## Resultados

Tempos em milissegundos:

| Notas | Bytes médios | Referências | Full scan med./p95 | Backlink sob demanda med./p95 | Rebuild med./p95 |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 2.210,85 | 600 | 78,50 / 98,56 | 81,96 / 134,40 | 100,77 / 177,57 |
| 1.000 | 2.209,14 | 6.000 | 865,73 / 996,86 | 790,57 / 951,11 | 874,70 / 1.020,61 |
| 5.000 | 2.208,06 | 30.000 | 4.677,21 / 4.836,38 | 4.582,75 / 5.088,72 | 4.417,55 / 4.541,58 |
| 20.000 | 2.207,37 | 120.000 | 16.751,43 / 17.107,58 | 16.235,71 / 17.421,54 | 17.525,93 / 19.080,77 |

Atualização incremental, mediana/p95 em milissegundos:

| Notas | Editar (substituir origem) | Remover | Restaurar |
| ---: | ---: | ---: | ---: |
| 100 | 1,076 / 1,890 | 0,013 / 0,026 | 0,646 / 1,879 |
| 1.000 | 0,736 / 1,756 | 0,015 / 0,020 | 0,856 / 1,984 |
| 5.000 | 0,636 / 1,562 | 0,017 / 0,022 | 0,645 / 1,678 |
| 20.000 | 0,713 / 1,436 | 0,016 / 0,018 | 0,571 / 0,889 |

## Memória aproximada

`sys.getsizeof` recursivo, com deduplicação por identidade, sobre as quatro
estruturas Python. É uma aproximação conservadora do protótipo, não promessa de
layout Rust. O contexto guarda 96 caracteres por par origem/destino e domina o
custo. O pico RSS cumulativo do processo longo chegou a 292.360 KiB, mas inclui
runtime, estruturas temporárias e picos de todas as escalas; por isso não é
atribuído a cada índice.

Um par repetido na mesma nota colapsa para um contexto. Se a implementação
guardar toda ocorrência, esta aproximação deixa de valer e o teto de 64 MiB
continua: ocorrências terão de ser limitadas/projetadas, não o orçamento movido.

| Notas | origem→destinos | destino→origens | NoteRevision | contexto | Total |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 49,8 KiB | 25,3 KiB | 17,1 KiB | 178,9 KiB | 271,1 KiB |
| 1.000 | 496,5 KiB | 246,8 KiB | 164,1 KiB | 1,86 MiB | 2,74 MiB |
| 5.000 | 2,42 MiB | 1,18 MiB | 794,8 KiB | 9,16 MiB | 13,54 MiB |
| 20.000 | 9,77 MiB | 4,75 MiB | 3,10 MiB | 36,71 MiB | 54,33 MiB |

## Leitura dos números

A opção A, varredura sob demanda, deixa de ser interativa antes de 1.000 notas:
o p95 de abrir backlinks é 951 ms. Em 20.000 notas são 17,4 s. Repetir isso a
cada painel seria uma regressão visível; a cada tecla seria indefensável.

A opção B paga uma reconstrução equivalente ao full scan uma vez, fora do
caminho de edição, e depois atualiza uma origem abaixo de 2 ms p95 em todas as
escalas. O custo Python aproximado de 54,33 MiB para 20.000 notas/120.000
referências cabe no orçamento de 64 MiB definido pela ADR-066, mas exige que o
contexto seja limitado e medido de novo na implementação Rust.

Persistir não melhora a operação incremental e somente esconderia uma
reconstrução de até 19,1 s p95. Esse custo pode ser assíncrono e progressivo;
não justifica uma segunda fonte no store, formato, migração ou manifesto de
backup. A decisão é índice derivado em memória, descartável e reconstruível.

## Reprodutibilidade e limites

O gerador, extrator e benchmark usados nesta execução permaneceram descartáveis
por exigência da fase. A 6.A.2 deve implementar e provar os 110 casos completos;
a 6.A.4 deve transformar os orçamentos da ADR-066 em benchmarks Rust. Python
amplifica custo e memória em relação a uma estrutura Rust compacta, mas isso
não enfraquece a comparação: a varredura e o update usam o mesmo extrator, e a
diferença medida é de três a quatro ordens de grandeza nas escalas maiores.

A carga mede relações sintáticas por nome e todos os alvos existem; não modela
o custo futuro de resolução para UUID, ambiguidade nem nota ilegível. Isso
subestima o rebuild de produção e torna a rejeição do full scan conservadora.
A 6.A.4 deve medir novamente em Rust com destinos resolvidos, não resolvidos e
ambíguos, uma nota ilegível e contexto por ocorrência. Os tempos Python são
evidência da decisão; os testes Rust afirmam os orçamentos, não estes tempos.
