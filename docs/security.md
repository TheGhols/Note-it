# Segurança e higienização de conteúdo

## Princípios de segurança

Note-it lida com conteúdo local Markdown, mas como renderiza rich text em uma webview do WebKit, ele trata todas as entradas HTML com limpeza estrita:

1. **Lista de permissões HTML restrita:**
   - Tags embutidas permitidas: `<u>`, `<span data-note-it-color="...">`, `<mark data-note-it-highlight="...">`.
   - Atributos permitidos: `style="color: #..."`, `style="background-color: #..."`, `data-note-it-color`, `data-note-it-highlight`.
2. **Elementos e vetores bloqueados:**
   - `<script>`, `<iframe>`, `<object>`, `<embed>`, `<form>`, `<style>` (blocos independentes).
   - Atributos de evento (`onclick`, `onload`, `onerror`, etc.).
   - Esquemas URI executáveis ​​(`javascript:`, URIs `data:` perigosos).
3. **Links externos:**
   - Links em notas não navegam na webview do WebKit.
   - Clicar em um link envia uma solicitação ao host Rust para abrir o navegador padrão do sistema via `xdg-open`/GIO.
   - O host Rust analisa independentemente o URI e permite apenas `https:`, `http:` e `mailto:` antes de invocar GIO.
4. **Política de segurança de conteúdo (CSP):**
   - O webview impõe CSP estrito, proibindo scripts embutidos de fontes remotas e restringindo conexões de rede.

## O mecanismo matemático não tem avaliador

Os cálculos em uma nota são lidos por um lexer e um analisador descendente recursivo escrito apenas para aquela gramática (`ui/src/math/`). Não há `eval`, nem `Function`, nem importação dinâmica, nem cronômetro, nem acesso de propriedade e nem sintaxe de chamada em nenhum lugar dele, e nenhuma biblioteca foi adicionada para fornecer alguma. Uma expressão é uma sequência de dez formas de token e se torna uma árvore de seis tipos de nós, que é então percorrida para fazer aritmética.

É por isso que `= window.location`, `= process.exit()`, `= fetch(...)` e `= constructor.constructor("return 1")()` não são entradas a serem filtradas. Eles não podem ser escritos: a gramática não possui token para `.`, `[`, `"` ou uma chamada, então eles param no primeiro caractere que não é uma das formas acima e são relatados como uma expressão inválida.

Variáveis ​​são mantidas em `Map`, nunca em um objeto. Um objeto resolveria `constructor`, `__proto__`, `toString` e `valueOf` para valores reais JavaScript; um `Map` não tem chaves herdadas, portanto, um nome desconhecido é desconhecido, seja qual for o seu nome, e declarar um armazena uma chave em vez de chegar a um protótipo.

O comprimento da expressão, a contagem de tokens e a profundidade do aninhamento são todos limitados, portanto, uma colagem hostil ou acidental custa um valor fixo em vez da pilha. As mensagens de erro são sete constantes; nenhuma parte de uma nota é ecoada através dela.

As unidades são resolvidas da mesma maneira e pelo mesmo motivo. `ui/src/units/registry.ts` constrói um `Map` a partir de uma tabela literal e cada pesquisa passa por ela, então `= 10 constructor em m` e `= 10 km em __proto__` são unidades desconhecidas em vez de alcançar uma propriedade JavaScript. Nada é indexado dinamicamente a partir de um objeto host, e as duas conversões de caracteres adicionadas ao lexer — `°` para `°C` e `²`/`³` para `m²` e `cm³` — são caracteres identificadores e não concedem nenhum novo recurso. A regra para como uma *variável* pode ser chamada permanece inalterada e ainda ASCII.

Nada no mecanismo chega à rede e um teste afirma isso: não há `fetch`, não há `XMLHttpRequest`, não há `WebSocket`, não há `navigator`, não há armazenamento. Cada unidade Note-it convertida é uma constante, e é exatamente por isso que as moedas não estão entre elas.

## Pipeline de produção Markdown

O Markdown bruto continua sendo o formato de origem e nunca é transmitido por inteiro por meio de `DOMParser`. Antes de Tiptap analisá-lo, Note-it inspeciona apenas fragmentos HTML incorporados, remove blocos perigosos e tags não suportadas, canoniza as tags personalizadas suportadas e valida suas cores como HEX de 3 ou 6 dígitos. O mesmo validador HEX é usado pelos tokenizadores e serializadores Markdown personalizados. A área de transferência HTML é higienizada separadamente antes que ProseMirror a analise.

## A pesquisa lê notas; nunca os executa

Uma consulta é texto. A consulta passa por normalização de acentos (accent folding), conversão para minúsculas e correspondência como substring literal - não há mecanismo de regex, então `.*`, `[a-z]` e `(foo|bar)` são esses caracteres e custam o que esses caracteres custam. Nada é passado para um shell, para SQL ou para qualquer interpretador, porque não há ninguém para quem passá-lo.

Os limites são explícitos e dizem exatamente o que vinculam: 512 caracteres de consulta, 100 resultados, cerca de 240 caracteres de snippet. Uma consulta maior que o teto é recusada em vez de truncada, e um store de qualquer tamanho produz no máximo cem linhas. A varredura lê o corpo das notas em vez de carregar um WebView para cada uma – pesquisar mil notas cria zero WebViews adicionais.

**O que esses limites não limitam é a nota.** Uma nota é um arquivo de texto e qualquer coisa pode ser colada em um, e a pesquisa lê tudo: encontrar uma palavra no final de uma nota grande requer a leitura até o final de uma nota grande, e cortar esse trecho curto significaria um texto no store que nenhuma pesquisa poderia retornar. Portanto, uma única nota enorme custa o que custa o seu tamanho. Esse custo é medido e não limitado – mil notas totalizando cerca de 1,1 MB são pesquisadas em aproximadamente 40 ms, e uma única nota de 2 MB é pesquisada, com seus acentos intactos e sem escrever nada, em `a_very_large_note_is_searched_correctly_and_never_written`. Não há nenhuma garantia formal de que algum arquivo individual arbitrariamente grande não possa tornar um pressionamento de tecla lento, e este documento não reivindica isso.

**Um snippet é texto.** Rótulos e snippets são escritos com `textContent`, nunca com `innerHTML`. Uma nota contendo `<script>alert(1)</script>` ou `<img onerror=...>` mostra esses caracteres na lista de resultados; nenhum elemento é criado a partir deles e nada é executado. A nota é um arquivo que o usuário controla, e o resultado da pesquisa é uma renderização dele, não uma execução dele.

**A interface não pode nomear um arquivo.** Um resultado de pesquisa carrega um `note_id`, e a mensagem que o WebView envia de volta para abrir um carrega um `Uuid` — um caminho não pode ser escrito nele, então `../../etc/passwd` não é uma solicitação que existe. O host resolve o identificador por meio das mesmas regras de storage que todo o resto usa e relata uma nota perdida em vez de criar uma.

**A pesquisa não grava.** Nenhuma nota é salva, liberada ou reescrita para responder a uma consulta, nenhum movimento `updated_at` e não há arquivo de índice, portanto não há uma segunda cópia das notas do usuário no disco para proteger, fazer backup ou vazar.

## A lixeira e o backup nunca seguem o caminho da página

Cada ação de dados que a interface pode solicitar nomeia uma nota por identificador. As mensagens de ponte carregam um `Uuid`, que `serde` aceitará apenas como um, então `../../etc/passwd`, `notes/a.md` e `/home/…/state.json` não são solicitações que existem - elas falham ao serem analisadas antes que qualquer código as veja. O host constrói cada caminho sozinho, a partir dos próprios diretórios do store, exatamente como faz para abrir um resultado de pesquisa.

**Uma lista de lixeira é um texto.** Os rótulos e as visualizações são escritos com `textContent`, nunca com `innerHTML`. Seguem as mesmas regras dos resultados da pesquisa. Uma nota contendo `<script>alert(1)</script>` ou `<img onerror=…>` mostra esses caracteres na lista; nenhum elemento é criado a partir deles e nada é executado. Nada de uma nota na lixeira é analisado como Markdown ou como HTML, e nada em um backup é executado, aberto ou interpretado – um instantâneo é copiado e listado, nunca executado.

**Um backup nunca segue um link simbólico e nunca sai dos dois diretórios que foi solicitado a copiar.** Cada candidato é verificado com `symlink_metadata` e apenas os arquivos normais são copiados; links simbólicos são ignorados e relatados, diretórios não são baixados e nomes que começam com `.` são ignorados - o que também impede um `.tmp.…` de ser interrompido no salvamento de um instantâneo. Uma entrada criada dentro do store, portanto, não pode fazer a cópia de backup `/etc`, `/home`, um ponto de montagem ou qualquer outra coisa fora de `notes/` e `trash/`. `config.toml` e `state.json` são verificados da mesma maneira, portanto, um arquivo de configuração substituído por um link para outra coisa é ignorado em vez de copiado.

**A varredura de rascunho remove apenas seu próprio rascunho.** A limpeza após um backup interrompido exclui diretórios em `backups/` cujo nome começa com `.tmp.` e nada mais — nem um instantâneo, nem um arquivo, nem qualquer coisa que uma pessoa tenha colocado lá. Confundir o arquivo de um usuário com detritos seria uma falha pior do que os detritos.

**Rede zero, ainda.** A lixeira e o backup não adicionaram nenhum cliente HTTP, nenhum soquete e nenhum serviço. Nada chega fora da máquina e nada usa `eval` ou `Function`.

## Colar um URL cria um link através de um portão

Colar um URL sobre o texto selecionado transforma esse texto em um link, e o URL é avaliado por `safeLinkUrl` — a mesma lista de permissões que o restante do aplicativo usa. `http`, `https` e `mailto` são aprovados; todo o resto, `javascript:`, `data:`, `file:`, `vbscript:` e `ftp:` entre eles, é colado como texto comum. Espaços em branco, caracteres de controle, uma string somente de esquema e um `http://` sem host são todos recusados.

Há deliberadamente exatamente uma opinião no aplicativo sobre o que é uma URL. O próprio `linkOnPaste` de Tiptap está desligado, porque usa `linkifyjs` — um segundo analisador, com uma resposta diferente, que aceita esquemas que esta aplicação não permite. Um teste afirma que colar `ftp://…`, `ssh://…` ou `www.…` não produz nenhum link.

Nada é buscado. Nenhum título, nenhum favicon, nenhum OpenGraph, nenhuma visualização e nenhum cliente HTTP foram adicionados: a área de transferência já contém tudo o que o recurso precisa, portanto, o recurso não adiciona nenhuma superfície de rede.

## Uma resposta MCP é limitada, e uma leitura é integral ou é recusada

O servidor MCP publica notas para um agente, e um agente é um programa com uma
janela de contexto e um host do outro lado. Duas propriedades separadas
protegem esse fio, e elas se protegem uma à outra.

**Toda resposta tem envelope finito.** Cada listagem tem um teto de itens e cada
texto por item tem um teto de caracteres — 100 resultados, 50 candidatos, 240 de
snippet, 120 de rótulo, 100 entradas de lixeira, 20 warnings. `noteit_read` é a
única superfície sem nada dessa forma para contar, porque devolve uma nota
inteira, e por isso tem um teto sobre a própria resposta: 4 MiB do
`CallToolResult` serializado, medido no fio e não em `content.len()`. O payload
é publicado duas vezes pelo SDK e o escape JSON expande o que a nota contém —
2,04× o corpo em ASCII e 2,88× em texto de aspas, contrabarras e emoji, medido —
então contar bytes crus subestimaria a resposta em mais do que o dobro.

**Uma leitura entrega o estado inteiro ou não entrega revision nenhuma.** A
`revision` que `noteit_read` publica autoriza a próxima escrita, e autoriza uma
escrita sobre *o estado que ela nomeia*. Devolver parte de uma nota junto da
revision do todo daria a um agente permissão para gravar sobre um texto que ele
nunca viu. Então acima do teto a recusa é `response_too_large` e não carrega
corpo, revision, metadados, caminho nem pedaço de conteúdo: não há estado
parcial que possa ser confundido com um inteiro, e não há token para gravar a
partir dele. Não existe leitura paginada — seria protocolo novo, e um protocolo
novo é onde essa propriedade voltaria a ser opcional.

## Uma mensagem pública é uma frase que o servidor escreveu

Todo `message` que o MCP publica é uma constante escolhida pelo `code`, e isso é
garantido pelo tipo: uma recusa recebe `&'static str`, então uma frase montada
em tempo de execução não tem como chegar ali.

A regra existe porque a alternativa foi medida. As frases eram o `Display` do
Core, e o Core as escreve para quem está depurando um store: elas nomeiam o
arquivo. Um `noteit_list` sobre um store com um único link simbólico publicava
o caminho absoluto do diretório de notas; uma nota com um escalar de trezentos
kilobytes no front matter publicava trezentos kilobytes dele, porque a mensagem
citava o parser; um seletor de trezentos kilobytes voltava inteiro. Um warning
agora é `code` e `note_id` e nada mais, em todas as leituras — a decisão que o
Context Engine já tinha tomado na 4.2C, aplicada às outras quatro.

O que se perde é uma frase que ninguém podia usar para decidir nada. O `code` diz
o que aconteceu, o `note_id` diz onde olhar, e quem precisa reparar o arquivo
tem o arquivo.

## E o que recusa um argumento também é uma frase que o servidor escreveu

A regra acima valia para tudo que o **domínio** diz, e o domínio só fala depois
que os argumentos foram desserializados. Antes disso há outra fronteira, e ela
tinha a falha inteira: o extractor de parâmetros do SDK respondia uma falha de
desserialização com a frase do `serde_json`, que cita o valor que não entendeu
por inteiro. Medido no fio, contra o binário real: um `limit` recebendo
trezentos kilobytes de string voltava em 307 361 bytes, com o canário. O mesmo
para uma variante de enum, um booleano, uma lista — e, uma camada acima, para um
`method` desconhecido de trezentos kilobytes, que o SDK devolvia pelo nome.

Agora os argumentos entram por um extractor deste repositório, que **descarta o
erro sem lê-lo** e responde uma constante; e o método de uma requisição que não
roteia é recusado sem ser nomeado. As mesmas chamadas respondem em 112 e 103
bytes, e um argumento de 1 KiB e outro de 1 MiB recebem exatamente o mesmo
número de bytes — que é a propriedade, mais forte que um teto: o tamanho da
recusa não é do cliente.

O que se perde é o nome do campo dentro da recusa. Ele vinha do `serde_json`, e
o `serde_json` só o dá dentro da frase que repete a entrada. Os campos
obrigatórios de cada tool estão publicados no `inputSchema`. Consulte a
ADR-055.
