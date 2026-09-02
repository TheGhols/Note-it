# Funcionalidades

## Modos de janela e camada

Note-it aproveita Wayland Layer Shell para fornecer três modos de superfície distintos:

1. **Modo Desktop (camada `bottom`):**
   - As superfícies post-it permanecem fixadas acima do papel de parede da área de trabalho, mas atrás das janelas do aplicativo.
   - Modo de teclado não intrusivo para evitar roubar o foco durante a navegação normal pela janela.

2. **Modo de sobreposição (camada `overlay`):**
   - Superfícies post-it aparecem acima de todos os aplicativos ativos, incluindo janelas maximizadas e em tela cheia.
   - O foco interativo está habilitado para edição rápida.

3. **Modo Oculto:**
   - As superfícies são desanexadas/ocultadas enquanto o daemon de segundo plano permanece pronto para ativação instantânea.

## Cabeçalho da nota

- Uma nota expandida mantém um cabeçalho sobreposto e não pinta nada até que seja solicitado. Mover o ponteiro para a faixa na parte superior da nota revela os controles ao longo de cerca de 120 ms; sair da barra permite que eles recuem. O foco do teclado dentro do cabeçalho, um painel de ação rápida aberto e uma nota recolhida mantêm o chrome por conta própria.
- O editor reserva apenas essa faixa — `--note-chrome-gutter`, e não a altura total do barra — para que a nota comece perto de sua borda superior. A faixa é a única parte da superfície que é sempre um alvo do ponteiro e é exatamente o preenchimento superior do editor, portanto, nenhuma linha de texto fica abaixo dela: a primeira linha permanece clicável, selecionável e endereçável por cursor em qualquer lugar. Enquanto o chrome está oculto, os controles não recebem nenhum evento de ponteiro, portanto, um botão invisível nunca pode reivindicar um clique destinado ao texto abaixo dele.
- **Ações rápidas:** seis botões somente ícones, cada um abrindo um painel que o menu já possui — **Cor da nota**, **Tamanho do texto**, **Cor do texto**, **Marca-texto**, **Blocos** e **Buscar**. Nenhum deles tem lógica própria; eles são uma segunda maneira de entrar no mesmo painel e no mesmo manipulador. Eles ficam ocultos enquanto a nota está recolhida.
  - Seus desenhos são SVG embutidos escritos em `index.html` no momento da construção a partir de seis arquivos no
coleção de ícones fornecida — `bucket`, `larger-text`, `text`, `edite`, `Category` e `Search`.
Esses seis são os únicos que a compilação lança e cada um é a única fonte de seu ícone.
Nada é buscado: o próprio `default-src 'self'` da página bloqueia uma solicitação de imagem para uma máscara CSS
ou um URL `data:`, razão pela qual os ícones mascarados anteriores apareciam em branco em WebKitGTK.
  - Cada forma herda `currentColor` com força total, então um arquivo serve todos os sete papéis e
ambos os temas de interface e limpa 3:1 contra cada um deles.
- **Clipper (clip de papel):** um sétimo ícone na barra, entre **Buscar** e o timer, que abre imediatamente o seletor de imagens — o mesmo seletor, a mesma importação e o mesmo diretório de ativos de *☰ › Mídia › Inserir imagem…*, que fica exatamente onde está. É o único botão do cabeçalho que faz alguma coisa em vez de abrir um painel, e colocar uma imagem em uma nota é a coisa mais comum que alguém faz na seção Mídia, então um painel na frente seria um clique para não ler. Seu desenho é SVG embutido da mesma coleção (`attach-svgrepo-com5`), fica oculto enquanto a nota está recolhida como o seis e não há atalho de teclado para ele.
  - É também o único controle que cede em uma nota mais estreita que 300 px: a barra tem um formato rígido
orçamento em `MIN_NOTE_WIDTH`, e com o relógio e o indicador de captura nele, algo precisa
rende antes que o cruzamento próximo o faça. O clipe de papel é um atalho para o que o cardápio ainda oferece em
cheio, então perdê-lo custa um clique e nada mais - onde perder ☰, o relógio, o indicador ou
a cruz custaria um controle sem nenhum outro lugar para estar.
- **Menu Configurações (`☰`):**
  - Um botão de três linhas à esquerda do cabeçalho abre um pequeno popover ancorado na barra.
  - Entries: **Tipo de papel**, **Intensidade**, **Dados**, **Zoom da nota**, **Interface**,
**Tema**, **Camada** e **Recolher nota** / **Expandir nota**. As ações rápidas de formatação não são repetidas aqui - uma
função, um lugar para alcançá-lo - mas os painéis que eles abrem são os do próprio menu.
  - O menu mostra o papel atual, intensidade, zoom, tema e camada em suas próprias linhas, portanto, nenhum
deles depende de abrir um submenu ou conhecer um atalho.
  - Resumindo, o popover é limitado à altura restante do WebView e rola verticalmente;
uma nota grande mantém o menu original de altura natural.
  - Fecha com clique externo, `Escape`, ou seleção de uma entrada; existe apenas um popover por nota.
  - O botão e o popover ficam fora da região de arrastar, portanto, usá-los nunca move a nota.
- **Dica de informações de observação:**
  - Posicionar o cursor na área livre do cabeçalho por aproximadamente 450 ms mostra a criação da nota e
datas de modificação em pt-BR `dd/MM/aaaa HH:mm`.
  - A dica de ferramenta nunca pega o ponteiro (`pointer-events: none`) e é descartada deixando o
barra, clicando, arrastando ou abrindo o menu.
- **Recolher/Expandir:**
  - Recolher reduz a nota à sua barra de cabeçalho; o editor fica oculto, nunca desmontado, então o
o conteúdo, a formatação e a instância Tiptap são preservados.
  - A largura e a altura expandidas são registradas antes do recolhimento e restauradas na expansão, em
qualquer que seja a posição em que a barra recolhida foi deixada.
  - Uma nota recolhida ainda pode ser arrastada; o redimensionamento não estará disponível até que seja expandido novamente.
  - Seu cabeçalho permanece visível e nomeia a nota a partir da primeira linha de conteúdo útil. Um marcador de rumo
é retirado para apresentação, uma nota vazia diz **Nota sem título**, e nomes longos terminam em `…`.
O rótulo nunca é escrito em Markdown ou front matter.
  - O estado recolhido é persistido, portanto, uma nota deixada recolhida reabre recolhida.

## Papel

Cada nota carrega seu próprio papel, independentemente de qualquer outra nota.

- **Cor da nota:** the seven colours — Amarelo, Azul, Verde, Rosa, Roxo, Cinza, Preto.
- **Tipo de papel:** **Liso**, **Pautado**, **Pontilhado**, **Quadriculado pequeno**, **Quadriculado grande**. O papel comum tem a aparência original e não desenha nenhum padrão.
- **Intensidade:** **Suave**, **Normal**, **Forte** — a opacidade com a qual o padrão é desenhado e nada mais. Nunca altera a cor do papel, o texto ou a geometria da nota. O papel comum mantém a intensidade que lhe foi dada; simplesmente não tem um padrão para agir.
- O padrão é puro CSS: um sistema parametrizado onde o tipo escolhe um padrão e seu espaçamento, a intensidade escolhe a opacidade e a cor do papel escolhe a tinta – tinta escura nos papéis claros, tinta clara nos papéis escuros, para que permaneça visível em todos os sete.
- O espaçamento é em pixels, então o zoom dimensiona o texto enquanto o padrão permanece no mesmo lugar. O papel pautado é espaçado na caixa de linha padrão da nota, mas é um plano de fundo, não uma grade de layout: as linhas não são fixadas em linhas individuais de texto.
- O padrão é pintado na superfície de rolagem, de modo que acompanha o texto, e a própria cor da nota ainda preenche a janela abaixo – um redimensionamento rápido expõe o papel, nunca uma faixa sem pintura.
- A barra de uma nota recolhida mostra sua cor sem o padrão; expandir traz o padrão de volta.
- O tipo e a intensidade do papel são propriedades da nota, armazenadas em seu front matter ao lado da cor. Alterar salva a nota sem alterar seu conteúdo ou data de modificação.

## Tema

O tema é a aparência do **aplicativo**, não de uma nota.

- **Sistema**, **Claro**, **Escuro**, escolhidos no menu de qualquer nota e compartilhados por todas as notas. A preferência é global e reside em `config.toml`.
- **Sistema** segue o esquema de cores da área de trabalho e continua seguindo-o, portanto, mudar a área de trabalho para escuro alcança notas abertas sem reiniciar.
- Ele veste apenas o chrome: menus, popovers, bordas, sombras, estados de foco e foco e texto auxiliar. Tudo o que é desenhado no papel – o texto da nota, as caixas de seleção, os destaques, os botões do cabeçalho – continua tirando a cor do papel.
- Uma nota mantém a cor que lhe foi dada: uma nota amarela permanece amarela no tema escuro e uma nota preta permanece preta no tema claro.

## Posicionamento e interações da janela

- **Arrastar e redimensionar:**
  - Região de arrasto do cabeçalho (`.drag-region`) para mover post-its livremente pela área de trabalho.
  - Alça de redimensionamento discreta no canto inferior direito (`.resize-handle`) com limites de dimensão mínima (`220x160` px).
  - Um gesto emite deltas geométricos apenas enquanto exatamente um ponteiro é capturado; `pointerup`,
`pointercancel`, uma captura de ponteiro perdida ou um movimento informando que nenhum botão foi pressionado, tudo encerra
completamente, e uma moldura que sobrou antes do final não pode mover a janela.
  - A geometria persistiu para `$XDG_STATE_HOME/note-it/state.json` exclusivamente no final do gesto (zero E/S de disco durante arrastar/redimensionar ativo).
- **Fixação geométrica segura e substituto do monitor:**
  - A fixação garante que as notas permaneçam visíveis na tela mesmo após alterações na resolução do monitor.
  - Detecção de conector de vários monitores com fallback elegante se um monitor for desconectado.
- **Colease em cascata inteligente:**
  - Novas notas são exibidas em cascata de forma incremental pela grade da tela.

## Ciclo de vida da nota

- **Fechar mantém a nota:** o botão `×` salva a nota, registra-a como fechada e destrói apenas a janela. O arquivo Markdown, sua geometria, cor, zoom e estado recolhido permanecem no disco.
- **A invocação o traz de volta:** executar `note-it` restaura as notas e as torna visíveis. Com cada nota fechada, a última usada é reaberta em vez de ser criada uma nota em branco.
- **Uma instância:** uma segunda invocação atinge a instância em execução por meio do despachante de instância única e sai; ele nunca inicia um segundo aplicativo.
- **`note-it new`** é a maneira explícita de criar uma nota adicional.

## Tarefas

- **Markdown Listas de tarefas:**
  - Digitar `- [ ] ` cria uma tarefa; `- [x] ` ou `- [X] ` cria um completo.
  - Nós de editor reais com caixas de seleção quadradas, não caracteres de texto, aninhados em qualquer profundidade com
    `Tab` / `Shift+Tab`.
- **Conclusão:**
  - Concluir uma tarefa marca a caixa, risca o texto e registra o momento, mostrado
discretamente como `Concluído dd/MM/aaaa HH:mm`.
  - Reabrir uma tarefa limpa a data; completá-lo novamente registra um novo.
  - Uma tarefa escrita em outro lugar como `- [x]` é carregada como concluída sem nenhuma data inventada para ela.

## Blocos Inteligentes

Quatro tipos de blocos, todos armazenados como Markdown comum e acessíveis a partir da seção **Blocos** do menu da própria nota - nenhuma segunda barra de ferramentas foi introduzida.

- **Bloco de código:** um bloco cercado cuja linguagem sobrevive intacta a cada viagem de ida e volta, incluindo uma que nada aqui pode destacar. Dezesseis gramáticas são carregadas: `plaintext`, `bash`, `javascript`, `typescript`, `json`, `html`/`xml`, `css`, `markdown`, `python`, `rust`, `c`, `cpp`, `java`, `sql`, `yaml` e `toml`, além dos aliases que cada um já responde (`js`, `ts`, `py`, `sh`, `cpp`…). O idioma é escolhido em **Blocos → Linguagem**, que mostra o atual e é oferecido apenas onde significa alguma coisa.
- **Chamada:** `> [!NOTE]`, `> [!TIP]`, `> [!IMPORTANT]`, `> [!WARNING]` e `> [!CAUTION]` — sintaxe de alerta de GitHub, que Obsidian também lê. Um texto explicativo é uma citação em bloco que carrega um tipo, portanto contém vários parágrafos, listas e blocos aninhados sem um modelo de conteúdo próprio. Um tipo não reconhecido é deixado como a citação que já é, com seu texto intacto.
- **Citação:** a citação em bloco simples, que permanece independente das frases de destaque e nunca é promovida a uma. Recuado, pautado na lateral, definido na própria cor do texto da nota, em vez de esmaecido e em itálico.
- **Comentário:** um `<!-- ... -->` mantido no arquivo e mostrado como um pequeno bloco rotulado. É editável - um comentário que a janela nunca mostrou seria aquele que ninguém poderia remover - mas não faz parte do texto da nota.

Markdown digitado manualmente ainda funciona: `` ``` `` opens a code block and `> ` abre uma cotação, exatamente como antes.

O destaque é **apenas apresentação**: decorações do editor sobre os mesmos caracteres, nunca marcação no arquivo. Não é aplicado a um bloco sem idioma e nunca é adivinhado para aquele cujo idioma é desconhecido - um bloco não destacado é a resposta honesta, não um esquema de cores escolhido por semelhança. Digitar fora de um bloco de código não o executa novamente, portanto, uma nota cheia de código permanece tão leve para editar quanto qualquer outra.

Cada cor que um bloco inteligente pinta – sete tokens de sintaxe e cinco acentos de texto explicativo – é definida uma vez para os papéis claros e uma vez para os escuros, e cada uma limpa 4,5:1 em relação ao papel em que é realmente desenhado. Os motivos são tingidos do papel, em vez de serem superfícies próprias, de modo que uma nota mantém sua cor sob cada bloco.

## Motor matemático

Uma nota é calculada conforme é escrita. Nada é pressionado, nada é executado novamente, não há botão de cálculo e nenhum modo para entrar: uma linha que parece aritmética mostra sua resposta ao lado dela, e a resposta segue a nota conforme a nota muda.

O resultado é uma **decoração**, não um texto. Ele não está no documento, portanto não é salvo, selecionado, copiado e não faz parte de uma etapa de desfazer. O `.md` no disco contém exatamente o que foi digitado, o que torna seguro abrir a mesma nota em outro editor.

### Calculando uma linha

Um cálculo começa com `=`:

```text
= 2 + 2                            4
= (100 + 50) / 3                  50
= 10 * 8                          80
```

`+`, `-`, `*`, `/` e parênteses, com a precedência usual e associatividade à esquerda. Os números podem ser negativos e podem ser escritos `10.5` ou `10,5` — ambos os separadores são lidos como decimais. Um número com **dois** separadores (`1.234.567`) é recusado em vez de adivinhado: Note-it não aceita separador de milhares, em nenhuma direção, portanto, um resultado sempre pode ser lido como ele mesmo.

Não há operador de módulo. `%` significa porcentagem e apenas porcentagem, porque um símbolo que significa duas coisas é um símbolo no qual ninguém pode confiar.

### Variáveis

Uma declaração é `nome := expressão`:

```text
preco := 120
quantidade := 3
subtotal := preco * quantidade    360
= subtotal + 10%                 396
```

- Os nomes são ASCII: uma letra ou `_`, depois letras, dígitos e `_`. `preço` não é um nome, e uma linha que diz `:=` com um nome inutilizável é relatada como **nome inválido** em vez de ser lida silenciosamente como prosa.
- `sum`, `avg`, `count` e `de` pertencem à gramática e não podem ser nomes.
- As variáveis ​​são **locais à nota** e resolvidas **de cima para baixo**: uma variável existe a partir de sua declaração para baixo. `= preco * 2` escrito *acima* `preco := 100` relata uma variável desconhecida, que também torna os ciclos impossíveis - `a := b + 1` sobre `b := a + 1` simplesmente falha na primeira linha.
- Uma declaração posterior substitui uma anterior dessa linha. Uma declaração que falha desdeclara o nome, então tudo abaixo diz isso.
- Uma declaração cujo lado direito é um número simples não mostra nenhum resultado: o valor já está na linha.

### Porcentagens

```text
= 10% de 200                      20
= 200 + 10%                      220
= 200 - 10%                      180
taxa := 10%
= taxa * 200                      20
```

`X%` é um centésimo. As leituras contextuais — um aumento, um desconto e `de` — aplicam-se a um `%` **escrito na linha**, nunca a um valor que veio de um: `taxa` contém `0.1`, então `= 200 + taxa` é `200,1`. O que você pode ver é o que se aplica. `de` requer uma porcentagem à sua esquerda; `200 de 10` é uma expressão inválida e não um número que ninguém quis dizer.

### `sum`, `avg` e `count`

Um agregador é a expressão **inteira** de sua linha e lê o bloco de linhas de cálculo consecutivas diretamente acima dela:

```text
= 10                              10
= 20                              20
= 30                              30
= sum                             60
= avg                             20
= count                            3
```

O bloco é exatamente "as linhas `=` imediatamente acima que produziram um valor". Uma linha de prosa, um título, uma declaração ou um cálculo falhado termina-o, de modo que um número colocado numa frase nunca é adicionado a nada e duas listas separadas por uma linha de texto permanecem duas listas. Os três agregadores leem o bloco sem consumi-lo, então empilham; o primeiro valor abaixo de um inicia um novo bloco.

Um bloco vazio soma `0` e conta `0`; sua média é `0 / 0` e diz isso.

### Quando o cálculo não é executado

O cálculo é lido **somente parágrafos simples**. Dentro de um bloco de código protegido, um intervalo de código embutido, um comentário, um título, uma lista, uma tarefa, uma citação ou uma chamada, `= 2 + 2` é o texto que é. Este é um limite deliberado da primeira versão: uma linha que calcula em um lugar e não em outro por razões invisíveis é pior do que uma que nunca calcula em nenhum dos dois.

### Quando não pode responder

Uma falha consiste em quatro palavras ao lado da linha, em itálico, e nada mais — nenhuma caixa de diálogo, nenhum pop-up, nenhum rastreamento de pilha e nada escrito no arquivo:

| | |
| --- | --- |
| `= 1 / 0` | divisão por zero |
| `= nao_existe * 2` | variável desconhecida |
| `= (2 + 3` | expressão inválida |
| `12preco := 1` | nome inválido |

### Reatividade e quanto custa

Toda a nota é reavaliada a cada alteração no documento. Esse é todo o mecanismo de reatividade: altere `preco` e cada linha abaixo dele se move na mesma passagem, sem nenhum gráfico de dependência para ficar obsoleto e sem temporizadores. Medido em uma nota muito maior do que um post-it – 100 parágrafos de prosa, 20 variáveis, 50 expressões e todos os três agregadores – um pressionamento de tecla custa uma fração de milissegundo.

### Não há avaliador

As expressões são lidas por um pequeno lexer e um analisador descendente recursivo escrito para isso e nada mais. Não há `eval`, nem `Function`, nem acesso de propriedade, nem sintaxe de chamada e nenhum objeto host em qualquer lugar dele, e nenhuma dependência foi adicionada. Uma nota escrevendo `window.location`, `constructor.constructor(...)` ou `fetch(...)` está escrevendo uma expressão inválida ou nomeando uma variável que não existe - as variáveis ​​vivem em um `Map`, que não tem nenhuma cadeia de protótipo para acessar.

### Aparência

Discreto: uma pequena lasca no final da linha, numa tinta misturada com as duas do próprio papel, sobre o mesmo fundo esmaecido que o bloco de código, o texto explicativo e o comentário já utilizam. Ele limpa 4,5:1 em todos os sete papéis, não participa da seleção ou interação do ponteiro e não precisa de substituição de cor ou tema próprio.

## Conversões

Uma conversão é um cálculo com uma unidade de cada lado e funciona da mesma forma que qualquer outro resultado: escrito na nota, calculado conforme você digita, mostrado ao lado da linha e nunca escrito no arquivo.

```text
= 10 km em m                      10000 m
= 1500 m em km                    1,5 km
= 0 C em F                        32 °F
```

### A sintaxe

```text
= <expressão> <unidade> em <unidade>
```

`em` é a palavra-chave de conversão e a única — não há uma segunda grafia para a mesma coisa. É uma palavra reservada, portanto nenhuma variável pode ser chamada `em`.

O lado esquerdo é uma expressão completa do mecanismo matemático, então tudo isso é lido:

```text
= (10 + 5) km em m                15000 m
= 2 * 3 km em m                   6000 m

distancia := 12
= distancia km em m               12000 m

x := 5
= x * 2 km em m                   10000 m
```

A unidade se aplica a **toda a expressão à sua esquerda**, então `= 10 + 5 km em m` equivale a quinze quilômetros. Não há álgebra unitária aqui para dar um significado à outra leitura, e uma regra que você pode manter em sua cabeça é melhor do que duas que você precisa adivinhar. Use parênteses quando o agrupamento for importante para você.

Uma declaração pode conter uma conversão — `metros := 10 km em m` — e a variável então contém `10000`. Ela contém um **número**, não uma quantidade: uma unidade em uma variável não faz parte desta versão, então `distancia := 10 km` é uma expressão inválida em vez de uma distância. Consulte a limitação no final desta seção.

### As unidades

Cada grafia abaixo corresponde **exatamente**. Não há conversão de maiúsculas/minúsculas (case folding): `m` é um metro e `M` não é nada, porque uma regra que os dobrasse também dobraria `MB` sobre `mb`. Quando uma conveniência em letras minúsculas é segura, ela é simplesmente listada como um alias, e é por isso que `ml` e `l` funcionam e `mb` não.

### Comprimento — base `m`

| unidade | apelidos | exibida | fator |
| --- | --- | --- | --- |
| `mm` | `milimetro`, `milimetros` | mm | 0.001 |
| `cm` | `centimetro`, `centimetros` | cm | 0.01 |
| `m` | `metro`, `metros` | m | 1 |
| `km` | `quilometro`, `quilometros` | km | 1000 |
| `in` | `polegada`, `polegadas` | em | 0.0254 |
| `ft` | `pe`, `pes` | ft | 0.3048 |
| `yd` | `jarda`, `jardas` | yd | 0.9144 |
| `mi` | `milha`, `milhas` | mi | 1609.344 |

### Massa — base `g`

| unidade | apelidos | exibida | fator |
| --- | --- | --- | --- |
| `mg` | `miligrama`, `miligramas` | mg | 0.001 |
| `g` | `grama`, `gramas` | g | 1 |
| `kg` | `quilograma`, `quilogramas`, `quilo`, `quilos` | kg | 1000 |
| `t` | `tonelada`, `toneladas` | t | 1000000 |
| `oz` | `onca`, `oncas` | oz | 28.349523125 |
| `lb` | `libra`, `libras` | lb | 453.59237 |

### Volume — base `mL`

| unidade | apelidos | exibida | fator |
| --- | --- | --- | --- |
| `mL` | `ml`, `mililitro`, `mililitros` | mL | 1 |
| `cL` | `cl`, `centilitro`, `centilitros` | cL | 10 |
| `dL` | `dl`, `decilitro`, `decilitros` | dL | 100 |
| `L` | `l`, `litro`, `litros` | L | 1000 |
| `cm³` | `cm3`, `cc` | cm³ | 1 |
| `m³` | `m3` | m³ | 1000000 |

### Temperatura — base `K`

| unidade | apelidos | exibida | conversão |
| --- | --- | --- | --- |
| `°C` | `C`, `c`, `celsius` | °C | `K = °C + 273,15` |
| `°F` | `F`, `f`, `fahrenheit` | °F | `K = (°F + 459,67) × 5/9` |
| `K` | `kelvin` | K | — |

### Tempo — base `s`

| unidade | apelidos | exibida | fator |
| --- | --- | --- | --- |
| `ms` | `milissegundo`, `milissegundos` | ms | 0.001 |
| `s` | `seg`, `segundo`, `segundos` | s | 1 |
| `min` | `minuto`, `minutos` | min | 60 |
| `h` | `hora`, `horas` | h | 3600 |
| `dia` | `dias`, `d` | dia / dias | 86400 |
| `semana` | `semanas` | semana / semanas | 604800 |

### Área — base `m²`

| unidade | apelidos | exibida | fator |
| --- | --- | --- | --- |
| `mm²` | `mm2` | mm² | 0.000001 |
| `cm²` | `cm2` | cm² | 0.0001 |
| `m²` | `m2` | m² | 1 |
| `km²` | `km2` | km² | 1000000 |
| `ha` | `hectare`, `hectares` | ha | 10000 |

Uma unidade de área é sua própria unidade com seu próprio fator, não um comprimento com um expoente: `= 1 m2 em cm2` é `10000 cm²`.

### Dados digitais — base `B`

| unidade | apelidos | exibida | fator |
| --- | --- | --- | --- |
| `B` | `byte`, `bytes` | B | 1 |
| `KB` | — | KB | 1000 |
| `MB` | — | MB | 1000000 |
| `GB` | — | GB | 1000000000 |
| `TB` | — | TB | 1000000000000 |
| `KiB` | — | KiB | 1024 |
| `MiB` | — | MiB | 1048576 |
| `GiB` | — | GiB | 1073741824 |
| `TiB` | — | TiB | 1099511627776 |

Os prefixos SI são **decimais** e os prefixos IEC são **binários**, que é o que os dois conjuntos de nomes existem para distinguir: `= 1 GB em MB` é `1000 MB` e `= 1 GiB em MiB` é `1024 MiB`. Note-it nunca os confunde.

### Velocidade — base `m/s`

| unidade | apelidos | exibida | fator |
| --- | --- | --- | --- |
| `m/s` | — | m/s | 1 |
| `km/h` | — | km/h | 1/3,6 |
| `mph` | — | mph | 0.44704 |

Três linhas nomeadas, não um comprimento dividido por um tempo. Não há álgebra de unidades derivadas por trás deles, então `kg/L` e `m/s²` são unidades desconhecidas em vez de quantidades que Note-it funcionam.

### `m` é um metro, `min` é um minuto

`m` nunca é um minuto, em qualquer contexto. Se os minutos ganhassem uma abreviatura de uma letra, os dois colidiriam, então eles não têm uma.

### O que uma conversão recusa

| | |
| --- | --- |
| `= 10 banana em m` | unidade desconhecida |
| `= 10 km em foo` | unidade desconhecida |
| `= 10 kg em km` | unidades incompatíveis |
| `= 1 m2 em m` | unidades incompatíveis |
| `= -300 C em K` | conversão inválida — nada é mais frio que o zero absoluto |
| `= 10 km` | Expressão inválida — uma conversão não tem alvo |
| `= banana km em m` | variável desconhecida |

Um par incompatível é recusado antes mesmo de a expressão ser avaliada: uma dimensão é uma propriedade da grafia, portanto `= 10 kg em km` não pode se tornar válido para algum valor do lado esquerdo.

### Onde uma conversão é lida

Exatamente onde um cálculo é: **somente parágrafos simples**. Dentro de um bloco de código protegido, um intervalo de código embutido, um comentário, um título, uma lista, uma tarefa, uma citação ou uma chamada, `= 10 km em m` é o texto que é.

### Agregadores e quantidades convertidas

`sum`, `avg` e `count` somam números simples e não sabem nada sobre unidades, então uma linha convertida **termina** o bloco que eles lêem em vez de ser totalizada nele. Agregar unidades é um recurso real; agregar silenciosamente entre eles é um bug.

### Precisão e como um resultado é escrito

Os fatores são os definidos e nada foi arredondado para arrumar uma mesa: uma polegada equivale exatamente a 0,0254 m, uma libra equivale exatamente a 453,59237 g, uma milha equivale exatamente a 1.609,344 m. A temperatura carrega seus próprios conversores em vez de um fator, porque nenhuma multiplicação leva de 0 a 32 e de 100 a 212 ao mesmo tempo.

Os resultados são escritos pelo mesmo formatador que o mecanismo matemático sempre usou: vírgula para o separador decimal, sem separador de milhar, doze dígitos significativos. O agrupamento ausente é deliberado — `.` e `,` são ambos lidos como separadores decimais, portanto, um resultado agrupado seria aquele que esse mesmo mecanismo lê de volta como um número diferente.

`dia` e `semana` são as únicas unidades cujo nome exibido muda com o valor, porque `1 dia` e `7 dias` devem ser lidos em português.

### As moedas não estão aqui

`USD em BRL` não tem resposta sem uma taxa, a taxa muda a cada minuto e uma taxa escrita em uma tabela está errada antes de ser confirmada. Note-it converte apenas quantidades constantes, off-line e idênticas quando a nota for reaberta em dez anos. As moedas são uma fase posterior com uma fonte própria — consulte `docs/decisions.md`, ADR-025.

### Limitação conhecida: uma unidade não pode viver em uma variável

```text
distancia := 10 km     ← expressão inválida
```

Uma variável contém um número, então a unidade vai na linha que a utiliza:

```text
distancia := 10
= distancia km em m    10000 m
```

Transportar unidades através de variáveis ​​significaria que cada valor no motor se tornaria uma quantidade, e com ele percentagens, agregações e todas as regras já estabelecidas. É um limite deliberado para esta versão, em vez de um limite incompleto.

## Pesquisa

Aberto com `Ctrl+K` dentro de qualquer nota. A paleta é um painel na página, não uma segunda janela e não faz parte do documento — nada digitado nela pode chegar ao Markdown.

### O que é pesquisado

**corpo** da nota: tudo abaixo de front matter. Títulos, listas, tarefas, citações, textos explicativos, blocos de código e comentários são todos conteúdos de notas e podem ser pesquisados.

O front matter em si não é. `note_it:`, `created_at:`, `updated_at:` e `paper:` são como o arquivo é escrito, não o que o leitor escreveu, e uma pesquisa por `paper` não deve retornar todas as notas no store.

Tampouco é algo que o editor apenas desenha. Um `4` mostrado ao lado de `= 2 + 2`, um `10000 m` mostrado ao lado de `= 10 km em m` e todas as outras decorações não estão no arquivo, portanto nenhuma pesquisa pode encontrá-los.

### Como uma consulta é correspondida

| Propriedade | Comportamento |
| --- | --- |
| Maiúsculas/minúsculas | Insensível — `BIÓPSIA`, `Biópsia` e `biópsia` são uma palavra |
| Acentos | Insensível — `biopsia` encontra `Biópsia`, `coracao` encontra `Coração` |
| Correspondência | Substring literal. `.*`, `[a-z]` e `(foo\|bar)` são esses caracteres, não um padrão |
| Limite de consulta | 512 caracteres; mais é recusado em vez de truncado silenciosamente |
| Resultados | 100 notas no máximo |
| Trecho | Cerca de 240 caracteres, cortados no limite do caractere |
| Ordem | Mais recentemente escrito no primeiro |
| Notas varridas | **Todas as notas.** Não há limite máximo de varredura — o limite está nos resultados, não na extensão da pesquisa |

Não há lematização, correspondência difusa e pesquisa semântica. `biopsia` encontra `biópsia`; não encontra `punção`. A regra é aquela que o leitor pode prever, e esse é o ponto.

**O que os limites não limitam é a nota.** Vinculam a consulta e a resposta; o arquivo é lido até o fim, porque uma palavra no final de uma nota longa deve ser localizável. Um store de mil notas é pesquisado em cerca de 40 ms e uma única nota de 2 MB é pesquisada corretamente e sem escrever nada - ambos medidos por testes - mas não há garantia formal sobre um arquivo individual arbitrariamente grande, e nenhuma é reivindicada. Consulte ADR-027.1.

### Qual é a aparência de um resultado

Uma nota é um resultado, não importa quantas vezes a palavra apareça nela.

```text
Biópsia hepática                                    4
…a biópsia transjugular é utilizada quando…
```

- O **rótulo** é a primeira linha não vazia da nota, com os marcadores Markdown mais óbvios removidos para exibição — `# Biópsia hepática` é mostrado como `Biópsia hepática`. Nada é gravado no arquivo para criar um título. Uma nota sem texto é listada como `Nota vazia`.
- O **snippet** é o texto em torno da primeira correspondência, renderizado como texto. Uma nota contendo `<script>alert(1)</script>` mostra esses caracteres; não se torna um elemento.
- A **contagem** aparece quando uma nota contém mais de uma ocorrência.

### Uma consulta vazia lista notas recentes

Abrir a paleta sem digitar mostra as notas mais recentemente **escritas**, portanto, o mesmo controle também é como você se move entre elas. Aparecer nessa lista não é edição: `updated_at` não se move.

"Escrito mais recentemente" é o próprio `updated_at` da nota, não a data no arquivo. Alterar a cor, o papel, a intensidade do padrão ou o tamanho da fonte de uma nota reescreve o arquivo sem ser uma edição e não move a nota para cima nesta lista - repintar uma nota não é escrever nela. Uma nota sem `updated_at` — escrita antes da existência do campo, ou com front matter que não pode ser lida — retorna ao carimbo de data/hora do próprio arquivo. A mesma regra decide qual nota uma invocação traz de volta, portanto há uma ideia de “mais recente” na aplicação, em vez de duas que discordam.

### Abrindo um resultado

`Enter` ou um clique:

- uma nota **já aberta** é ativada;
- uma nota que está **fechada** é aberta;
- uma nota **recolhida** é expandida;
- a nota rola até a primeira ocorrência e a destaca, com a barra de localização aberta para que o destaque tenha uma causa visível e uma saída óbvia.

Nada disso altera o texto da nota e nada move `updated_at`. A camada Desktop/Overlay também não é tocada: abrir uma nota de uma pesquisa nunca troca a camada por todo o resto.

Um resultado que o store não possui mais - excluído de fora entre a pesquisa e o `Enter` - diz `nota não encontrada`, descarta a linha e pesquisa novamente. Nada é recriado.

### Teclado

| Key | Ação |
| --- | --- |
| `Ctrl+K` | Abrir |
| `Esc` | Fechar, retornando o teclado ao editor |
| `↓` / `↑` | Resultado seguinte/anterior, embrulho |
| `Enter` | Abra o resultado selecionado |
| `Ctrl+Shift+Space` | Deliberadamente **não** reivindicado — a camada pertence ao aplicativo e alterná-la com a paleta aberta não a fecha nem digita um espaço |

A digitação é interrompida em 120 ms e cada solicitação é numerada. Somente a resposta à solicitação atualmente pendente pode alterar a lista, portanto, uma resposta a `bio` é descartada assim que `biopsia` for solicitado - se ela chega antes ou depois da mais recente e se alguma coisa foi ou não respondida ainda. Uma resposta que chega após o fechamento da paleta não altera nada.

### Pesquisar não escreve nada

Nenhum salvamento, nenhuma liberação, nenhum `.md` tocado, nenhum `updated_at` movido, nenhum arquivo de índice e nada registrado em `state.json` - nem a consulta, nem a seleção, nem a paleta. Abrir uma nota fechada a partir de um resultado altera o `is_open` dessa nota, porque o leitor realmente a abriu.

### Sem índice

Não há nenhum, de propósito. Mil notas são listadas, lidas, dobradas, combinadas e transformadas em fragmentos em cerca de 40 ms, de modo que um índice não compraria nada que uma pessoa pudesse perceber e custaria invalidação, reconstrução, um formato de arquivo para migrar e uma segunda implementação para manter a honestidade. A medição é um teste, então o dia em que ela deixa de ser verdadeira é o dia em que algo falha. Consulte ADR-027.

## Localizar e substituir

Dentro da nota que você está vendo, sobre o documento ativo – incluindo texto digitado há um segundo e ainda não salvo.

### Localizar

| Key | Ação |
| --- | --- |
| `Ctrl+F` | Aberto, propagado a partir da seleção quando é curto e em uma linha |
| `Enter` | Próxima ocorrência |
| `Shift+Enter` | Ocorrência anterior |
| `Esc` | Fechar |
| `Aa` | Caso de correspondência |

O contador indica `2 de 7` ou `nenhuma`. A navegação envolve ambas as direções. Cada ocorrência é destacada, a atual com mais força, usando tokens temáticos para que o destaque fique visível tanto em papel claro quanto em papel preto.

Encontrar não altera nada: os destaques são decorações, portanto não há transação, nenhuma etapa de desfazer, nenhuma reescrita de Markdown e nenhuma alteração em `updated_at`.

Find pesquisa o documento, portanto não consegue encontrar um resultado calculado ou convertido — pesquisando uma nota contendo `= 2 + 2` para `4` relatórios `nenhuma`.

### Substituir

`Ctrl+H` adds a second row: **Substituir por…**, **Substituir**, **Todas**.

- **Substituir** substitui a ocorrência atual e passa para a próxima. Cada um tem sua própria etapa de desfazer.
- **Todas** substitui todas as ocorrências em **uma** transação, aplicada da última para a primeira para que as posições anteriores permaneçam válidas. Vinte substituições retornam com um único `Ctrl+Z`.
- A substituição é literal. Não há regex, nem `$1`, nem `\1` nem grupos de captura.
- Marcas, listas, títulos, tarefas, citações e blocos de código sobrevivem, porque o documento é editado em vez de serializado, substituído por strings e recarregado.
- A substituição **sensibiliza o sotaque**, ao contrário da pesquisa global: `saude` não substitui `saúde`. Por causa disso, uma nota aberta na paleta recebe a grafia que realmente corresponde a ela, portanto, pesquisar `biopsia` ainda destaca `Biópsia`.
- A substituição é uma edição real, então `updated_at` se move — uma vez, para a edição, e não novamente para as decorações que a seguem.

Substituir atos apenas na nota atual. Não há atualização de notas cruzadas.

## Colando um URL sobre o texto selecionado

Selecione `site oficial`, cole `https://example.com` e a nota será válida:

```markdown
[site oficial](https://example.com)
```

As palavras que você escolheu são mantidas e passam a ser o link, em vez de serem substituídas pela URL.

- O URL é avaliado por `safeLinkUrl`, a mesma lista de permissões usada pelo restante do aplicativo. `http`, `https` e `mailto` tornam-se links; `javascript:`, `data:`, `file:`, `ftp:` e qualquer outra coisa são colados como texto normal.
- Nada é buscado. Sem título, sem favicon, sem OpenGraph, sem visualização — e, portanto, sem rede, sem rastreamento e sem espera.
- Dentro do código embutido ou de um bloco de código, ou com uma seleção abrangendo dois blocos, é uma colagem comum: um URL na origem contém caracteres e um link não pode quebrar uma estrutura.
- É uma etapa de desfazer.

**A renderização de link compacto não é implementada deliberadamente.** Encurtar um URL oculta parte de onde ele leva, e o leitor que mais precisa ver `https://evil.example.com/path` por completo é exatamente aquele que uma abreviação enganaria. Consulte ADR-027.

## Lixo

Excluir uma nota é uma ação explícita e recuperável.

**Movendo uma nota para a lixeira.** *☰ › Dados › Mover esta nota para a lixeira*. O aplicativo pede confirmação primeiro:

```text
Mover esta nota para a lixeira? Você poderá restaurá-la depois em Dados › Lixeira.
[Cancelar] [Mover]
```

**Cancelar** recebe o foco, portanto a tecla que já está sob seu dedo escolhe a opção que não faz nada. Pressionar `Esc` ou clicar fora também cancela a ação.

- O botão `×` e `Ctrl+W` ainda significam **fechar a janela**. Fechar uma nota nunca a excluiu e ainda não exclui.
- A nota é salva primeiro. Se o texto mais recente não puder ser escrito, nada será movido: a nota permanecerá aberta, a falha será relatada e você poderá tentar novamente.
- O arquivo deixa `notes/` por `trash/`, byte por byte. Front matter, cor, papel, tarefas, links, cálculos e comentários viajam com ele.
- Mover uma nota para a lixeira não é uma edição, portanto sua data de modificação não muda.

**Uma nota na lixeira não é uma nota.** `Ctrl+K` não a encontra, a lista de consulta vazia não a oferece, uma invocação não a traz de volta e a reinicialização não a reabre.

**Recuperando uma nota.** *☰ › Dados › Lixeira* lista o que pode ser recuperado — a primeira linha de cada nota, uma prévia de seu início e quando foi excluída — da mais recente para a mais antiga. As teclas de seta percorrem a lista, `Enter` restaura a nota selecionada e `Esc` fecha o painel. Cada linha também possui um botão chamado **Restaurar**.

A restauração coloca o arquivo de volta em `notes/` com o mesmo identificador e os mesmos bytes, e a nota torna-se localizável novamente imediatamente. Ele mantém a data de modificação original: uma nota recuperada volta para onde estava no alternador rápido, em vez de pular para o topo como se tivesse acabado de ser escrita.

**A restauração nunca substitui uma nota ativa.** Se uma nota com o mesmo identificador já estiver no store, a restauração será recusada, nenhum dos arquivos será alterado e o painel informará isso.

**Não há exclusão permanente nem "esvaziar a lixeira"** nesta versão. Isso é deliberado: esta é a fase que torna a exclusão recuperável, e um botão irreversível ao lado de um botão de restauração está a um clique errado do que ele existe para evitar. A lixeira, portanto, cresce até que você mesmo a limpe, o que pode ser feito com qualquer gerenciador de arquivos - uma nota na lixeira é um `.md` comum em `~/.local/share/note-it/trash/`.

## Cópias de segurança

Note-it mantém instantâneos locais de tudo que pode ser recuperado.

**Onde.** `~/.local/share/note-it/backups/<data-e-hora>/`, contendo `notes/`, `trash/`, `assets/`, `config.toml`, `state.json` e um `manifest.json` descrevendo o instantâneo. Diretórios comuns e arquivos comuns — sem arquivo, sem banco de dados, sem formato próprio do Note-it.

**As imagens viajam com as notas que as contêm.** Uma nota que diz `![](../assets/…)` é apenas meia nota sem o arquivo para o qual aponta, então `assets/` é copiado com as mesmas garantias e no mesmo formato, byte por byte. Um instantâneo que não conseguiu copiar uma imagem não é confirmado - você nunca obtém um backup que pareça completo e sem imagens. Uma imagem que nenhuma nota aponta mais também é copiada: um backup registra o store como está e não é o local para decidir quais arquivos ainda são desejados.

**Quando.** No máximo um snapshot automático a cada 24 horas, tirado **antes** da primeira alteração depois que essa janela tiver passado. A questão é considerar primeiro: o estado ao qual vale a pena voltar é aquele antes da edição. Não há cronômetro – um daemon ocioso não funciona e um daemon deixado aberto por dias tira seu instantâneo no momento em que você começa a digitar novamente.

**Agora, se você quiser.** *☰ › Dados › Fazer backup agora* tira um instantâneo imediatamente e diz se funcionou, em uma linha no rodapé da nota, em vez de uma caixa de diálogo sobre ele. Útil antes de fazer algo sobre o qual você não tem certeza.

**Quantos.** Os sete mais recentes são mantidos. Os antigos são removidos somente **após** um novo ter sido completamente gravado, portanto, um backup que falha nunca custa a proteção que você já tinha.

**O que nunca está em um snapshot:** snapshots anteriores, arquivos temporários e qualquer coisa alcançada por meio de um link simbólico. Um backup copia arquivos regulares dos dois diretórios que foi solicitado a copiar e não segue nada deles.

**Se um backup falhar,** a nota ainda será salva. Um instantâneo é uma camada extra de segurança; sua falha é gravada na saída de diagnóstico e tentada novamente posteriormente, nunca se transformando em uma recusa em escrever seu texto.

**Recuperar um instantâneo** é `cp`, com o aplicativo fechado — consulte [docs/storage.md](storage.md#recuperando-se-de-um-instantâneo) para obter o procedimento exato, incluindo como recuperar uma única nota em vez de todo o store. Deliberadamente, não existe "restaurar tudo" com um clique: essa é uma transação com vários arquivos e merece seu próprio design, em vez de uma entrada de menu.

> **Um backup local não é uma recuperação de desastres.** Esses instantâneos ficam no mesmo disco que as notas.
> Eles protegem contra exclusão acidental, corrupção lógica, edição que você deseja desfazer ou
> versão para a qual você deseja voltar. Eles protegem contra **nenhuma** unidade morta, máquina perdida ou
> roubado e eles não são criptografados. A proteção contra falhas de hardware precisa de uma cópia em outro
> hardware e Note-it não fabrica um.

## Imagens

Uma imagem em uma nota, mantida como um arquivo em vez de contrabandeada para o texto.

**Colocando um.** Cole, solte na nota ou peça um seletor de arquivo — no clipe no cabeçalho ou em *☰ › Mídia › Inserir imagem…*. Todos terminam no mesmo lugar: os bytes são gravados no store e a nota ganha uma referência a eles. O clipe de papel e a entrada do menu são duas portas para uma sala: uma solicitação, um seletor, uma importação, para que nada possa ficar entre eles.

**PNG, JPEG, WebP e GIF.** O que é um arquivo *é* decidido por seus primeiros bytes, nunca por seu nome — então um PNG chamado `.txt` é um PNG, e algo chamado `.png` que não é uma imagem é recusado. **SVG não é aceito**: é um formato de documento que pode conter script, e admiti-lo abriria uma superfície inteira por causa de uma imagem. Uma recusa diz isso em uma linha no rodapé da nota e não deixa nada para trás – nenhum diretório, nenhum arquivo escrito pela metade, nenhuma alteração na nota.

**Para onde vão os bytes.** `~/.local/share/note-it/assets/<note-id>/<asset-id>.<ext>`, ao lado de `notes/` e `trash/`. Arquivos comuns com nomes comuns, copiados com `cp` como tudo aqui. Nada é embutido no Markdown como base64: uma captura de tela transformaria uma nota que você pode ler em um megabyte que você não pode, e faria isso com seus backups e diferenças também.

**O que a nota armazena.** Um caminho relativo a `notes/` — `../assets/<note-id>/<asset-id>.png` — e nunca absoluto, então uma nota que você coloca no Git não diz nada sobre seu diretório inicial. Essa forma relativa também é a razão pela qual uma nota chega ao lixo e volta intacta: `notes/` e `trash/` são irmãos, então `..` sobe de qualquer um deles para o mesmo lugar e nada precisa ser reescrito.

**Dois formulários armazenados e uma regra para os quais.** Embora não haja nada a dizer além de onde está a imagem, ela é clara Markdown - `![](../assets/…)`. Depois de escolher uma largura ou alinhamento, que a sintaxe da imagem de Markdown não tem onde colocar, ela se torna uma tag canônica carregando exatamente quatro coisas:

```html
<img src="../assets/…" alt="" data-note-it-width="320" data-note-it-align="left">
```

Sempre esses atributos, sempre nessa ordem, e apenas aqueles realmente definidos — então a mesma imagem sempre grava os mesmos bytes e um salvamento que não alterou nada não altera nada no disco. Qualquer outra coisa nessa tag é descartada em vez de mantida: um `onerror`, um `style`, um `srcset` ou uma fonte que não seja um dos ativos deste store.

**Tamanho.** Uma nova imagem abre tampada — larga o suficiente para ser vista em uma nota larga, pequena o suficiente para caber em uma nota estreita — e nunca maior que seu tamanho natural. Selecione-o e arraste uma das alças para redimensionar: as proporções são mantidas porque apenas a largura é armazenada, a altura seguindo a própria imagem. Uma imagem pode ser tão larga quanto a nota e não mais larga, independentemente do que o ponteiro faça. Todo o arrasto é uma entrada no histórico, então `Ctrl+Z` retorna a largura a partir da qual você começou.

**Alinhamento e empacotamento.** Selecione a foto e escolha *Esquerda*, *Centro* ou *Direita*. Flutuam para a esquerda e para a direita, e o texto percorre o outro lado – ao redor da imagem, nunca abaixo dela. Center é um bloco próprio, com o texto acima e abaixo. Citações, comentários e blocos de código ficam ao lado de uma imagem flutuante, e não abaixo dela.

**Removendo um.** Retire-o da nota como qualquer outro conteúdo. **O arquivo não é excluído.** Não há mais coleta automática de imagens, nem anotações, deliberadamente: decidir que um arquivo não é utilizado é uma suposição, e agir de acordo com essa suposição destrói algo. Se você quiser o espaço de volta, os ativos são arquivos comuns em um diretório comum e `rm` ainda funciona.

**Nada é buscado, nunca.** Não há como inserir uma imagem por URL, e uma imagem remota que alguém digitou à mão é desenhada sem nenhuma fonte — portanto, abrir uma nota chega à rede de graça, e uma nota não pode ser usada para dizer a ninguém que você a leu. A página não consegue nem nomear um arquivo: ela pede `note-it-asset:/<note>/<asset>.<ext>` ao aplicativo, e o aplicativo resolve isso dentro do próprio diretório de ativos da nota ou não o resolve.

**Uma imagem não é texto.** Nada sobre como uma imagem é armazenada chega ao título recolhido, a um resultado de pesquisa, à lixeira ou ao que a nota diz: pesquisar um identificador, uma largura, um alinhamento ou `assets` não encontra nada, e uma nota contendo uma imagem e nenhuma palavra ainda é *Nota sem título*. As palavras em torno de uma imagem permanecem tão fáceis de encontrar como sempre foram.

## Flashcards

Escreva o cartão na nota. `Pergunta :: Resposta` é estudado em uma direção e `Termo ::: Definição` em ambas. Os espaços ao redor de um delimitador embutido fazem parte da sintaxe: `A::B`, `namespace::method`, horários, URLs, código embutido, blocos de código e uma linha com mais de um delimitador possível permanecem como conteúdo comum. Quatro ou mais dois pontos também não são uma carta.

Para um lado que seja um bloco inteiro, coloque `::` ou `:::` sozinho em um parágrafo de nível superior entre os dois blocos. Exatamente o bloco imediatamente anterior é o da frente e exatamente o bloco posterior é o de trás. Um lado pode, portanto, ser um título, parágrafo com quebras rígidas, lista, lista de verificação, citação, texto explicativo, imagem gerenciada ou imagem e texto juntos. Um marcador aninhado em um desses blocos é apenas texto, não estrutura.

**O documento é o baralho.** Não há arquivo flashcard, banco de dados, identificador oculto ou cópia paralela para sincronizar. O detector lê o documento ProseMirror ativo e projeta cartões de origem a partir dele; Markdown e a árvore `assets/` existente continuam sendo a única fonte durável. Excluir o delimitador exclui o cartão, e um backup já o carrega porque carrega a nota e suas fotos.

Os delimitadores reconhecidos permanecem visíveis e recebem uma leve decoração do editor. A marca é pintada sobre o documento, nunca uma transação: ela não altera Markdown, carimbo de data/hora ou histórico de desfazer. A contagem em *☰ › Estudo* segue o documento ativo e indica tanto os cartões de origem quanto os itens de revisão, porque uma fonte reversível produz duas perguntas.

**Estudando.** *☰ › Estudo › Estudar esta nota* abre um painel interno sobre a nota atual. Começa na frente com a resposta oculta; *Mostrar resposta*, *Anterior*, *Próximo* e *Embaralhar* operam nessa sessão, sem envoltório em nenhuma das extremidades. `Space` ou `Enter` revela, `ArrowLeft` e `ArrowRight` se movem e `Escape` fecha enquanto o painel está em foco. Cartões longos rolam dentro do painel e as imagens usam a mesma referência `note-it-asset:` sem alças ou controles do editor.

Uma sessão é um instantâneo dos itens de revisão no instante em que é aberta. A edição ou AutoPaste pode continuar alterando a nota abaixo sem alterar a questão em estudo; feche e abra novamente para tirar um novo instantâneo. Shuffle permuta itens de revisão, retorna ao primeiro e oculta sua resposta. Nada sobre ordem ou progresso persiste.

**O estudo é somente leitura em relação às notas.** Abertura, revelação, movimentação, embaralhamento, classificação e fechamento de envio sem transação do editor. Markdown, `updated_at` e o histórico de desfazer permanecem exatamente como estavam. Um Timer ou Pomodoro continua funcionando quando seu popover abre espaço para Estudo; recolher a nota fecha a sessão, e ocultar ou desistir a destrói com o WebView.

### Central de estudos e Ladder-v1

O botão deck no cabeçalho abre todos os itens de revisão de todas as notas ativas, incluindo notas cujas janelas estão fechadas. O host fornece documentos de notas de `notes/` - nunca de `trash/` - e o WebView os analisa sequencialmente com um editor efêmero Tiptap, o mesmo esquema e `extractFlashcards` usados ​​pela nota visível. O Markdown ativo da nota atual substitui a cópia armazenada dessa passagem, portanto, abrir o Hub nunca precisa forçar um salvamento. Fechar e reabrir gera um novo instantâneo do catálogo.

*Revisar agora* mostra os itens vencidos, os mais vencidos primeiro, seguidos dos novos itens na ordem do documento; *Todos* também inclui itens futuros; *Esta nota* limita a lista à nota invocadora. Cada linha compacta mostra o título da nota projetada e o status Novo/Revisar agora/futuro. As imagens permanecem imagens gerenciadas `note-it-asset:` e nunca são copiadas. O mesmo FlashcardPanel renderiza sessões locais e globais, agora adicionando a nota de origem, classificações após revelação, visualizações de intervalo e um resumo mínimo.

O progresso reside separadamente em `$XDG_DATA_HOME/note-it/study.json`. A chave de uma direção de revisão é SHA-256 sobre uma versão, nota UUID, frente/verso semântico, direção e ordinal duplicado. Posição, negrito/itálico/destaque/cor/tamanho e largura/alinhamento da imagem não participam; texto, fonte/alt e direção da imagem gerenciada fazem. Direções reversíveis, portanto, programadas de forma independente. Chaves editadas ou removidas podem permanecer órfãs e reaparecer naturalmente se o cartão semântico exato retornar.

Os níveis fixos do Ladder-v1 são 10 minutos, 1, 3, 7, 14, 30, 60, 120 e 240 dias. Uma nova carta começa no nível 0/1/2 para Difícil/Médio/Fácil; uma carta existente se move −1/+1/+2 dentro de 0–8. O host Rust escolhe o dia civil local e instantâneo UTC e escreve atomicamente o próximo estado. Somente o seu sucesso ACK avança o painel e incrementa a atividade diária; a falha deixa o cartão, o mapa de calor e o estado persistente inalterados, e cliques duplos não podem enviar uma segunda classificação.

O Hub distingue cartões de origem de direções de revisão: **Cartões** é o número definido em Markdown, enquanto **Revisões** é o número de direções que podem ser agendadas. Assim, `A :: B` mais `C ::: D` são 2 cartas e 3 avaliações. Também mostra avaliações vencidas e novas, notas com cartões, avaliações de hoje, sequência atual e sequência mais longa. Seu mapa de calor de 365 dias usa níveis fixos (0, 1–4, 5–9, 10–19, 20+) e cada célula nomeia sua data e contagem de revisões. A cor é complementar. A tendência atual permanece viva hoje, quando o último estudo foi ontem; a sequência mais longa é derivada de datas civis, e não persistente.

O cabeçalho também carrega Zoom −/+, que usa o caminho de zoom existente e limites de 75–300, e um ícone de lixeira imediatamente ao lado de X. A lixeira abre apenas a confirmação recuperável existente; X permanece Fechado. Em larguras estreitas medidas, os atalhos opcionais de deck, imagem, zoom e lixeira aparecem antes do Menu, ativar Timer/AutoPaste ou Fechar, e todos ficam ocultos em uma nota recolhida.

## AutoPaste da área de transferência

Copie algo em qualquer lugar da máquina e ele aparecerá no final da nota que você escolheu. Nenhuma janela aparece, nenhuma tecla é pressionada para você e nada ocupa o seu cursor.

> **Isso não é *Colar URL na seleção*.** Esse — selecione algumas palavras, cole um URL, obtenha um link —
> é um recurso diferente e ainda está onde estava. AutoPaste é um modo de captura.

**Desativado, sempre, até que você diga o contrário.** O AutoPaste está desativado quando Note-it é iniciado, e ativá-lo é uma decisão que você toma em *☰ › Captura*. Enquanto estiver desligado, não há nenhum manipulador de área de transferência conectado: nada é observado, lido, hash, armazenado, registrado ou enviado. Isso é uma propriedade do acordo e não uma promessa sobre ele – não há nada assinado para ser observado.

**Ele não volta sozinho.** Se o AutoPaste estava ativado não está escrito em lugar nenhum - nem na nota, nem em `state.json`, nem em `config.toml`. Uma reinicialização, um logout, uma falha ou uma atualização o deixam desativado e você decide novamente. Um modo que observa o que você copia nunca deve ser retomado sem ser solicitado, e a única maneira de garantir isso é não ter nada para retomar.

**Uma nota por vez.** A área de transferência do sistema é uma coisa, então exatamente uma nota pode ser o alvo. Ativá-lo em uma segunda nota desliga-o na primeira, na mesma etapa, e a barra e o menu da primeira nota param de reivindicá-lo.

**O que captura.** Texto. Uma imagem, arquivo ou formato desconhecido copiado é recusado dos formatos oferecidos pela área de transferência, sem que um byte dela seja lido. Uma cópia vazia ou em branco não arquiva nada – nenhuma linha, nenhum delimitador, nenhuma data de modificação. E a área de transferência como estava *antes* de você ativar o modo nunca é capturada: apenas uma alteração após esse momento conta, então o que quer que estivesse lá permanece onde estava.

**Onde pousa.** No final da nota, sempre. Não no cursor e nem sobre a seleção: você está em outro aplicativo, portanto, o cursor nessa nota está onde você a deixou e não significa "inserir aqui". A nota não tira foco, não rola, não vem para frente e não muda de camada. Se você estiver olhando, verá o texto chegar; isso é tudo o que acontece.

**Como texto, exatamente.** Uma captura é uma colagem de texto simples, com o mesmo significado que um `Ctrl+V` tem aqui: `**isso é literal**` permanece como asteriscos, `<script>alert(1)</script>` permanece com onze caracteres e um URL copiado permanece como um URL que você pode ler. Nada é buscado – nenhuma pesquisa de título, nenhuma visualização, nenhum favicon – então o AutoPaste funciona com a rede desligada. Acentos, emoji, 日本語 e cópias de várias linhas permanecem inalterados.

**Uma captura, um desfazer.** `Ctrl+Z` recupera toda a última captura, delimitador e tudo, não um caractere por vez.

**Separando capturas.** *☰ › Captura › Separar capturas* oferece três:

| | Entre uma captura e outra |
|---|---|
| **Linha** | a próxima linha do mesmo parágrafo |
| **Linha em branco** | um parágrafo próprio – o padrão |
| **Separador** | uma regra horizontal |

Exatamente um é aplicado entre cada par, nunca dois, e nunca antes da primeira captura em uma nota vazia. A alteração da preferência aplica-se à próxima captura e não reescreve nada já escrito. A escolha é lembrada nas reinicializações, porque diz como você gosta das capturas e nada sobre o que você copiou.

**Isso não devolverá à nota suas próprias palavras.** Copiar ou recortar dentro da nota que está capturando não anexa o que você acabou de copiar. Isso não é uma comparação de texto - é a resposta do próprio kit de ferramentas para "este aplicativo colocou isso na área de transferência", verificada antes de qualquer leitura começar. A distinção é importante: copiar `ABC` duas vezes de outro aplicativo, em duas ações separadas, arquiva-o duas vezes, porque você o solicitou duas vezes.

**Enquanto estiver ativado** a nota mantém sua barra com um 📋 ao lado dos outros controles, de modo que um modo que monitora cada cópia nunca seja executado de forma invisível. O indicador também está na barra de uma nota recolhida e pressioná-lo abre o painel que o desliga.

**O que isso nunca faz:** apropriar-se da área de transferência (após uma captura, o que você copiou ainda cola normalmente em qualquer outro lugar), manter um histórico do que você copiou, acessar a rede, gravar o conteúdo da área de transferência em qualquer registro ou colocar um marcador próprio em sua nota. Uma captura é um conteúdo comum quando chega - pesquisável, excluível e parte do próprio título da nota, se a nota estiver vazia.

**Ele desliga sozinho** quando a nota é fechada, enviada para a lixeira, quando Note-it está oculto e quando sai - antes de qualquer uma delas terminar, então uma leitura ainda em andamento não pode chegar depois. Recolher a nota, alterar a camada ou mudar para outro aplicativo, deixe-o ativado; esse último é para que serve o modo.

## Timer e Pomodoro

Uma contagem regressiva na nota em que você está trabalhando, sem sair dela e sem segunda janela.

**Onde.** O botão ⏱ no final da barra de cabeçalho abre um pequeno painel abaixo dela. Existem dois modos no painel e uma contagem regressiva por nota: uma nota executa um Timer ou um Pomodoro, nunca ambos, então as guias de modo ficam indisponíveis enquanto uma execução está ao vivo, em vez de ser uma forma de terminar com duas.

**Temporizador.** Sete predefinições — 5, 10, 15, 25, 30, 45 e 60 minutos — e um campo para qualquer outra coisa de 1 a 600 minutos inteiros. Uma duração que não é uma dessas é recusada e o diz; nada é arredondado para o intervalo, porque um cronômetro que funcionou silenciosamente por um período que você não escolheu é pior do que um que se recusou a iniciar. `Enter` no campo inicia.

**Pomodoro.** O ciclo clássico: 25 minutos de foco, 5 minutos de intervalo curto e um intervalo longo de 15 minutos após a quarta sessão de foco, após o qual a contagem começa novamente. O painel mostra em qual fase você está, em qual sessão das quatro e quatro notas do ciclo.

**Nada começa sozinho.** Quando uma fase termina ela é marcada como concluída e a *próxima* é oferecida no botão — "Iniciar pausa curta" — para você começar quando estiver pronto. Uma pausa que começasse sozinha enquanto você ainda estava no meio da frase seria um Pomodoro com o qual você nunca concordou. *Pular etapa* passa para a próxima etapa sem esperar por esta.

**Iniciar, pausar, continuar, cancelar.** Somente os controles aplicáveis ​​são mostrados, portanto, não há Pausa em um cronômetro pausado e nem Continuação em um que nunca foi iniciado. Cancelar um temporizador mantém a duração que você escolheu; cancelar um Pomodoro mantém o seu lugar no ciclo, e *Reiniciar ciclo* é o que volta ao início.

**É honesto sobre o tempo.** Uma contagem regressiva em execução é armazenada como o *instante em que termina*, não como um número que algo precisa ser diminuído. Cada leitura é aquele instante menos o relógio agora, então nada muda e nada é perdido para um WebView que foi acelerado, uma máquina que estava ocupada ou um laptop que foi desligado por dez minutos. Suspenda a máquina por dez minutos faltando quinze e você volta para cinco. A pausa é o espelho: o instante é descartado e o restante congelado, portanto o tempo de pausa não pode ser gasto – nem enquanto a nota estiver oculta, nem enquanto o aplicativo estiver fechado.

**Ele sobrevive à nota indo embora.** Recolha a nota, esconda tudo, feche o aplicativo e volte: uma corrida é retomada com o tempo que realmente passou já decorrido, e aquela cujo fim já passou volta **terminada** em vez de contar até zero. Uma execução que terminou enquanto o aplicativo não estava aberto não toca quando você retorna – um alarme sobre o passado não é um alarme – mas o estado finalizado está ali na barra.

**Em uma nota recolhida** a barra mantém o relógio ao lado de ⏱, próximo ao nome da nota, para que uma contagem regressiva em execução nunca precise que a nota seja expandida para ser confiável. Numa nota estreita demais para conter ambos, os dígitos cederam e o ícone permaneceu; o nome e o botão Fechar nunca funcionam.

**Quando termina** o relógio indica `00:00`, a barra e o painel dizem *Concluído*, uma linha no final da nota indica o que terminou e a área de trabalho recebe uma notificação — "Timer concluído" ou "Pomodoro — Sessão de foco concluída." A notificação não traz nada da nota: nem seu título, nem uma linha de seu texto. Exatamente um é enviado por execução, independentemente do tempo em que a nota fique em zero. Uma área de trabalho sem daemon de notificação simplesmente não recebe notificação; nada sobre o cronômetro depende disso.

**Um cronômetro não faz parte da nota.** Ele nunca é escrito no Markdown — nenhum comentário, nenhuma chave de front matter, nenhum marcador. Iniciar, pausar, terminar ou cancelar deixa o arquivo de notas byte por byte como estava e deixa sua data de modificação onde estava, para que uma nota com um cronômetro não salte para o topo do alternador rápido. É invisível para pesquisa, para o título recolhido e para a lixeira: pesquisar `25:00` não encontrará uma nota apenas porque tem um Pomodoro de 25 minutos em execução. O estado fica ao lado da geometria da janela em `state.json` e é escrito apenas quando algo realmente acontece – um início, uma pausa, uma retomada, um cancelamento, uma mudança de fase ou uma conclusão. Uma contagem regressiva em execução não escreve absolutamente nada, uma vez por segundo ou não.

## Controles de visualização

- **Zoom da nota (`Ctrl+=` / `Ctrl+-` / `Ctrl+0`):**
  - Dimensiona o conteúdo da nota entre 75% e 300% em passos de 10%, sem alterar o tamanho da janela,
o Markdown ou a data de modificação da nota. A barra de cabeçalho mantém seu tamanho.
  - Persistido por nota em `state.json`; notas sem zoom armazenado abrem em 100%.
- **Escala da interface (menu):**
  - Dimensiona o chrome do aplicativo de 90% a 160% em etapas de 10%: alvos de acertos da barra de ferramentas, texto do menu,
Os controles SearchPalette, Find, Trash, Timer, Study e image recebem métricas de layout reais.
  - Compartilhado por todas as notas abertas e persistido uma vez em `config.toml`; uma configuração mais antiga padrão
para 100%. Não afeta o documento, suas marcas de tamanho de texto, zoom por nota ou `updated_at`.
  - A altura do hospedeiro de uma nota recolhida segue a mesma escala enquanto sua geometria expandida permanece
armazenado inalterado.
- **Tema (menu):**
  - Sistema / Claro / Escuro, aplicado imediatamente a todas as notas abertas e persistido globalmente.
- **Camada (`Ctrl+Shift+Space`):**
  - Alterna entre **Sempre no topo** (acima de outras janelas) e **Área de trabalho** (atrás
eles, ainda abertos). Esta é a mesma opção para todo o aplicativo que `note-it toggle`.
- **Recolher (`Ctrl+Shift+M`):**
  - A mesma ação da entrada do menu, reduzindo a nota à barra de cabeçalho e vice-versa. Aplica-se a
apenas a nota focada.
- **Recolher tudo (`note-it toggle-collapse-all`):**
  - Recolhe todas as notas ainda expandidas e expande todas elas quando todas estão recolhidas. Cada
note mantém seu próprio sinalizador recolhido e tamanho expandido.
- **Uma nota recolhida se expande quando clicada:**
  - Clicar em qualquer lugar da barra restaura o tamanho anterior. O botão Fechar ainda
fecha, arrastar a barra ainda a move, e o botão `☰` expande a nota e abre seu menu
em um único clique.

## Experiência de edição

- **Formatação rica em WYSIWYG:**
  - Parágrafos e títulos (H1, H2, H3)
  - Negrito, Itálico, Sublinhado (`<u>`)
  - Cor do texto semântico (`<span data-note-it-color="...">`) de uma paleta compacta
  - Marcador de destaque (`<mark data-note-it-highlight="...">`) de uma paleta compacta, sempre desenhado
com um primeiro plano escuro para que o texto destacado permaneça legível em todas as cores de papel
  - Tamanhos de texto discretos (12–32 px) aplicados a uma seleção, independentemente dos títulos e do zoom
  - Listas com marcadores e listas numeradas
  - Listas de verificação interativas (`- [ ]` / `- [x]`)
  - Digitar `->` torna-se um `➜` real, armazenado como o próprio caractere, em vez de depender de uma fonte
com ligaduras e deixado intacto dentro do código embutido e dos blocos de código
  - Blockquotes e código / blocos de código embutidos
- **Escala de fonte:**
  - O tamanho base da fonte da nota é armazenado em seu front matter e aplicado quando a nota é carregada.
`Ctrl+=` / `Ctrl+-` direcionam o zoom da visualização em vez desse tamanho base.
- **Temas de papel:**
  - 7 cores de papel pastel suave selecionadas: Amarelo, Azul, Verde, Rosa, Roxo, Cinza, Preto (com texto claro de alto contraste).
- **Atalhos de teclado:**
  - `Ctrl+N` para criar uma nova nota em cascata.
  - `Ctrl+W` para salvar e descartar a nota atual.
  - `Ctrl+K` para pesquisar cada nota, `Ctrl+F` para encontrar nesta, `Ctrl+H` para localizar e substituir.
Todos os três estavam livres antes da Fase 3.8 e não colidiram com nada acima.

## Armazenamento e confiabilidade

### Tags e propriedades

- Tags e propriedades textuais são estruturadas em nível superior YAML ao lado do bloco reservado `note_it`, nunca no conteúdo do corpo Markdown. Os campos ausentes ficam vazios na memória e omitidos no disco.
- Core possui validação e identidade de pesquisa compartilhada: até 32 tags (64 caracteres cada) e 32 propriedades (chaves de 64 caracteres, valores de 512 caracteres). As entradas acima de um limite são rejeitadas, nunca truncadas; identidades de tags duplicadas são reduzidas à primeira ortografia e identidades de chave de propriedade duplicadas são rejeitadas.
- Uma entrada **Metadados** abre o único editor. As tags são pílulas de cores determinísticas acessíveis em uma única linha responsiva; As propriedades ficam dentro do painel de rolagem interna. O preenchimento automático é derivado sob demanda de notas ao vivo e nunca escreve por sugestão.
- Os valores semânticos são inseridos com APIs de texto/valor DOM e nunca se tornam HTML, estilo, classe, URL ou identificadores DOM arbitrários. Eles não inserem ProseMirror, texto visível, pesquisa, títulos, estudo ou flashcards.
- Os metadados usam o mesmo redator de notas transacionais e a mesma política de backup antes da mutação. Um rascunho confirmado carrega o WebView Markdown atual, evitando que um documento host obsoleto substitua o texto pendente. As gravações somente de metadados preservam ambos os carimbos de data/hora.
- Varredura de catálogos ao vivo `notes/`; o lixo está naturalmente ausente e a restauração retorna naturalmente. Não existe índice, banco de dados ou arquivo secundário.

- **Exclusão recuperável:**
  - A exclusão de uma nota move seu arquivo para `trash/`, de onde ela pode ser restaurada com seu identificador,
seus bytes e sua data de modificação intactos. O salvamento vem primeiro: uma nota cujo texto não pôde ser
escrito nunca é movido.
- **Instantâneos locais:**
  - No máximo um backup automático a cada 24 horas, feito antes da primeira alteração após essa janela, mais
um manual a pedido. Sete são mantidos, os antigos são removidos somente após a conclusão de um novo.
- **Salvamento automático atômico:**
  - Gravação eliminada (300 ms) por meio de substituição temporária de arquivos e sincronização de diretórios para evitar corrupção de dados.
  - Fechar e `Ctrl+W` enviar o conteúdo mais recente do editor em uma solicitação de salvar e fechar; a janela fecha somente após a persistência ser bem-sucedida.
- **Liberação transacional ao ocultar e sair:**
  - `note-it hide` e `note-it quit` solicitam explicitamente o conteúdo do buffer mais recente de todos os WebViews ativos, cancelam rebotes e aguardam a confirmação de gravação atômica para cada nota antes de destruir superfícies ou sair.
  - Uma resposta WebView ausente, expirada ou inválida é uma falha de liberação; o host nunca substitui seu documento potencialmente obsoleto na memória como uma confirmação bem-sucedida.
  - Se alguma nota não for confirmada ou salva, a operação será abortada: hide mantém todas as superfícies abertas no modo anterior e quit mantém o daemon em execução. Sem a confirmação do conteúdo atual de WebView, nenhuma operação destrói superfícies ou saídas.
- **Front matter YAML padrão:**
  - ID da nota, cor do papel, tipo de papel, intensidade do padrão, tamanho da fonte e carimbos de data/hora armazenados de forma limpa
nos cabeçalhos das notas.
  - `created_at` é corrigido na criação; `updated_at` segue apenas edições de conteúdo, não de aparência ou
mudanças na janela. Uma nota sem carimbos de data e hora ainda será aberta e os reportará como desconhecidos.
  - Visitar uma nota não é editá-la: abrir e fechar, convocar, ocultar, mostrar ou sair
sem alterar o texto, deixa `updated_at` em paz e o arquivo não é reescrito.

## CLI headless (`noteit`)

- **Separação de preocupações:** `noteit` é um binário CLI independente e leve, sem GUI, GTK, WebKitGTK, Wayland ou dependências de servidor de exibição.
- **Orientação e orientação:** executar `noteit` sem argumentos gera uma tela de boas-vindas concisa com orientação para os comandos disponíveis.
- **Interface bilíngue e erros humanos:** comandos primários em português (`listar`, `ler`, `buscar`, `tags`, `propriedades`, `tarefas`, `lixeira`, `status`, `ajuda`, `versao`) com aliases internacionais canônicos (`list`, `read`, `search`, `properties`, `tasks`, `trash`, `help`, `version`, `status`, `--help`, `-h`, `--version`, `-V`). Erros de uso são apresentados como mensagens amigáveis ​​em português no stderr com código de saída 2.
- **Subcomandos de leitura sem cabeça API:**
  - `noteit listar` / `noteit list`: lista notas ao vivo em ordem canônica de atualidade com identificadores, rótulos, tags e carimbos de data/hora.
  - `noteit ler <ID>` / `noteit read <ID>`: lê e renderiza cabeçalho, metadados, propriedades e corpo da nota por UUID completo ou prefixo exclusivo (>= 8 caracteres hexadecimais).
  - `noteit buscar <Q>` / `noteit search <Q>`: pesquisa corporal sem distinção entre maiúsculas e minúsculas e acentos, retornando rótulos, snippets e contagens de ocorrências correspondentes.
  - `noteit tags`: lista o catálogo de tags derivadas com contagens de uso de notas ativas.
  - `noteit propriedades` / `noteit properties`: lista o catálogo de chaves de propriedades derivadas com contagens de uso de notas ativas.
  - `noteit tarefas` / `noteit tasks`: extrai tarefas agrupadas por nota, preservando hierarquia de profundidade, estado de caixa de seleção e datas de conclusão ISO 8601.
  - `noteit lixeira` / `noteit trash`: lista notas excluídas recuperáveis ​​na lixeira com carimbos de data e hora de exclusão.
- **Filtragem e Limitação:**
  - `--limite N` / `--limit N`: fixa a saída em 1..=100 resultados (padrão 20).
  - `--tag <TAG>`: filtro repetível aplicando AND booleano entre tags (sem distinção entre maiúsculas e minúsculas e acentos).
  - `--propriedade <K=V>` / `--property <K=V>`: filtro repetível aplicando AND booleano entre propriedades.
  - `--estado <ESTADO>` / `--state <STATE>`: filtragem de tarefas por estado (`pendentes`, `concluidas`, `todas` / `pending`, `completed`, `all`).
- **Limpeza de segurança do terminal:** todas as strings não confiáveis ​​renderizadas para stdout/stderr (conteúdo de notas, consultas de pesquisa, seletores, contextos de argumentos refletidos e caminhos XDG) são limpas antes do estilo e da saída, neutralizando sequências de escape ANSI (CSI, OSC, injeção de área de transferência OSC 52), BEL, backspace e caracteres de controle, preservando Unicode e Markdown válidos.
- **Consistência de fuso horário local:** carimbos de data/hora humanos em todos os subcomandos CLI (`listar`, `ler`, `tarefas`, `lixeira`) são formatados no fuso horário local da máquina (`dd/MM/yyyy HH:mm`) correspondente ao contrato GUI do desktop, enquanto os modelos Core permanecem estritamente UTC (`DateTime<Utc>`).
- **Desacoplamento de avisos digitados:** anomalias de leitura não fatais produzem itens `ReadWarning` digitados dentro de `ReadBatch<T>` em `noteit-core` sem impressão. O CLI os renderiza de forma limpa para stderr em português (`Aviso: ...`).
- **Leituras estritamente somente leitura:** todas as operações de leitura API inspecionam o store de forma puramente somente leitura, sem criar diretórios ausentes, arquivos de estado, backups — ou qualquer arquivo de coordenação de gravação. A leitura nunca é alugada e nunca abre uma tomada.
- **Subcomandos de gravação coordenada API:**
  - `noteit criar [TEXTO]` / `noteit create`: cria uma nota e responde com seu UUID. Aceita
`--stdin` para Markdown multilinha e `--tag` / `--propriedade` para aplicar metadados na criação.
Não abre janela, não foca e não registra nada como aberto — com ou sem Note-it em execução.
  - `noteit adicionar <ID> <TEXTO>` / `noteit append`: anexa Markdown ao final do corpo. O
a regra de junção é fixa e documentada: um corpo vazio se torna a carga útil; caso contrário, exatamente uma linha
break é inserido primeiro. A carga útil nunca é cortada ou refluída.
  - `noteit editar <ID> <TEXTO>` / `noteit edit`: substitui todo o corpo. Não é um `$EDITOR` — o texto
vem do argumento ou `--stdin`, nunca ambos. Esvaziar uma nota requer `--vazio`, então um
um cano vazio acidental não pode destruir um.
  - `noteit tags adicionar|remover <ID> <TAG>` / `tags add|remove`: a identidade da tag permanece maiúscula e minúscula
insensível ao sotaque; adicionar um já presente ou remover um ausente é um sucesso autônomo que
não reescreve nada.
  - `noteit propriedades definir|remover <ID> <K=V>` / `properties set|remove`: mesmas regras autônomas, com
todos os limites, manipulação de Unicode e identidade de chave decidida por Core. O CLI nunca analisa YAML.
  - `noteit tarefas concluir|reabrir <ID> <REF>` / `tasks complete|reopen`: completar escreve o
comentário canônico `<!-- note-it:completed_at=... -->` com fuso horário explícito; reabertura remove
apenas esse comentário, preservando recuo, marcador, aninhamento e comentários HTML de qualquer outra pessoa.
  - `noteit lixeira restaurar <ID>` / `trash restore`: restaura dados e nada mais — sem janela, não
foco, nenhuma camada ou alteração de geometria. Uma nota ativa com o mesmo identificador nunca é substituída.
- **Referências de tarefas:** `noteit tarefas` mostra uma referência de oito caracteres ao lado de cada tarefa. É um *token de instantâneo otimista*, não uma identidade: nada é armazenado, nenhum arquivo secundário é criado e é recalculado em relação à nota no momento da gravação. Se a tarefa mudar nesse meio tempo, o comando será recusado e você listará as tarefas novamente – muito melhor do que marcar silenciosamente uma tarefa diferente.
- **Exatamente um gravador por store:** as gravações são serializadas por um bloqueio de aconselhamento. Com Note-it em execução, a alteração é realizada pela instância em execução; sem ele, o CLI escreve diretamente através do Core. Dois comandos simultâneos sobrevivem. Se o store for mantida e seu proprietário não puder ser contatado, nada será escrito e o CLI diz isso - ele nunca escreve em torno de outro gravador.
- **Um Note-it em execução possui suas notas:** o aplicativo de desktop recebe o lease de escrita e abre seu canal de controle antes de abrir qualquer outra coisa. Se outro gravador mantém o store, ou o canal não pode ser aberto, isso explica o porquê em uma frase e não inicia - nenhuma janela, nenhuma nota, nenhum salvamento automático, nada escrito. Nunca funciona como um segundo gravador.
- **A janela confirma, não é presumido:** depois que uma alteração é confirmada, a nota na tela diz que ela foi adotada. Até que isso aconteça, o comando relata a alteração conforme escrita *e* avisa que a janela ainda pode estar mostrando o texto antigo — para que ninguém repita uma alteração que já aconteceu. Uma escrita que demora um pouco diz isso e mantém a nota segura; ele nunca é devolvido no meio do commit.
- **Uma janela que fica para trás para em vez de fingir:** no raro caso em que uma alteração é gravada no disco, mas a nota aberta não pode aceitá-la, a nota é retida e diz isso ("A alteração foi gravada, mas esta janela não conseguiu acompanhá-la. Reabra a nota."). Ele não volta a aceitar a digitação que não conseguiria salvar - um editor que descarta silenciosamente o trabalho é pior do que aquele que para visivelmente. A alteração está segura no disco; reabrir a nota traz a janela de volta exatamente, sem nada perdido e nada duplicado.
- **Nada que não foi salvo é perdido:** alterar uma nota que está aberta na tela congela seu editor *antes* de lê-la, dobra o texto que você digitou, mas ainda não salvou, no mesmo commit e devolve a nota enviada de volta para a janela. Uma edição que ainda não chegou ao disco nunca é substituída, e um salvamento automático já em andamento não pode desfazer a alteração quando ela chegar.
- **Timestamps Follow Significado:** anexar, editar e alternar uma tarefa move `updated_at` somente quando o corpo realmente mudou. Tags e propriedades não movem nenhum carimbo de data/hora — elas dizem sobre o que é uma nota *sobre*, não que ela foi editada. `created_at` nunca se move.
- **Resultados honestos:** uma gravação que falhou antes do ponto de confirmação não mudou nada e pode ser repetida com segurança. Uma gravação que foi confirmada, mas não conseguiu atualizar a janela, relata um aviso, nunca uma falha, portanto, ninguém anexa o mesmo parágrafo duas vezes. Uma conexão que caiu após a solicitação ser encerrada é relatada como desconhecida, em vez de adivinhada.
- **Nota escreve notas apenas por toque:** nenhum comando de gravação modifica `config.toml`, `state.json`, o cache, geometria, camada, tema ou zoom.
- **Apresentação e compatibilidade de terminal:** formatação limpa com estilo ANSI discreto em terminais interativos, voltando automaticamente para texto simples quando redirecionado, canalizado ou quando `NO_COLOR` é definido.
- **Códigos de saída padrão:** código de saída `0` para sucesso, `2` para sintaxe inválida ou argumentos desconhecidos e `1` para erros de execução.
