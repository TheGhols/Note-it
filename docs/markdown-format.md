# Especificação do formato Markdown

Cada post-it Note-it é armazenado como um arquivo Markdown (`.md`) válido e legível por humanos, nomeado usando um UUID (por exemplo, `550e8400-e29b-41d4-a716-446655440000.md`).

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

`note_it` está reservado para metadados de aplicativos. `tags` e `properties` são metadados semânticos de autoria do usuário e permanecem fora do corpo Markdown. Ambas são opcionais: uma nota herdada ou nova que não possui nenhuma omite ambas as chaves, é lida como `tags = []` e `properties = {}` e nunca é migrada em massa.

Tags são strings YAML. Note-it corta o valor, aceita e remove uma conveniência `#`, rejeita valores vazios/controle/multilinhas ou muito longos e mantém no máximo 32 tags com no máximo 64 caracteres Unicode cada. A identidade usa a mesma dobragem em letras minúsculas + diacríticos latinos da pesquisa corporal, então `Medicina`, `medicina` e `MEDICINA` são uma tag, assim como `Urgência` e `urgencia`. A primeira grafia humana fornecida é mantida para exibição.

As propriedades são um mapeamento YAML da chave textual para o valor textual. São aceitas até 32 inscrições; uma chave aparada tem de 1 a 64 caracteres Unicode e um valor de linha única tem no máximo 512. As chaves usam a mesma identidade que não diferencia maiúsculas de minúsculas/acentos, portanto, `Status` e `status` não podem coexistir e a serialização os ordena deterministicamente. V1 deliberadamente não possui objetos, relações, fórmulas ou valores computados aninhados.

A alteração dos metadados semânticos não altera `created_at` ou `updated_at`; este último continua significando a última edição do corpo textual. Se o texto estiver pendente quando os metadados forem confirmados, ambos serão escritos em um candidato atômico e `updated_at` se moverá porque o texto mudou, não porque os metadados mudaram.

Valores YAML de nível superior desconhecidos são preservados semanticamente na resserialização. Comentários YAML, âncoras e formatação exata não fazem parte do modelo de valor do serde e podem ser normalizados por um salvamento real. Apenas abrir e fechar uma nota intocada não escreve nada, portanto seus bytes permanecem idênticos.

## As linhas em branco finais não são conteúdo

Uma nota armazenada termina com uma única nova linha, da mesma forma que qualquer outra ferramenta grava um arquivo. Esse terminador não faz parte da nota, nem qualquer linha em branco antes dela: Markdown não dá sentido às linhas em branco finais, e o próprio editor de Note-it termina um documento que termina em um bloco - uma lista, um texto explicativo, um bloco de código - com um, enquanto um documento que termina em um parágrafo não recebe nenhum.

Portanto, a mesma nota possui várias grafias igualmente válidas. Note-it compara e armazena um formato canônico, com as novas linhas finais removidas, e grava o terminador novamente ao salvar. É isso que torna a abertura de uma nota uma leitura: um `.md` escrito por outro editor, ou qualquer nota que termine em uma lista, não é reescrita e não tem seu `updated_at` movido ao ser aberto.

Os **espaços** finais são deixados em paz — dois deles são a quebra de linha rígida de Markdown e são conteúdo.

## Sintaxe de bloco

Tudo o que Note-it escreve é ​​normal Markdown. Nada abaixo é uma extensão privada do formato de arquivo: outro editor abre uma nota e vê limites de código, citações em bloco e comentários HTML, e GitHub renderiza uma chamada como um alerta.

### Blocos de código cercados

````md
```python
def soma(a, b):
    return a + b
```
````

O identificador de idioma é executado em ambas as direções **exatamente como está escrito**. Nunca é reescrito, normalizado ou descartado:

- uma cerca sem idioma permanece sem idioma e não recebe inadimplência;
- uma linguagem sem gramática disponível — `brainfuck`, um erro de digitação, algo mais recente que esta versão — mantém a ortografia e simplesmente não é destacada;
- um alias permanece um alias. Uma nota dizendo ` ```js ` is still ` ```js ` após salvar, embora esteja destacada como JavaScript.

O conteúdo é literal. Nada dentro é interpretado: nenhuma formatação embutida, nenhuma substituição tipográfica e nenhum HTML — `<script>` dentro de um bloco contém os cinco caracteres `<`, `s`, `c`… e chega ao documento como texto.

A cerca de fechamento é sempre maior que a sequência mais longa de crases dentro do bloco, portanto, uma nota contendo um exemplo Markdown é escrita inteira.

O destaque é **apenas apresentação**. É desenhado como cenário do editor sobre os mesmos personagens; o arquivo armazenado é uma cerca simples, sem marcação. Dezesseis gramáticas são carregadas - `plaintext`, `bash`, `javascript`, `typescript`, `json`, `html`/`xml`, `css`, `markdown`, `python`, `rust`, `c`, `cpp`, `java`, `sql`, `yaml` e `toml` - com os aliases aos quais cada uma delas já responde.

### Alertas (callouts)

A sintaxe são os alertas de GitHub, que a Obsidian lê como textos explicativos:

```md
> [!NOTE]
> Um parágrafo.
>
> - e uma lista, se quiser
```

`NOTE`, `TIP`, `IMPORTANT`, `WARNING` e `CAUTION` são reconhecidos, em qualquer caso; a forma canônica maiúscula é o que é escrito de volta. Uma frase de destaque é uma citação em bloco que carrega um tipo, portanto, contém tudo o que uma citação em bloco contém - parágrafos, listas, blocos aninhados.

O marcador deve ficar sozinho na primeira linha. Qualquer outra coisa não é uma chamada e é **deixada como a citação que já é**, com seu texto intacto:

| Escrito | Leia como |
| --- | --- |
| `> [!NOTE]` + corpo | uma chamada de NOTA |
| `> [!FOO]` + corpo | uma citação cuja primeira linha é `[!FOO]` |
| `> [!NOTE] com texto` | uma citação, marcador e tudo |
| `> [!NOTE` | uma citação, marcador e tudo |

Degradar nunca custa conteúdo. Um literal `[` é escapado como `\[` no caminho de volta, que é como Markdown escreve um, e o resultado é estável a partir de então.

### Citações em bloco

Uma citação comum permanece uma citação comum:

```md
> uma citação
```

Ele nunca é promovido a um texto explicativo por si só e é escrito de volta sem qualquer tipo de decoração - sem atributos, sem classes, sem HTML.

### Cálculos

Uma linha que começa com `=` é um cálculo e uma linha no formato `nome := …` é uma declaração. Ambos são **texto Markdown comum**, e esse é o ponto principal:

```md
preco := 120
quantidade := 3
= preco * quantidade
```

Outro editor abre isto e vê três linhas de prosa, porque é isso que são. Note-it desenha `360` ao lado do terceiro como uma decoração do editor - o mesmo mecanismo usado pelo realce de sintaxe - e **não escreve nada**. Nenhum resultado, nenhum marcador, nenhum atributo chega ao arquivo, portanto uma nota nunca é reescrita ao ser recalculada e sua data de modificação nunca muda para uma.

A gramática completa está em `docs/features.md`. O que importa para o formato do arquivo:

- o cálculo é lido **apenas parágrafos simples**. Um título, uma lista, uma tarefa, uma cotação, um texto explicativo, um bloco de código, um comentário e um intervalo de código embutido são todos deixados como o texto que são;
- `*` em um cálculo é escapado como `\*` na saída, que é como Markdown escreve um asterisco literal em prosa e lê de volta como `*`. Esta é a regra existente do serializador para qualquer prosa, não algo introduzido por cálculos;
- os resultados são recalculados na carga. Uma nota cujas expressões não foram alteradas é idêntica em bytes após ser aberta, recalculada e fechada.

Uma conversão é a mesma coisa com uma unidade de cada lado:

```md
distancia := 10
= distancia km em m
```

`em` é a palavra-chave de conversão. O arquivo contém essas duas linhas e nada mais; `10000 m` é desenhado ao lado do segundo e nunca escrito. As unidades são palavras comuns em prosa comum, então outro editor mostra a nota exatamente como ela está armazenada, e um `.md` escrito em outro lugar converte no momento em que Note-it a abre.

### Comentários

```md
<!-- lembrete que não aparece na nota -->
```

Um comentário é armazenado no arquivo e mostrado no editor como um pequeno bloco rotulado, para que possa ser lido, editado e removido — mas não faz parte do que a nota diz e nunca é renderizado como conteúdo.

São dados, nunca marcação: o que ele contém é texto, e um `<script>` dentro de um tem cinco caracteres. Um `-->` digitado em um comentário é escrito como `--&gt;`, porque a sequência literal fecharia o comentário mais cedo e espalharia o resto da nota; ele lê de volta como o que foi digitado.

Um `<!--` não terminado não é um comentário. Ele escapa para `&lt;!--` para que tudo depois sobreviva, em vez de ser engolido até o final do arquivo.

Os próprios metadados de tarefa de Note-it (`<!-- note-it:completed_at=… -->`) permanecem o que sempre foram: um comentário embutido absorvido pela tarefa em sua linha.
