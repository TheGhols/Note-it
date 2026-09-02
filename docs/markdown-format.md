# Especificação do formato Markdown

Cada post-it do Note-it é armazenado como um arquivo Markdown (`.md`) válido e legível por humanos, nomeado com um UUID (por exemplo, `550e8400-e29b-41d4-a716-446655440000.md`).

## Estrutura de arquivo

```md
---
note_it:
  version: 1
  id: "550e8400-e29b-41d4-a716-446655440000"
  color: "yellow"
  paper_type: "lined"
  paper_intensity: "subtle"
  font_size: 16
  created_at: "2026-08-26T14:00:00Z"
  updated_at: "2026-08-26T14:05:00Z"
tags:
  - Medicina
  - Urgência
properties:
  tipo: estudo
  fonte: Harrison
---

# Meeting Notes

- [ ] Complete project setup
- [x] Create documentation

Remember to check <u>underlined points</u> and <span data-note-it-color="#D32F2F" style="color:#D32F2F">urgent tasks</span>.
```

## Metadados semânticos

`note_it` é reservado para metadados da aplicação. `tags` e `properties` são metadados semânticos criados pelo usuário e permanecem fora do corpo do Markdown. Ambos são opcionais: notas herdadas ou notas novas sem metadados omitem ambas as chaves, lidas como `tags = []` e `properties = {}`, nunca sofrendo migrações em massa.

Tags são strings YAML. O Note-it remove espaços excedentes (trim), aceita e remove um caractere `#` inicial de conveniência, rejeita valores vazios, com caracteres de controle, multilinha ou excessivamente longos, e mantém no máximo 32 tags com até 64 caracteres Unicode cada. A identidade semântica utiliza a mesma normalização (minúsculas e remoção de diacríticos latinos) da busca textual, de modo que `Medicina`, `medicina` e `MEDICINA` representam a mesma tag, assim como `Urgência` e `urgencia`. A primeira grafia fornecida pelo usuário é preservada para exibição visual.

Propriedades são um mapeamento YAML de chave textual para valor textual. Até 32 entradas são aceitas; uma chave limpa contém de 1 a 64 caracteres Unicode e um valor de linha única possui no máximo 512 caracteres. As chaves usam a mesma identidade insensível a maiúsculas e acentos; portanto, `Status` e `status` não podem coexistir, sendo serializadas em ordem determinística. A versão V1 deliberadamente não possui objetos aninhados, relações, fórmulas ou valores computados.

Alterar metadados semânticos não altera `created_at` nem `updated_at`; o último continua significando a edição textual mais recente do corpo. Se houver texto pendente no editor quando os metadados forem confirmados, ambos são gravados em um único candidato atômico e `updated_at` avança porque o texto mudou, não por causa dos metadados.

Valores YAML desconhecidos de nível raiz são preservados semanticamente na resserialização. Comentários YAML, âncoras e formatação exata não fazem parte do modelo de valores do serde e podem ser normalizados em um salvamento real. Apenas abrir e fechar uma nota intocada não executa gravação, mantendo seus bytes idênticos.

## Linhas em branco finais não são conteúdo

Uma nota armazenada termina com uma única quebra de linha (`\n`), da mesma forma que qualquer ferramenta padrão grava arquivos. Esse terminador não faz parte do conteúdo da nota, assim como nenhuma linha em branco que o anteceda: o Markdown não atribui significado a linhas em branco finais, e o próprio editor do Note-it encerra documentos terminados em bloco (uma lista, um callout ou um bloco de código) com uma linha em branco, enquanto documentos terminados em parágrafo não recebem nenhuma.

Assim, a mesma nota possui várias grafias equivalentes válidas. O Note-it compara e armazena uma forma canônica, com novas linhas finais removidas, e adiciona o terminador ao salvar. É isso que torna a abertura de uma nota uma leitura pura: um `.md` gravado por outro editor ou uma nota terminando em lista não é reescrito nem tem seu `updated_at` alterado ao ser aberto.

Espaços finais em linhas de texto são preservados — dois espaços no final de linha representam a quebra de linha forçada (hard break) do Markdown e constituem conteúdo.

## Sintaxe de blocos

Tudo o que o Note-it grava é Markdown padrão. Nada abaixo é uma extensão proprietária do formato de arquivo: outro editor abre a nota e enxerga cercas de código, citações e comentários HTML normais, e o GitHub renderiza um callout como um alerta.

### Blocos de código cercados (Fenced Code Blocks)

````md
```python
def soma(a, b):
    return a + b
```
````

O identificador de linguagem é transportado em ambas as direções **exatamente como foi escrito**. Nunca é reescrito, normalizado ou descartado:

- um bloco sem linguagem permanece sem linguagem, sem receber padrão automático;
- uma linguagem sem gramática disponível — `brainfuck`, um erro de digitação ou algo mais recente que esta versão — mantém sua grafia e simplesmente não recebe destaque de sintaxe;
- um alias permanece um alias: uma nota contendo ` ```js ` continua como ` ```js ` após salvar, mesmo sendo destacada como JavaScript.

O conteúdo é literal. Nada em seu interior é interpretado: sem formatação inline, sem substituição tipográfica e sem HTML — `<script>` dentro de um bloco de código são os caracteres `<`, `s`, `c`… e chegam ao documento como texto puro.

A cerca de fechamento é sempre dimensionada para ser maior que a mais longa sequência contígua de crases dentro do bloco, garantindo que uma nota contendo exemplos de Markdown seja gravada integralmente sem truncamento.

O destaque de sintaxe é **apenas apresentação visual**. É renderizado como decorações do editor sobre os mesmos caracteres; o arquivo em disco é uma cerca simples sem marcações extras. Dezesseis gramáticas são suportadas: `plaintext`, `bash`, `javascript`, `typescript`, `json`, `html`/`xml`, `css`, `markdown`, `python`, `rust`, `c`, `cpp`, `java`, `sql`, `yaml` e `toml` — com os aliases correspondentes reconhecidos.

### Alertas (Callouts)

A sintaxe é compatível com os alertas do GitHub, lidos pelo Obsidian como callouts:

```md
> [!NOTE]
> Um parágrafo.
>
> - e uma lista, se quiser
```

`NOTE`, `TIP`, `IMPORTANT`, `WARNING` e `CAUTION` são reconhecidos independentemente de maiúsculas/minúsculas; a forma canônica em maiúsculas é a gravada de volta. Um callout é um blockquote que transporta um tipo, suportando tudo o que um blockquote suporta — parágrafos, listas e blocos aninhados.

O marcador deve estar isolado na primeira linha. Qualquer outra variação não é tratada como callout e é **preservada como o blockquote comum que já é**, com seu texto intacto:

| Escrito | Lido como |
| --- | --- |
| `> [!NOTE]` + corpo | um callout NOTE |
| `> [!FOO]` + corpo | um blockquote cuja primeira linha é `[!FOO]` |
| `> [!NOTE] com texto` | um blockquote comum, incluindo o marcador |
| `> [!NOTE` | um blockquote comum, incluindo o marcador |

A degradação nunca perde conteúdo. Um colchete literal `[` é escapado como `\[` na saída, que é como o Markdown representa o caractere, tornando o resultado estável a partir de então.

### Citações em bloco (Blockquotes)

Um blockquote comum permanece um blockquote comum:

```md
> uma citação
```

Nunca é promovido automaticamente a callout e é gravado sem qualquer decoração — sem atributos, classes ou tags HTML.

### Cálculos

Uma linha iniciada por `=` é um cálculo e uma linha na forma `nome := …` é uma declaração de variável. Ambas são **texto Markdown comum**:

```md
preco := 120
quantidade := 3
= preco * quantidade
```

Outro editor abre isso e enxerga três linhas de texto comum. O Note-it desenha `360` ao lado da terceira linha como uma decoração visual do editor — o mesmo mecanismo usado no realce de sintaxe — e **não grava nada adicional no arquivo**. Nenhum resultado, marcador ou atributo é persistido no arquivo; a nota nunca é reescrita ao ser recalculada e sua data de modificação não é alterada por recálculos.

A gramática completa está descrita em `docs/features.md`. No que tange ao formato do arquivo:

- cálculos são reconhecidos **apenas em parágrafos comuns**. Títulos, listas, tarefas, citações, callouts, blocos de código, comentários e spans de código inline permanecem como o texto original que são;
- o caractere `*` em um cálculo é escapado como `\*` na gravação em disco, que é o escape padrão do Markdown para asteriscos literais em prosa, sendo lido de volta como `*`;
- resultados são recalculados ao carregar. Uma nota cujas expressões não mudaram permanece byte a byte idêntica após ser aberta, recalculada e fechada.

Uma conversão de unidades segue a mesma regra, com uma unidade em cada lado:

```md
distancia := 10
= distancia km em m
```

`em` é a palavra-chave de conversão. O arquivo armazena apenas essas duas linhas; `10000 m` é exibido ao lado da segunda linha como decoração visual e nunca gravado no arquivo. As unidades são palavras comuns no texto, de modo que outro editor exibe a nota exatamente como está armazenada, e um `.md` escrito externamente é convertido assim que o Note-it o abre.

### Comentários

```md
<!-- lembrete que não aparece na nota -->
```

Um comentário é armazenado no arquivo e exibido no editor como um pequeno bloco identificado, permitindo leitura, edição e remoção — mas não faz parte do texto legível da nota e nunca é renderizado como conteúdo final.

É dado, nunca marcação executável: seu conteúdo é texto puro, e `<script>` em seu interior são apenas caracteres literais. Uma sequência `-->` digitada dentro de um comentário é gravada como `--&gt;` para evitar o fechamento prematuro da tag HTML; na leitura, é recuperada exatamente como digitada.

Uma tag `<!--` não finalizada não é tratada como comentário, sendo escapada como `&lt;!--` para que todo o conteúdo subsequente seja preservado, sem ser engolido até o fim do arquivo.

Os metadados de tarefa do próprio Note-it (`<!-- note-it:completed_at=… -->`) permanecem como sempre foram: um comentário inline absorvido pela tarefa em sua linha.
