# Segurança e higienização de conteúdo

## Princípios de segurança

O Note-it foi projetado com uma abordagem rigorosa em relação à segurança de conteúdo e isolamento de processos:
- Toda entrada do usuário através da interface gráfica ou linha de comando passa por validação estrita.
- Zero conexões externas de rede em runtime: todo processamento é 100% local.
- Sanitização rigorosa de Markdown/HTML para prevenir injeções de script (XSS) no WebView.
- Comunicação IPC via mensagens JSON estritamente tipadas e limitadas.

## O mecanismo matemático não possui avaliador

O mecanismo matemático do Note-it (`ui/src/math/`) calcula sem chamar `eval`, `new Function`, `setTimeout` ou qualquer interpretador JavaScript. Não há importação dinâmica, temporizadores, acesso a propriedades ou sintaxe de chamada em lugar algum de seu código, e nenhuma biblioteca externa foi adicionada para fornecê-los. Uma expressão é uma sequência de dez formatos de tokens e se transforma em uma árvore de seis tipos de nós, que é então percorrida para realizar cálculos aritméticos.

É por isso que expressões como `= window.location`, `= process.exit()`, `= fetch(...)` e `= constructor.constructor("return 1")()` não são entradas a serem filtradas por lista negra. Elas simplesmente não podem ser representadas na gramática: a linguagem não possui tokens para `.`, `[`, `"` ou chamadas de função; portanto, o parser interrompe a leitura no primeiro caractere que não corresponda às formas suportadas e reporta uma expressão inválida.

As variáveis são armazenadas em um `Map`, nunca em um objeto convencional. Um objeto resolveria propriedades como `constructor`, `__proto__`, `toString` e `valueOf` para valores reais do runtime JavaScript; um `Map` não possui chaves herdadas do protótipo, de modo que um nome desconhecido é tratado como desconhecido qualquer que seja seu nome, e declarar uma variável simplesmente armazena uma chave em vez de acessar um protótipo.

O comprimento da expressão, a quantidade de tokens e a profundidade de aninhamento são todos estritamente limitados, garantindo que uma colagem hostil ou acidental consuma um custo fixo previsível em vez de esgotar a pilha de execução (call stack). As mensagens de erro consistem em sete constantes fixas; nenhuma parte do conteúdo da nota é refletida de volta nas mensagens de erro.

As unidades são resolvidas da mesma forma e pelo mesmo motivo. `ui/src/units/registry.ts` constrói um `Map` a partir de uma tabela literal e cada consulta passa por ele; assim, expressões como `= 10 constructor em m` e `= 10 km em __proto__` são tratadas como unidades desconhecidas em vez de acessar propriedades JavaScript. Nada é indexado dinamicamente a partir de um objeto host, e as duas conversões de caracteres adicionadas ao lexer — `°` para `°C` e `²`/`³` para `m²` e `cm³` — são tratadas como caracteres identificadores e não concedem novas capacidades de execução. A regra para a nomenclatura de *variáveis* permanece inalterada e estritamente restrita a ASCII.

Nada no mecanismo acessa a rede, e testes automatizados garantem isso: não há chamadas a `fetch`, `XMLHttpRequest`, `WebSocket`, `navigator` ou armazenamento web (storage). Cada unidade convertida pelo Note-it é uma constante estática, razão exata pela qual taxas de câmbio e moedas não estão incluídas entre elas.

## Pipeline de produção Markdown

O Markdown bruto permanece como o formato canônico de origem e nunca é repassado integralmente por meio de `DOMParser`. Antes que o Tiptap o processe, o Note-it inspeciona apenas fragmentos HTML incorporados, remove blocos perigosos e tags não suportadas, canoniza as tags personalizadas permitidas e valida suas cores como valores HEX de 3 ou 6 dígitos. O mesmo validador HEX é compartilhado pelos tokenizadores e serializadores personalizados de Markdown. O conteúdo HTML vindo da área de transferência é sanitizado separadamente antes que o ProseMirror o processe.

## A busca lê as notas; nunca as executa

Uma consulta de busca é tratada puramente como texto. Ela passa por normalização de acentos (accent folding), conversão para letras minúsculas e correspondência como substring literal — não há motor de expressões regulares (regex), portanto caracteres como `.*`, `[a-z]` e `(foo|bar)` são tratados como caracteres literais e custam exatamente o custo desses caracteres. Nada é repassado a um shell de sistema, banco SQL ou interpretador externo, pois nenhum desses existe na cadeia de busca.

Os limites de busca são explícitos e delimitam com exatidão seu escopo: 512 caracteres máximos para a consulta, 100 resultados no máximo e cerca de 240 caracteres por snippet (trecho de texto). Uma consulta que exceda o limite máximo é recusada em vez de truncada, e um store de qualquer volume produz no máximo cem linhas de resultado. A varredura lê diretamente o corpo textual das notas em vez de instanciar WebViews adicionais — pesquisar mil notas cria zero WebViews extras.

**O que esses limites não restringem é o tamanho individual da nota.** Uma nota é um arquivo de texto no qual qualquer conteúdo pode ser colado, e a busca lê a nota por completo: encontrar uma palavra ao final de uma nota extensa requer a leitura até o final do arquivo, e truncar essa leitura impediria que termos legítimos dentro do store fossem encontrados. Portanto, uma nota individual massiva custa o tempo correspondente ao seu tamanho. Esse custo é mensurado e não artificialmente truncado — mil notas totalizando cerca de 1,1 MB são pesquisadas em aproximadamente 40 ms, e uma única nota de 2 MB é pesquisada com acentuação preservada e sem disparar gravações em disco no teste `a_very_large_note_is_searched_correctly_and_never_written`. Não há garantias formais de que um arquivo arbitrariamente gigantesco não possa tornar uma digitação lenta, e este documento não alega o contrário.

**Um snippet é texto puro.** Rótulos e snippets são inseridos no DOM utilizando `textContent`, nunca `innerHTML`. Uma nota contendo `<script>alert(1)</script>` ou `<img onerror=...>` exibe esses caracteres literais na listagem de resultados; nenhum elemento HTML é instanciado a partir deles e nenhum código é executado. A nota é um arquivo controlado pelo usuário, e o resultado da busca é uma renderização textual, não uma execução.

**A interface gráfica não pode nomear caminhos de arquivo.** Um resultado de busca transporta apenas um `note_id`, e a mensagem enviada pelo WebView para abrir a nota carrega exclusivamente um `Uuid` — não é possível especificar caminhos arbitrários nela, de modo que strings como `../../etc/passwd` não são requisições existentes. O host resolve o identificador através das mesmas regras de storage utilizadas por todo o sistema e relata uma nota ausente caso o UUID não exista, em vez de criar arquivos arbitrários.

**A busca é estritamente somente leitura.** Nenhuma nota é gravada, sincronizada com disco ou reescrita para responder a uma consulta; nenhum valor de `updated_at` é alterado e não há criação de arquivos de índice, evitando que uma cópia secundária das notas do usuário resida em disco demandando proteção ou backups adicionais.

## A lixeira e o backup nunca aceitam caminhos vindos da página

Toda ação sobre dados solicitada pela interface gráfica referencia uma nota exclusivamente por seu identificador. As mensagens IPC pela bridge transportam um `Uuid` tipado, que a biblioteca `serde` aceita apenas no formato válido; requisições contendo `../../etc/passwd`, `notes/a.md` ou `/home/…/state.json` simplesmente falham no parsing antes que qualquer lógica do Core as processe. O host constrói todos os caminhos no sistema de arquivos por conta própria a partir dos diretórios internos do store, exatamente como faz ao abrir um resultado de busca.

**A listagem da lixeira é texto puro.** Rótulos e pré-visualizações são inseridos exclusivamente com `textContent`, nunca com `innerHTML`, seguindo rigorosamente as mesmas regras da busca. Uma nota contendo `<script>alert(1)</script>` ou `<img onerror=…>` exibe esses caracteres literais na listagem; nenhum elemento DOM é criado a partir deles e nada é executado. Nenhum conteúdo de uma nota na lixeira é processado como Markdown ou HTML durante a listagem, e nada em um backup é executado, aberto ou interpretado — um snapshot é copiado e listado, nunca executado.

**Um backup nunca segue links simbólicos e nunca ultrapassa os diretórios autorizados a copiar.** Cada arquivo candidato é verificado com `symlink_metadata`, e apenas arquivos regulares são copiados; links simbólicos são ignorados e reportados como warnings, diretórios não são percorridos recursivamente além da estrutura conhecida, e arquivos iniciados por `.` são ignorados — o que também impede que arquivos temporários residuais (`.tmp.…`) de salvamentos interrompidos entrem no snapshot. Uma entrada forjada dentro do store não consegue fazer o backup copiar `/etc`, `/home`, pontos de montagem ou qualquer caminho fora de `notes/` e `trash/`. `config.toml` e `state.json` são validados da mesma forma; portanto, um arquivo de configuração substituído por um symlink é ignorado em vez de copiado.

**A limpeza de arquivos temporários remove apenas seus próprios temporários.** A limpeza após um backup interrompido remove exclusivamente diretórios dentro de `backups/` cujo nome comece com o prefixo `.tmp.`, ignorando qualquer snapshot legítimo, arquivo ou conteúdo criado manualmente pelo usuário. Tratar arquivos legítimos do usuário como resíduos temporários seria uma falha mais grave do que manter os próprios arquivos residuais.

**Zero conexões de rede.** As operações de lixeira e backup não introduziram nenhum cliente HTTP, socket de rede ou serviços remotos. Nenhuma operação ultrapassa os limites da máquina local e nenhuma lógica utiliza `eval` ou `Function`.

## Colar uma URL cria um link passando por um único gate

Colar uma URL sobre um texto selecionado transforma esse texto em um link clicável, sendo a URL avaliada pela função `safeLinkUrl` — a mesma allowlist utilizada por toda a aplicação. Apenas os esquemas `http`, `https` e `mailto` são aceitos; quaisquer outros, incluindo `javascript:`, `data:`, `file:`, `vbscript:` e `ftp:`, são colados como texto puro comum. Espaços em branco, caracteres de controle, strings contendo apenas esquemas sem destino e URIs como `http://` sem host são integralmente recusados.

A aplicação adota deliberadamente uma definição única e estrita sobre o que constitui uma URL segura. A funcionalidade nativa `linkOnPaste` do Tiptap permanece desativada por depender do `linkifyjs` — um parser secundário que aceita esquemas e formatos não permitidos pelo Note-it. Testes automatizados garantem que colar strings como `ftp://…`, `ssh://…` ou `www.…` não gera links automáticos.

Nenhuma requisição externa de rede é disparada. Não são realizadas buscas de títulos, favicons, metadados OpenGraph ou pré-visualizações, e nenhum cliente HTTP foi incluído: a área de transferência já contém todo o conteúdo necessário para a funcionalidade, mantendo a superfície de ataque de rede estritamente nula.
