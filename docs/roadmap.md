# Roadmap do Note-it

## Fase 0: Fundação Pública (Concluída)
- [x] Inicialização do repositório, `.gitignore`, licenciamento e documentação.
- [x] Rust e TypeScript constroem estrutura inicial.
- [x] Arquitetura do projeto e especificação de storage.

## Fase 1: Fatia Vertical e Integridade Markdown (Concluída)
- [x] Trabalhando na janela de notas GTK4 + `gtk4-layer-shell` + WebKitGTK 6.0.
- [x] Ponte bidirecional IPC entre o host nativo e o editor de webview.
- [x] Carregue e salve automaticamente os arquivos `.md` com YAML front matter.
- [x] ProseMirror / Tiptap 3 Markdown serializador e sanitizador de ida e volta.
- [x] Preservação de código nativo Markdown (blocos protegidos, extensões inline e sintaxe literal).
- [x] Pipeline de ações GitHub CI em execução nativa no ambiente de contêiner Arch Linux.

## Fase 2: Shell, ciclo de vida, camadas e geometria (concluída com a Fase 2R)
- [x] Distinção estrita entre estado `.md`, `is_open` no disco, WebViews instanciados e superfícies visíveis.
- [x] Ciclo de vida do daemon lento: `--background` começa com 0 WebViews criados (inativo ~0% CPU).
- [x] Modos Wayland Layer Shell: Desktop (`bottom`), Sobreposição (`overlay`) e Oculto.
- [x] Despachante CLI dinâmico de instância única (`new`, `toggle`, `show`, `hide`, `quit`).
- [x] Alça de arrastar janela (cabeçalho `.drag-region`) e alça de redimensionamento discreta (`.resize-handle`).
- [x] Persistência da geometria da janela em `$XDG_STATE_HOME/note-it/state.json` (persistiu apenas no final de arrastar/redimensionar).
- [x] Fixação segura de geometria, posicionamento em cascata e reserva de conector para vários monitores.
- [x] Política de link automático canônico (`https`, `http`, `mailto`) com escape seguro e não destrutivo.
- [x] Protocolo de liberação transacional antes de `hide` e `quit` para evitar perda de dados devido a edições rejeitadas.
- [x] Teste e validação ponta a ponta no compositor Niri.

## Fase 3: Editor, UX e recursos inspirados no Antinote (em andamento)

### Fase 3.0R.1: Editor e Estabilização Geométrica (Concluído)
- [x] Teclado físico pt-BR, teclas mortas e composição IME preservadas dentro do WebView.
- [x] Atalhos de formatação Markdown incluindo `Ctrl+R` tachado.
- [x] Arrastar e redimensionar com precisão de subpixel com o delta final `pointerup` aplicado.
- [x] A geometria da janela persistiu no final do gesto e foi restaurada na reabertura.

### Fase 3.1: Chrome da nota, menu de configurações, recolhimento e informações (concluída)
- [x] Popover de configurações do cabeçalho `☰` substituindo o ponto colorido direto.
- [x] Paleta de cores do papel movida dentro do menu, com persistência preservada.
- [x] Recolher/expandir reduzindo a nota à sua barra de cabeçalho, com a geometria expandida restaurada.
- [x] O estado recolhido persistiu durante as reinicializações, com migração de estado compatível com versões anteriores.
- [x] Observe as datas de criação e modificação ao passar o cabeçalho, formatado em pt-BR.
- [x] Ciclo de vida do gesto do ponteiro aprimorado: um ponteiro capturado por gesto, sem alteração de geometria
sem um gesto ativo.

### Fase 3.2: Tarefas, controles de visualização e formatação embutida (concluída)
- [x] Superfície do host apoiada na cor do papel da nota, para que um redimensionamento rápido não exponha mais uma cor escura
tira antes do WebView repintar.
- [x] Listas de tarefas Markdown com caixas de seleção quadradas, aninhamento e tachado automático.
- [x] Carimbos de data e hora de conclusão por tarefa que acompanham a tarefa e nunca são inventados.
- [x] O zoom de visualização (75–300%) persistiu por nota, independente do documento.
- [x] Tamanho do texto embutido, cor e destaque do texto, aplicados no menu de configurações.
- [x] `Ctrl+Shift+M` colapso, `Ctrl+Shift+Space` troca de camada, `Ctrl+Shift+>` / `Ctrl+Shift+<`
tamanho do texto, todos roteados através do único controlador de teclado.

### Fase 3.2R: Invocação, reabertura e tipografia (concluída)
- [x] `note-it` invoca a instância em execução de qualquer aplicativo em foco, criando uma camada de desktop
nota temporariamente sem perder a preferência armazenada.
- [x] Fechar a última nota não a deixa mais presa: a nota usada por último é reaberta na próxima invocação.
- [x] Digitar `->` produz um código real `→`, externo.

### Fase 3.3: Recolhimento de múltiplas notas e refinamentos de UX (concluída)
- [x] `note-it toggle-collapse-all` para cada nota, com `Ctrl+Shift+M` ainda por nota.
- [x] Uma nota recolhida se expande quando clicada e `☰` se expande e abre o menu com um clique.
- [x] O menu de configurações não fica mais recortado em uma nota recolhida.
- [x] `->` produz o `➜` mais pesado, legível em qualquer tamanho de texto.
- [x] O texto destacado pode ser lido em todas as cores de papel, incluindo o escuro.

### Fase 3.4: Papel e temas (concluída)
- [x] Cinco tipos de papel por nota — Liso, Pautado, Pontilhado, Quadriculado pequeno e Quadriculado grande — implementados como um sistema CSS parametrizado, não como cinco implementações distintas.
- [x] Intensidade do padrão por nota (Suave/Normal/Forte), afetando apenas a opacidade do padrão.
- [x] Tinta padrão escolhida a partir da cor do papel, para que permaneça visível em todas as sete cores, inclusive a escura, sem competir com o texto.
- [x] Tipo e intensidade do papel persistidos no front matter da nota, sem alterar seu conteúdo nem sua data de modificação; notas anteriores a esses campos abrem como papel liso.
- [x] O espaçamento do padrão é fixado em pixels, para que o zoom da visualização dimensione o texto e o deixe como está.
- [x] Tema da interface (Sistema/Claro/Escuro) armazenado uma vez em `config.toml` e transmitido a cada nota aberta, aplicado ao chrome sem alterar a cor própria da nota.
- [x] Conjunto de tokens `--ui-*` que separa o chrome do aplicativo do papel da nota, mantendo o menu legível tanto sobre uma nota preta quanto sobre uma amarela, em qualquer tema.

### Fase 3.4R: `updated_at` Integridade (concluída)
- [x] `updated_at` muda somente quando o conteúdo persistido da nota realmente muda. Abrir, fechar, invocar, ocultar, mostrar e sair sem editar deixam o campo intacto.
- [x] A comparação reside no único caminho pelo qual passa todo conteúdo salvo — salvamento automático, flush antes de ocultar ou sair e salvar e fechar —, não em cada chamador.
- [x] Uma nota cujo conteúdo permanece inalterado não é reescrita: nenhum arquivo temporário, nenhuma renomeação, nenhum fsync.
- [x] Fechar e liberar ainda relatam sucesso em um salvamento idêntico, para que o ciclo de vida nunca pare.
- [x] A recência, que decide qual nota uma invocação traz, agora acompanha a última edição, não o último fechamento. Consulte a observação na Fase 4 abaixo.

### Fase 3.4R.1: Integridade Transacional de Persistência (Concluída)
- [x] Uma alteração de conteúdo ou aparência é preparada em uma cópia e adotada na memória somente depois que `save_note_atomic` confirma a gravação; assim, o documento sempre descreve a nota em disco.
- [x] Uma falha de salvamento deixa intactas tanto a nota armazenada quanto a nota em memória; se o mesmo conteúdo chegar novamente, ele será realmente gravado, sem cair indevidamente no atalho de conteúdo idêntico.
- [x] Salvar e fechar nunca finaliza um fechamento após uma falha no salvamento e fecha normalmente quando o
nova tentativa foi bem-sucedida.
- [x] As liberações antes de ocultar e sair relatam uma falha na gravação como uma falha, e não como um sucesso.
- [x] Os salvamentos de aparência — cor, tipo e intensidade do papel e tamanho da fonte — seguem o mesmo caminho; uma falha não é mascarada pelo salvamento independente de conteúdo que acompanha o fechamento.
- [x] Uma falha ao salvar remove seu próprio arquivo temporário em vez de deixar detritos `.tmp.*` para trás.
- [x] Tudo o que a Fase 3.4R estabeleceu permanece inalterado: conteúdo persistente idêntico não grava nada,
`updated_at` muda apenas em uma edição real, `created_at` nunca muda e uma nota intocada preserva o horário de modificação do arquivo.

### Fase 3.4R.2: Ponto de Commit (Concluído)
- [x] A renomeação é o ponto de confirmação: um salvamento relata falha em qualquer coisa antes ou durante ele, e
sucesso a partir daí.
- [x] Uma sincronização de diretório que falha após a renomeação gera um aviso de durabilidade, não uma falha de salvamento; assim, memória e arquivo nunca terminam descrevendo versões diferentes da nota.
- [x] Nada rastreia uma sincronização perdida: uma sincronização de diretório libera todas as entradas pendentes, então a próxima
o salvamento bem-sucedido também torna a renomeação anterior durável.
- [x] O que não é garantido fica explícito: a sincronização não é repetida, e um salvamento cuja sincronização falhou não tem durabilidade garantida.
- [x] Tudo o que as Fases 3.4R e 3.4R.1 estabeleceram permanece inalterado.

### Fase 3.5: Blocos Inteligentes (Concluído)
- [x] Blocos de código cuja linguagem sobrevive à ida e volta pelo Markdown exatamente como escrita, inclusive uma cerca sem linguagem, caso em que nada é realçado.
- [x] Destaque de sintaxe para dezesseis gramáticas e seus aliases, apenas como decoração do editor, com
sem suposições e nada escrito no arquivo.
- [x] Chamadas na sintaxe de alerta de GitHub — NOTA, DICA, IMPORTANTE, AVISO, CUIDADO — contendo vários
parágrafos, listas e blocos aninhados, e degradando para uma citação simples quando o tipo não é
reconhecido.
- [x] Citações em bloco como estrutura própria, apresentadas corretamente e nunca promovidas a alertas.
- [x] Comentários armazenados como `<!-- ... -->`, editáveis ​​no editor e nunca fazem parte do texto da nota.
- [x] Todos os quatro acessíveis a partir do menu de notas existente, em uma seção **Blocos** em vez de uma
segunda barra de ferramentas.
- [x] Nenhuma arquitetura de bloco foi extraída. Os quatro não têm quase nada em comum — veja ADR-021.

### Fase 3.5R: Auditoria de Regressão e Estabilização (Concluída)
- [x] `Ctrl+Shift+Space` alterna a camada novamente. A ruptura foi o foco do host, não o atalho:
uma janela de shell de camada é mapeada sem widget de foco, então GDK recebeu chaves e as descartou
antes do WebKit, e uma mudança de camada limpava o foco novamente. O WebView agora recebe foco sempre que a janela está ativa. Isolar os três pontos de entrada revelou a causa: o menu e `note-it toggle` funcionavam, mas o teclado não.
- [x] Todos os atalhos dentro da nota se beneficiam: `Ctrl+N`, `Ctrl+W`, `Ctrl+R`, `Ctrl+=`/`-`/`0` e
`Ctrl+Shift+M` morreram pelo mesmo motivo sempre que a nota não foi clicada.
- [x] O atalho nunca digita um espaço na nota, é ignorado durante a composição pt-BR e
deixa AltGr — relatado como `Ctrl+Alt` — para o editor.
- [x] Uma nota é comparada e armazenada em uma grafia canônica, portanto, nem a nova linha de um arquivo é
terminado com nem a linha em branco que o serializador coloca depois que um bloco final é confundido com
uma edição. Tudo o que a Fase 3.4R estabeleceu ainda se mantém: uma edição real ainda se move `updated_at`.
- [x] Uma nota criada durante uma elevação por invocação abre na camada em que as outras notas estão, não na preferência armazenada.
- [x] `state.json` e `config.toml` são escritos sob a mesma regra de ponto de confirmação que uma nota, em um
gravação atômica compartilhada: a renomeação é confirmada, uma sincronização de diretório falha após ser uma durabilidade
aviso e uma configuração é totalmente substituída ou não é substituída.
- [x] Auditado sem encontrar um defeito: o coordenador do ciclo de vida e lote de liberação, o URL
lista de permissões e os sanitizadores Markdown/HTML, os blocos inteligentes e suas viagens de ida e volta, geometria
fixação e colapso, e as transições da camada invocar/ocultar/mostrar/reiniciar.

### Fase 3.5R.1: Refinamento de alternância de camada global
- [x] Niri possui o `Ctrl+Shift+Space` oficial; o atalho WebView é um substituto local.
- [x] A GAction `toggle-layer` direta atinge uma decisão de camada compartilhada sem lançar um
segundo processo GTK.
- [x] A promoção de desktop para overlay força um commit Wayland oportuno sem roubar o foco do aplicativo normal; a transição reversa permanece ao vivo e não remapeia a superfície.
- [x] A persistência da camada é debounce e lê o estado atual, enquanto os commits do ciclo de vida retêm o
garantias de durabilidade atômica existentes.
- [x] A repetição automática é suprimida para comandos de notas discretas e para a ligação Niri.

### Fase 3.6: Mecanismo Matemático (Concluído)
- [x] Cálculo contextual, avaliado conforme a nota é escrita: uma linha começando com `=`
mostra seu resultado ao lado e uma linha `nome := expressão` declara um valor nas linhas abaixo
ele pode usar.
- [x] Porcentagens nos formulários que as pessoas realmente escrevem — `10% de 200`, `200 + 10%`, `200 - 10%` —
com a leitura contextual vinculada a um `%` escrito na linha e não a um valor que
uma vez veio de um.
- [x] Variáveis ​​locais para a nota, resolvidas de cima para baixo, portanto existe uma variável a partir de sua declaração
para baixo e os ciclos são impossíveis sem um resolvedor para evitá-los.
- [x] Resultados reativos: toda a nota é reavaliada a cada alteração no documento, alterando assim uma
A declaração move todos os resultados abaixo dela sem nenhum rastreamento de dependência para ficar obsoleto.
- [x] `sum`, `avg` e `count` sobre o bloco de linhas de cálculo consecutivas diretamente acima deles.
- [x] Os resultados são decorações ProseMirror e nunca conteúdo: nada é escrito em `.md`,
`updated_at` não se move para um recálculo e a reabertura de uma nota o recalcula.
- [x] Um analisador sem avaliador por trás dele — sem `eval`, sem `Function`, sem acesso à propriedade, sem chamada
sintaxe — e nenhuma nova dependência de qualquer tipo.

### Fase 3.7: Conversões (concluídas)
- [x] Conversões de unidades escritas como `= 10 km em m`, avaliadas conforme a nota é escrita e mostradas como um
decoração ao lado da linha, exatamente como é um cálculo.
- [x] Oito dimensões, todas determinísticas e off-line: comprimento, massa, volume, temperatura, tempo,
área, dados digitais e velocidade. Cada grafia está listada em `docs/features.md`.
- [x] O lado esquerdo é uma expressão completa do mecanismo matemático, então parênteses, aritmética e variáveis
todos alimentam uma conversão.
- [x] Temperatura como escalas com zeros diferentes em vez de um fator, e área como sua própria unidade
em vez de um comprimento com um expoente.
- [x] Prefixos SI e IEC mantidos separados: `1 GB` tem 1.000 MB e `1 GiB` tem 1.024 MiB.
- [x] Unidades desconhecidas, dimensões incompatíveis e conversões impossíveis, cada uma relatada em seu próprio idioma
palavras, discretamente, sem nada escrito no arquivo.
- [x] Nada de novo no formato do arquivo, nada de novo no mecanismo visual e nenhuma nova dependência:
a tabela de unidades são dados e o resultado é a decoração que o mecanismo matemático já desenha.
- [x] Moedas deliberadamente **não** implementadas e nenhuma taxa codificada. A fronteira um futuro
a fonte deve ficar para trás está anotada em `ui/src/units/convert.ts` e ADR-025.

### Fase 3.7R: Isolamento do Harness de Testes (Concluída)
- [x] `scripts/note-it-isolated` isola o **barramento de sessão** e também o XDG. Note-it é uma `GApplication` de instância única; portanto, quando já havia um daemon no barramento real, um comando supostamente “isolado” era encaminhado a ele e o store real recebia a gravação. Foi assim que uma nota de teste chegou ao diretório de notas do próprio usuário durante os testes físicos da Fase 3.7.
- [x] Um `dbus-daemon` privado por execução de teste, com `DBUS_SESSION_BUS_ADDRESS` apontado para ele e o
variáveis de inicialização do D-Bus removidas. O daemon real nunca é interrompido e nem percebe a execução.
- [x] Falha segura em toda parte: o barramento é iniciado, comprovadamente distinto do real e acessível
alcançável *antes* de Note-it ser iniciado e o ambiente do processo iniciado ser lido novamente
de `/proc` e verificado. Os códigos de saída 90–93 indicam qual garantia não pôde ser cumprida.
- [x] `--root DIR` mantém o barramento privado ativo durante as invocações, portanto, um daemon iniciado por um
comando e um `new` enviado pelo próximo atingem a mesma instância; `--stop` termina e `--verify`
afirma que a instância realmente está no barramento privado.
- [x] `scripts/test-isolation` reproduz o incidente — uma sessão de ambiente com barramento e store próprios e, quando há display, um daemon real que possui o nome conhecido — e confirma que a nota chega apenas ao store descartável, enquanto o store do ambiente permanece inalterado até os nanossegundos.
Ele é executado em `cargo test`.
- [x] Nenhum código do aplicativo foi alterado. O defeito estava no harness e não em Note-it.

### Fase 3.8: Pesquisa e Produtividade (Concluída)
- [x] Pesquisa global em todas as notas, aberta com `Ctrl+K` em qualquer nota. Não diferencia maiúsculas de minúsculas e
insensível ao sotaque, então `biopsia` encontra `Biópsia` — a propriedade que o português mais precisa.
- [x] Nenhum índice persistente. Mil notas são listadas, lidas, dobradas, combinadas e transformadas em
trechos em dezenas de milissegundos, o que é mais rápido do que qualquer coisa que uma pessoa possa perceber e mais barato do que
um índice que teria que ser invalidado, reconstruído e mantido honesto. A medição é uma
teste, para que a reclamação continue sendo verificada — consulte ADR-027.
- [x] A pesquisa reside em `src/search.rs` e `StorageManager`, não na janela ou no WebView:
não precisa de GTK, WebKit e display, que é o que um futuro CLI também precisará.
- [x] Uma consulta vazia lista as notas escritas mais recentemente, portanto, o mesmo controle também é a maneira de
mover-se entre eles.
- [x] Um resultado é uma nota, endereçada por `note_id` — nunca pelo caminho, e nunca pelo rótulo, que
duas notas podem compartilhar. Abrir um ativa-o, abre-o se estiver fechado, expande-o se estiver
foi recolhido e rola para a partida, tudo sem tocar em `updated_at`.
- [x] Encontre dentro de uma nota com `Ctrl+F`, substitua por `Ctrl+H`. Enter e Shift+Enter percorrem
ocorrências e envoltório em ambas as extremidades; `Replace All` é uma única transação ProseMirror, então uma
`Ctrl+Z` coloca tudo de volta.
- [x] Nem a pesquisa nem a localização podem encontrar o que não está no arquivo: um cálculo `4` e um
os `10000 m` da conversão são decorações e procurá-los não encontra nada.
- [x] Colar URL na seleção: colar um URL sobre o texto selecionado transforma esse texto em link, conforme a lista de permissões que o aplicativo já possuía, sem rede nem busca de metadados e como uma única etapa de desfazer.
- [x] Renderização compacta de links avaliada e deliberadamente adiada: encurtar um URL oculta seu destino, o que seria uma regressão de segurança vendida como organização. A decisão foi registrada no ADR-027, não omitida silenciosamente.

### Fase 3.8R: Refinamento da Pesquisa (Concluído)

Quatro coisas que a Fase 3.8 disse que não foram exatamente o que fez. Nenhum recurso novo, nenhuma pesquisa difusa, nenhum índice, nenhum thread — a menor alteração correta para cada um e um teste para cada um. Consulte ADR-027.1.

- [x] "Cada nota" agora significa cada nota. A varredura parou em 5.000, então a nota 5.001 foi
inencontrável e nada teria dito isso. A varredura lê todo o store; o **resultado**
lista ainda está limitada a 100, porque cem linhas é o que uma pessoa lê e o leitor pode
veja que são cem. Um teste coloca uma nota na posição 5 001 e a encontra.
- [x] A listagem de consulta vazia mantém seu limite: mostra no máximo cem notas, portanto, lendo além delas
não responderia a nenhuma pergunta.
- [x] A paleta de pesquisa elimina qualquer resposta a uma pergunta que não está mais sendo feita. Numeração
peguei uma resposta lenta chegando *depois* de uma rápida e perdi a ordem oposta — `bio`
atendendo enquanto `biopsia` ainda está em vôo. Somente a resposta da solicitação pendente poderá
mude a lista.
- [x] Os limites são descritos como são: limites para a pergunta e para a resposta, não para
a nota. A pesquisa lê uma nota até o final, porque uma palavra no final deve ser localizável. O
o custo de uma nota grande é medido — uma nota de 2 MB é pesquisada corretamente, com seus acentos
intacto e sem escrita - em vez de reivindicado como limitado. Nenhuma maquinaria assíncrona foi
introduzido para tornar uma frase verdadeira; a frase foi corrigida.
- [x] "Mais recente" é o `updated_at` da própria nota, não o `mtime` do arquivo. Aparência – cor,
papel, intensidade do padrão, tamanho da fonte — reescreve o arquivo sem ser uma edição, portanto, ordenar por
`mtime` fez com que a repintura de uma nota contasse como escrita nela. Uma nota sem leitura `updated_at`
volta para `mtime`, os empates são quebrados pelo identificador e a listagem ainda não grava nada.
- [x] Sem regressão na Pesquisa, Troca Rápida, Localizar, Substituir, Colar URL na Seleção, o compartilhado
camada ou ciclo de vida; `updated_at` e o histórico de desfazer permanecem intactos.

### Fase 3.9: Confiabilidade (concluída)

Nenhuma nova superfície de produtividade. Apenas uma pergunta: alguma ação que Note-it oferece pode transformar um erro recuperável em texto perdido? Consulte ADR-028 e ADR-029.

- [x] **Lixeira recuperável.** *Dados › Mover esta nota para a lixeira* move `notes/<uuid>.md` para
`trash/<uuid>.md`, com uma confirmação informando que a exclusão pode ser desfeita. `×` e `Ctrl+W`
ainda significam fechar, como sempre significaram.
- [x] A ordem é flush → movimento → estado → superfície, e o movimento é o ponto de confirmação. Uma nota cujo
o último texto que não pôde ser escrito nunca é movido e nunca desaparece; além do movimento da nota
*está* na lixeira, e nem a redação do estado nem a desmontagem da janela podem informar o contrário.
- [x] Uma nota na lixeira não é uma nota: não aparece na pesquisa nem no alternador rápido, não é convocada nem reaberta na inicialização, pois todos esses recursos leem `notes/` e o arquivo já não está lá.
- [x] A restauração coloca o arquivo de volta com o mesmo identificador e os mesmos bytes. `hard_link` recusa
um nome existente, portanto, uma nota ativa contendo esse identificador nunca será substituída - uma propriedade de
o syscall, não de uma verificação que possa ser executada.
- [x] Nem excluir nem restaurar é uma edição: `updated_at` não se move, portanto, uma nota recuperada
retorna ao seu lugar no switcher rápido em vez de pular para o topo. Sua geometria vem
de volta também.
- [x] A data de exclusão fica em um arquivo secundário `<uuid>.json`, nunca em Markdown, portanto, uma nota cuja
front matter está danificado ainda vai para a lixeira e ainda volta byte por byte.
- [x] **Backup automático local.** `backups/<timestamp>/` contém `notes/`, `trash/`, `config.toml`,
`state.json` e um manifesto — diretórios comuns de arquivos comuns, recuperáveis ​​com `cp`.
- [x] No máximo um instantâneo automático a cada 24 horas, criado **antes** da primeira alteração qualificada após esse intervalo, não depois dela, para que valha a pena voltar ao estado capturado. Sem temporizador, thread ou polling: um daemon inativo não trabalha.
- [x] *Dados › Fazer backup agora* para um instantâneo sob demanda, relatado em uma linha no final do
nota em vez de um diálogo sobre ela.
- [x] Construído em `.tmp.…` e renomeado: um instantâneo é válido ou não existe. Resíduos deixados por uma falha são removidos pelo próximo backup, e somente diretórios com esse prefixo são removidos.
- [x] Sete instantâneos mantidos, removidos **somente depois** que um novo instantâneo é confirmado. Um backup que falha nunca
custa a proteção já existente no disco e nunca bloqueia o salvamento de uma nota.
- [x] Os instantâneos nunca contêm instantâneos, arquivos temporários ou qualquer coisa alcançada por meio de um link simbólico.
- [x] A recuperação é provada e não prometida: um instantâneo é copiado para uma segunda árvore XDG vazia
e aberto, e as notas, identificadores, Markdown, lixo, configuração e estado da janela, todos
voltar. O procedimento manual está em `docs/storage.md`.
- [x] Auditoria de confiabilidade em quinze casos de falha – uma nota que desapareceu, uma que não pode ser lida,
uma entrada de lixo removida externamente, uma restauração em um identificador ativo, um diretório de backups que
não pode ser criado, um store que não pode ser lido, um commit que não pode ser acessado, um arranhão deixado por um
falha, estado obsoleto, estado ausente, front matter danificado, configuração ausente e liberação
que falha com várias notas abertas.
- [x] Terminologia: o que a Fase 3.8 chama de "AutoPaste" é **Colar URL na seleção**
(`ui/src/editor/linkPaste.ts`). Comportamento inalterado; o nome está liberado para a área de transferência real
AutoPaste na Fase 3.11.

**Deliberadamente não nesta fase:** exclusão permanente, esvaziamento da lixeira e restauração de um store inteiro com um clique. Os dois primeiros são controles irreversíveis na fase cujo tema é a reversibilidade; a terceira é uma transação de vários arquivos que merece seu próprio design, em vez de uma entrada de menu.

### Fase 3.10: Timer e Pomodoro (concluída)

- [x] Uma contagem regressiva na nota, alcançada a partir de um ⏱ na barra de cabeçalho e mostrada em um pequeno painel abaixo
isto. Nenhuma segunda janela, nenhuma faixa permanente retirada da nota.
- [x] Predefinições em 5, 10, 15, 25, 30, 45 e 60 minutos e um campo para qualquer outra coisa de 1 a 600
minutos inteiros. Uma duração fora disso — zero, negativa, fracionária, `NaN`, absurda — é
recusou e disse isso, nunca arredondado para o alcance.
- [x] Pomodoro 25/5/15: quatro sessões de foco em um ciclo, a quarta seguida pelo intervalo longo, depois
a contagem começa novamente. A fase é um modelo explícito, não um comportamento espalhado pelos manipuladores.
- [x] Inicie, pause, continue, cancele e reinicie, exibindo apenas os controles aplicáveis. Pular
passa para a próxima etapa do Pomodoro sem esperar por esta.
- [x] Nada começa sozinho. Uma fase concluída é marcada como concluída e **oferece** a próxima;
o leitor começa.
- [x] **A verdade é um instante, não um contador.** Um cronômetro em execução é armazenado como o momento do relógio de parede
termina e cada leitura é `deadline - now`. Nada diminui, então nada flutua e
nada é perdido para um WebView estrangulado, uma máquina ocupada ou um laptop suspenso.
- [x] Pausar descarta o instante e congela o restante, então o tempo pausado não pode ser gasto –
através de uma ocultação, através de uma reinicialização, através de qualquer número de ciclos de pausa/retomada.
- [x] A execução sobrevive à nota ser recolhida, ocultada ou ao aplicativo ser fechado: ela vem
volta com o tempo que realmente passou, e aquele cujo fim já passou volta
**concluído** em vez de contar até zero.
- [x] Uma nota recolhida mantém o relógio na barra ao lado do nome da nota; uma nota muito estreita para ambos
abre mão dos dígitos e nunca do nome ou do controle próximo.
- [x] A conclusão acontece exatamente uma vez, protegida pela própria transição de estado e não por uma bandeira:
uma linha no final da nota e uma notificação na área de trabalho, independentemente do tempo que a nota permanecer
      zero.
- [x] As notificações não trazem nada da nota – nenhum título, nenhum texto. A página informa *qual* tipo
da execução terminou, de um conjunto fechado, e o host possui as palavras.
- [x] **Não conteúdo.** O cronômetro nunca é gravado no Markdown de nenhuma forma. Começando, pausando,
finalizando e cancelando deixe o arquivo de notas byte por byte como estava e deixe `updated_at`
onde estava; pesquisa, o título recolhido e a lixeira nunca o veem. Ele mora ao lado
geometria da janela em `state.json`, escrita apenas em uma mudança semântica e nunca em um tick.
- [x] Uma contagem regressiva por nota, codificada pelo identificador da nota, para que duas notas não possam misturar seus temporizadores
e não há gerenciador de cronômetro global.

**Deliberadamente não nesta fase:** o cronômetro e os cronômetros nomeados esta entrada uma vez listada. Um cronômetro conta *acima* e não tem prazo, que é um segundo modelo temporal em vez de um segundo botão neste; nomear um cronômetro é um rótulo sem lugar para ser lido - a nota já é o nome. Ambos pertencem a tudo o que os pede com um motivo, não à fase cujo assunto é uma contagem regressiva confiável.

### Fase 3.11: AutoPaste da área de transferência (concluído)

O verdadeiro, no sentido em que o Antinote usa a palavra: um modo de captura, não a colagem de URL sobre seleção Fase 3.8 enviada com esse nome. Esse ainda está lá, ainda chamado Colar URL na seleção, e intocado por isso.

- [x] Um modo de captura explícito, desativado por padrão, ativado em *☰ › Captura* com uma linha dizendo
exatamente o que fará.
- [x] **Desativado significa desativado, como uma propriedade e não como uma promessa.** Enquanto o AutoPaste estiver desativado, não há
manipulador conectado à área de transferência, então nada é observado, lido, hash, armazenado ou
enviado. Medido em uma sessão real Niri: três cópias com o modo desativado produziram zero área de transferência
eventos de qualquer tipo.
- [x] Orientado por evento através do próprio sinal `changed` de GDK. Sem votação, sem intervalo, não
`navigator.clipboard` e nenhuma nova dependência: o kit de ferramentas já em processo responde a isso.
- [x] **O modo é apenas de sessão e nunca é anotado.** Nem no Markdown, nem no
`state.json`, não em `config.toml`. Uma reinicialização, uma falha ou uma atualização o deixa desativado e o
leitor decide novamente.
- [x] Um alvo para todo o aplicativo, porque a área de transferência do sistema é uma coisa. Armando um
a segunda nota libera a primeira na mesma etapa, e a barra e o menu da nota liberada indicam isso.
- [x] Somente texto. Uma imagem, uma lista de arquivos ou um formato desconhecido foi recusado dos formatos oferecidos
sem que um byte dele seja transferido.
- [x] A área de transferência como era *antes* do switch nunca é capturada: conectar o manipulador lê
nada, então apenas uma mudança após esse momento é uma captura.
- [x] As capturas são anexadas ao **final** da nota, como uma transação, sem foco, sem
seleção movida, sem rolagem e sem janela levantada — o leitor está em outro aplicativo, que
é o ponto principal.
- [x] Uma captura é uma `Ctrl+Z`.
- [x] Três delimitadores — Linha, Linha em branco (padrão) e Separador — aplicados entre os
conteúdo existente e a captura, exatamente uma de cada vez, e nunca na frente da primeira
capturar em uma nota vazia. Alterar a preferência nunca reescreve o que já existe.
- [x] **Proteção de loop do kit de ferramentas, não de suposições.** Uma cópia ou corte dentro de Note-it torna o
usando o proprietário da área de transferência e GDK diz isso; essa mudança é recusada antes de qualquer leitura
começa. A comparação de texto foi deliberadamente rejeitada: duas cópias deliberadas das mesmas palavras são
duas capturas.
- [x] Uma geração em cada execução armada, verificada novamente quando cada leitura assíncrona retorna, portanto, uma leitura
ainda no ar quando o modo é desligado, o alvo muda, a nota fecha ou o
o aplicativo oculta não oferece nada.
- [x] Uma leitura de cada vez, então A, B, C chegam como A, B, C.
- [x] Desarmado **antes** do flush ao fechar, ocultar, sair e descartar, para que nenhum callback obsoleto possa atingir um
documento que está prestes a ser escrito e destruído.
- [x] Uma captura é uma edição real: o Markdown muda, o `updated_at` se move, o salvamento automático comum
escreve e a pesquisa o encontra. Ativar ou desativar o modo e alterar a alteração do delimitador
nada disso.
- [x] Nada sobre o modo é conteúdo. Nenhum marcador no Markdown, nada no título, o
snippet, o rótulo da lixeira ou o índice de pesquisa.
- [x] Nenhum conteúdo da área de transferência em nenhum log, em qualquer nível.
- [x] Note-it nunca se apropria da área de transferência: após uma captura, o texto copiado ainda é colado
normalmente em qualquer outro aplicativo.

### Fase 3.12: Imagens e layout rico (concluída)

Reordenado deliberadamente: o que faltava na nota era uma imagem nela, não uma saída dela. Captura e exportação recuam e Flashcards - que precisa de imagens para valer a pena construir - avançam em seguida.

- [x] Imagens locais em uma nota: coladas, descartadas ou escolhidas em *☰ › Mídia › Inserir imagem…*.
- [x] **Nunca base64 no Markdown.** Os bytes vão para `assets/<note-uuid>/<asset-uuid>.<ext>`
ao lado de `notes/` e `trash/`, e a nota refere-se a eles por um caminho relativo a `notes/`.
- [x] Esse caminho relativo é o motivo pelo qual uma nota chega à lixeira e volta sem que um byte seja
reescrito: `notes/` e `trash/` são irmãos, então `../assets/…` resolve o mesmo de qualquer um deles.
Nenhum caminho absoluto da máquina do leitor é escrito em uma nota.
- [x] A página nunca indica um caminho do sistema de arquivos. Ele carrega `note-it-asset:/<note>/<asset>.<ext>`, que
o host serve depois de analisar ambas as metades como `Uuid`s - então um `..`, um caminho absoluto ou um
o separador codificado não resolve um arquivo, ele não analisa. Consulte ADR-032.
- [x] PNG, JPEG, WebP e GIF, decididos pelos bytes e nunca pelo nome do arquivo. **SVG foi recusado**:
é um documento que pode conter escrita, e uma nota não é um lugar que precise dela.
- [x] Simples `![](…)` enquanto não há mais nada a dizer, e um canônico `<img>` carregando exatamente
`src`, `alt`, `data-note-it-width` e `data-note-it-align` quando uma largura ou alinhamento for
escolhido. O sanitizador reescreve a tag nesse formato ou a descarta completamente.
- [x] Redimensione arrastando uma alça, as proporções são mantidas porque apenas a largura é armazenada. Uma tragada
é uma entrada na história, não quinhentas.
- [x] Esquerda, centro e direita, com o texto contornando uma imagem alinhada à esquerda ou à direita.
- [x] Cada alteração em uma imagem é uma edição comum por meio do salvamento automático comum: o Markdown
muda, `updated_at` se move, a pesquisa encontra as palavras ao seu redor. Selecionando um, abrindo seu
controles, cancelar o seletor ou escolher o alinhamento que ele já possui não altera nada.
- [x] Uma imagem não é texto. Nada sobre como um é armazenado chega ao título recolhido, uma pesquisa
snippet, o rótulo da lixeira ou `visibleText` — pesquisando o identificador de um recurso, uma largura ou um
o alinhamento não encontra nada, e uma nota contendo uma imagem e nenhuma palavra ainda não tem nome.
- [x] Nada é buscado. Uma imagem remota percorre o texto que é e é desenhada sem fonte
de jeito nenhum, então a exibição de uma nota chega à rede de graça.
- [x] Nenhuma dependência foi adicionada.

**Deliberadamente não nesta fase:** corte, rotação, filtros, legendas, galerias, lightboxes, imagens por URL e **coleta automática de ativos órfãos** — remover uma imagem tira-a da nota e sai do arquivo, porque excluir bytes em uma estimativa é pior do que mantê-los.

#### 3.12R: O backup passa a incluir imagens

Enviado em 3.12 e detectado pela auditoria que se seguiu: as imagens foram para `assets/` e o instantâneo ainda copiou apenas `notes/`, `trash/`, `config.toml` e `state.json`. Um backup feito entre restaura o Markdown de uma nota e não o arquivo para o qual seu `![](../assets/…)` aponta, o que não é o que um backup promete.

- [x] `assets/` faz parte de um instantâneo, na mesma forma e byte por byte, para uso automático e
backups manuais – uma rotina serve ambos.
- [x] Copiado estritamente e fechado com falha: dois níveis conhecidos, nunca uma descida recursiva geral, nunca um
link simbólico seguido, e qualquer coisa que não seja `<note-uuid>/<asset-uuid>.<ext>` interrompe o
instantâneo em vez de ser omitido silenciosamente de um relatado como completo. Arranhão deixado por um
a importação interrompida é ignorada, como acontece com as notas.
- [x] Uma imagem para a qual nenhuma nota aponta mais também é copiada. Um backup não é uma coleta de lixo.
- [x] Uma falha na cópia de uma imagem falha em todo o snapshot antes do ponto de confirmação: nada é
renomeado e a retenção não é executada — um backup antigo nunca é excluído para liberar espaço
por um que não aconteceu.
- [x] A versão 2 do manifesto registra a contagem de imagens. Os snapshots da versão 1 permanecem listáveis ​​e legíveis.
- [x] Comprovado pela restauração em um segundo store vazio com o original excluído: ambas as notas vêm
de volta, ambas as imagens são renderizadas por meio de `note-it-asset:` e cada arquivo é idêntico em bytes.

#### 3.12R.1: O clipper de imagens

Um refinamento da mesma fase, não uma fase própria: a entrada teve três cliques de profundidade para aquilo que as pessoas mais fazem.

- [x] Um clipe de papel no cabeçalho, entre **Buscar** e o cronômetro, abrindo o seletor de arquivos no
primeiro clique – nenhum painel intermediário.
- [x] Uma função, dois gatilhos. O botão e *☰ › Mídia › Inserir imagem…* chamam o mesmo manipulador
e envie o mesmo `insert_image_requested`; nenhum segundo seletor, importador, caminho de ativos ou
serializer existe para se afastar do primeiro.
- [x] A entrada do menu permanece, assim como colar e soltar.
- [x] Oculto em uma nota recolhida, como as seis ações rápidas, e oculto em uma nota expandida mais estreita
mais de 300 px: o orçamento do bar em `MIN_NOTE_WIDTH` tem que ceder para algum lugar, e o clipe de papel é
o único controle cujo trabalho o menu ainda executa por completo.
- [x] O desenho é SVG embutido na coleção de ícones, escrito na página no momento da construção - o
pipeline que sobrevive ao `default-src 'self'` da página.
- [x] Nenhum novo atalho de teclado, nenhuma nova mensagem de ponte e nada alterado em `assets`, `backup`,
`storage`, `search`, `timer` ou `autopaste`.

### Fase 3.13: Flashcards Core (Concluído)

- [x] `Pergunta :: Resposta` produz um cartão de origem e um item de revisão; `Termo ::: Definição`
produz uma origem e duas direções adjacentes, na ordem do documento e sem desduplicação.
- [x] A sintaxe embutida requer espaços em branco, corresponde a `:::` como um todo antes de `::` e recusa o código,
URLs, horários, namespaces, atributos técnicos, `::::` e múltiplos delimitadores ambíguos.
- [x] Um parágrafo marcador de nível superior ocupa exatamente o bloco estrutural antes e depois dele. Títulos,
quebras duras, listas com marcadores e numeradas, tarefas, citações, textos explicativos, imagens e imagem mais texto são
preservado como o lado que a nota já contém.
- [x] A extração lê a árvore ProseMirror. Markdown continua sendo a fonte da verdade; não há
arquivo flashcard, banco de dados, identificador persistente, metadados ou protocolo de back-end.
- [x] As imagens gerenciadas mantêm o nó da Fase 3.12 e a rota `note-it-asset:`. O estudo não cria nenhuma cópia,
miniatura ou segundo ativo e não desenha controles de redimensionamento ou alinhamento.
- [x] Uma decoração ProseMirror somente leitura mantém o delimitador visível e marca os cartões reconhecidos
sem uma transação, salvamento, alteração de carimbo de data/hora ou desfazer entrada. Contagens de fontes/revisões atualizadas com
o documento ativo.
- [x] *☰ › Estudo* dá explicação zero cartão e abre painel interno de Estudo somente quando há
é algo para estudar - nenhum botão permanente da barra de ferramentas e nenhuma segunda janela GTK.
- [x] O estudo tem progresso, frente, revelação, resposta, anterior, próximo, embaralhamento e fechamento. Os fins não
enrolar; navegação e shuffle ocultam a resposta, e shuffle usa um Fisher-Yates RNG injetável
sobre itens de revisão.
- [x] A sessão é um instantâneo efêmero. A edição e o AutoPaste continuam abaixo, enquanto o
a lista atual permanece fixa até que o estudo seja fechado e reaberto.
- [x] O teclado e o foco permanecem no painel: `Escape`, `Space`/`Enter`, setas, botões nomeados,
nenhuma ação dupla em um botão em foco e o foco retornado ao invocador ao fechar.
- [x] O painel exclui popovers de menu, pesquisa, localização, lixeira e temporizador, fecha ao ser recolhido, ajusta-se ao
Faixa de notas de 220 a 900 px e rola um cartão longo internamente. Fechar o popover do temporizador não
pare sua contagem regressiva.
- [x] O estudo não possui editor e renderiza fragmentos ProseMirror seguros com o `DOMSerializer` da nota.
Abrir, revelar, mover, embaralhar e fechar deixam Markdown e `updated_at` inalterados.

### Fase 3.14: Sistema de estudo e repetição espaçada (concluída)

- [x] Versionado, atômico `$XDG_DATA_HOME/note-it/study.json`, separado das notas e `state.json`,
com dados corrompidos/mais recentes preservados e estudo com falha no encerramento.
- [x] SHA-256 revisa a identidade da nota UUID, lados semânticos, direção e ordinal duplicado;
mover ou formatar/redimensionar/alinhar somente apresentação não redefine um cartão.
- [x] Ladder-v1 determinístico com Difícil/Médio/Fácil, intervalos inteiros exatos, relógio de propriedade do host,
commit-before-advance, instruções reversíveis independentes e uma classificação por item por sessão.
- [x] Catálogo sob demanda de todas as notas ao vivo, incluindo notas fechadas e excluindo lixo, analisadas em
o WebView pelo mesmo esquema e extrator Tiptap do editor visível.
- [x] Centro de estudo interno com revisão agora, tudo, nota atual, rótulos de fonte, lista compacta, vencimento/novo
status, contagens úteis compactas, mapa de calor em escala fixa de 365 dias e sequência atual/mais longa.
- [x] O FlashcardPanel existente evoluiu para classificações, visualizações de intervalo, tratamento persistente de ACK/erro,
nota de origem e resumo de conclusão; conteúdo rico e seguro e imagens gerenciadas permanecem um renderizador.
- [x] Painel da barra de ferramentas, atalho para lixo recuperável ao lado de X e Zoom -/+ por meio de ações existentes, com
medidas de fallback responsivo e proteção contra notas recolhidas.
- [x] O manifesto de backup v3 carrega `study.json` opcional; v1/v2 permanecem legíveis e incompletos
a cópia do estudo não pode ser confirmada como um instantâneo completo.

Captura e Exportação, OCR e PDF são adiados. Eles não fazem parte da Fase 3.14.

### Fase 3.14R.1: Polimento de interface e acessibilidade visual (Concluída)

- [x] O Study Hub distingue os cartões de origem das instruções de revisão, incluindo o explícito 2 cartões /
Corpus reversível de 3 revisões.
- [x] Ações de cabeçalho agrupadas por finalidade com separadores restritos, IDs/manipuladores estáveis ​​e um
pílula de pesquisa ampla/compacta centralizada que abre o SearchPalette existente.
- [x] Tokens de movimento curto compartilhados para botões e painéis internos, movimento seguro de recolhimento somente de conteúdo,
semântica imediata de estado oculto e fallback `prefers-reduced-motion` completo.
- [x] Zoom por nota estendido para 300% através do mesmo caminho frontend/host/estado.
- [x] Escala global de interface de 90–160% em `config.toml`, transmitida para todas as notas e refletida em real
chrome e geometria recolhida sem afetar o zoom do documento, o tamanho do texto ou Markdown.
- [x] Os metadados de atalho central mantêm dicas de ferramentas, dicas de menu e `aria-keyshortcuts` alinhados sem
inventando atalhos para ações que não possuem nenhum.
- [x] Orçamento responsivo verificado de 220 a 900 px em 100/120/140/160%, preservando Menu, ativo
Timer/AutoPaste e Close antes dos atalhos opcionais.

## Fase 4: Note-it programável

Evolução arquitetônica de um aplicativo para uma plataforma local programável. GUI, CLI e futuras interfaces de máquina compartilham um domínio e autoridade de persistência.

- [x] **Fase 4.0A — Limite do Core.** Crate headless dedicada `noteit-core`; a GUI consome seus
      recursos compartilhados de notas, pesquisa, lixeira, estudo e storage, com uma barreira
      de dependências do Cargo que impede GTK, GDK, WebKitGTK, layer-shell, Wayland e Niri de
      entrarem no Core.
- [x] **Fase 4.0B — Fundação de metadados: tags + propriedades.** Metadados estruturados do usuário
      no front matter Markdown de cada nota, validados e persistidos pelo Core; catálogos derivados
      das notas ativas, gravações transacionais sobre o documento atual do WebView, pílulas
      responsivas e um painel compacto de metadados. Sem sidecar, índice, banco de dados ou comando CLI.
- [x] **Fase 4.0D — API de leitura.** Interface CLI headless e somente leitura apoiada pelas
      autoridades de `noteit-core`; subcomandos `listar`/`list`, `ler`/`read`, `buscar`/`search`,
      `tags`, `propriedades`/`properties`, `tarefas`/`tasks`, `lixeira`/`trash`; filtragem de
      metadados (`--tag`, `--propriedade`/`--property`, `--limite`/`--limit`), análise de tarefas
      com filtro de estado (`--estado`/`--state`), resolução segura de seletores de nota,
      sanitização para segurança do terminal e rigorosamente nenhuma mutação do store.
- [x] **Fase 4.0D.1 — Contrato da API de leitura e proteção do terminal.** Apresentação padronizada
      no fuso horário local da máquina (`dd/MM/yyyy HH:mm`), correspondente aos contratos da GUI;
      sanitização universal de entradas não confiáveis para terminal; avisos tipados e desacoplados
      do Core (`ReadBatch<T>`, `ReadWarning`), sem instruções de impressão no Core; e análise fiel,
      em TypeScript, dos comentários de metadados das tarefas.
- [x] **Fase 4.0D.2 — Pureza do pipeline de leitura e integridade dos avisos.** Pipeline unificado de
      carregamento e avisos de pesquisa para buscas filtradas e não filtradas em todo o universo de
      notas elegíveis; eliminação da saída direta para stderr nos caminhos de leitura do store no
      Core; separação entre consulta de domínio e sanitização de apresentação; e validação rigorosa
      dos tokens de comentários de tarefas.
- [x] **Fase 4.0E — API de gravação + concorrência entre GUI e CLI.** Exatamente um gravador do
      Note-it por store, garantido por um lease `flock` consultivo em um diretório de runtime por
      store: a instância de desktop o adquire na inicialização e o mantém durante toda a sessão; a
      CLI o adquire por um comando quando está livre e, quando não está, envia a alteração por um
      soquete Unix local privado, sem jamais gravar contornando outro gravador. Operações tipadas de
      gravação do Core (`WriteOperation`, `NoteMutation`, `WriteOutcome`, `WriteError`) são
      compartilhadas pelos dois caminhos; comandos `criar`/`create`, `adicionar`/`append`,
      `editar`/`edit`, `tags adicionar|remover`, `propriedades definir|remover`,
      `tarefas concluir|reabrir`, `lixeira restaurar`, com `--stdin` e `--vazio`. Uma nota aberta na
      tela é alterada atrás de uma barreira de gravação externa que congela o editor *antes* de
      lê-lo, incorporando o texto não salvo ao mesmo commit em vez de sobrescrevê-lo; uma geração de
      runtime por janela permite recusar todas as mensagens ainda em trânsito da execução anterior.
      Tokens de snapshot `TaskRef` otimistas, sem sidecar e sem identidade persistida de tarefa. A
      API de leitura permanece somente leitura; as gravações de notas nunca tocam em `config.toml`
      ou `state.json`.
- [x] **Fase 4.0E.1 — Autoridade de gravação com falha fechada e adoção confirmada pela UI.** A
      invariante central da 4.0E tornou-se estrutural, não aspiracional: a instância de desktop mantém
      `WriteAuthority` por valor e se recusa a iniciar sem um lease *e* um soquete de controle, de
      modo que um Note-it em execução e editável que não seja proprietário de seu store não pode ser
      representado. A adoção de um documento commitado é confirmada pela página
      (`ExternalWriteApplied`, validado por nota, solicitação e geração), em vez de inferida da
      avaliação de um script, com uma espera limitada que rebaixa o resultado para
      `ui_sync_warning`, nunca para falha. O tempo limite do lado do cliente que poderia liberar o
      editor enquanto um commit ainda estava em trânsito foi removido; uma gravação lenta agora é
      informada como lenta e permanece retida.
- [x] **Fase 4.0E.2 — A falha na adoção pela UI permanece bloqueada.** Fechada a última lacuna
      pós-commit: uma página que não conseguiu adotar um documento já commitado não é mais liberada.
      Ela mantém a geração anterior *e* permanece congelada, conserva na fila as ações do documento
      sem executá-las, envia apenas a confirmação negativa e informa ao leitor que a janela está fora
      de sincronismo. A gravação permanece commitada e ainda relata `ui_sync_warning`. Reabrir a nota
      a restaura exatamente a partir do arquivo commitado, como verificado de ponta a ponta no
      ambiente isolado.
- [x] **Fase 4.0E.2R — Estado terminal não sincronizado.** O estado terminal tornou-se terminal de
      fato: a barreira mantém uma fase explícita, cada transição é protegida por ela e nenhum
      temporizador, callback tardio, aplicação repetida, anulação ou atualização de geração pode
      devolver a uma página que não adotou um documento commitado um estado editável ou aparentemente
      sincronizado. Também foi corrigido um bloqueio de transação que permanecia desativado quando a
      adoção lançava uma exceção durante a execução.
- [x] **Fase 4.0F — Interface de máquina / JSON.** Primeiro contrato público estável para consumidores
      de máquina: uma opção global `--json` que emite exatamente um documento JSON versionado por
      execução, na saída padrão em caso de sucesso e no erro padrão em caso de falha, mantendo o outro
      canal vazio e sem ANSI. A saída é renderizada a partir do mesmo resultado tipado que origina as
      frases em português: `noteit-cli` ganhou `outcome.rs` (o que aconteceu) e `machine.rs` (o esquema
      público), e `run_with_args` agora retorna um `CliResponse` que carrega os dois canais como dados,
      impedindo que um aviso escape por `eprint!` no meio de um comando. Nomes canônicos de comandos
      independentes da grafia, UUIDs completos, carimbos de data e hora RFC 3339 UTC, tipos JSON reais,
      Markdown bruto não alterado pelo sanitizador do terminal e códigos de erro estáveis. Os dois
      estados pós-commit que de outra forma seriam reduzidos a “falhou” são de primeira classe: uma
      gravação commitada cuja janela não confirmou é `status: warning`, com
      `commit_state: committed` e saída `0`; um resultado desconhecido é `status: indeterminate`, com
      `commit_state: unknown`, nunca `not_committed`, para que nenhum agente repita a operação e
      duplique um acréscimo. O modo máquina sobrevive a erros de análise. O protocolo de controle
      privado não é exportado. A saída humana, os códigos de saída e as regras de gravação permanecem
      inalterados; a ajuda ganhou uma linha que documenta a opção. Contrato em
      `docs/machine-interface.md`; justificativa no ADR-041.
- [x] **Fase 4.0G — Experiência humana e apresentação da CLI.** `noteit` sem argumentos deixou de ser
      uma lista de comandos e passou a ser uma apresentação: logotipo `NOTE-IT` em blocos, versão vinda
      da própria versão do pacote, uma linha dizendo o que o Note-it é e cinco comandos por onde
      começar. Amarelo para a marca, magenta para o acento, e nada mais — cor nenhuma carrega
      informação sozinha. A apresentação se adapta ao terminal em vez de quebrar nele: logotipo a
      partir de 54 colunas, `NOTE-IT` escrito entre 27 e 53, versão e dois comandos abaixo disso. A
      largura vem do próprio terminal (`TIOCGWINSZ`), com `COLUMNS` apenas como reserva e só quando o
      valor é plausível; sem terminal, 80 colunas por suposição conservadora. `NO_COLOR` — mesmo vazio
      — e `TERM=dumb` desligam a cor, e `TERM=dumb` também dispensa a arte em blocos. Cano e
      redirecionamento recebem texto puro e determinístico. Executar `noteit` não cria nota, janela,
      socket, lock ou store: imprime, sai com `0` e não toca em carimbo de tempo nenhum. O logotipo
      aparece só aí — `noteit ajuda`, os erros e o `--json` seguem sem ele. `OutputContext` passou a
      responder por canal, corrigindo um vazamento de ANSI para a saída de erro redirecionada quando a
      saída padrão era um terminal, e ganhou largura, de modo que toda a matriz é testável sem um
      terminal físico. A ajuda passou a documentar `--help`, `--version` e os aliases de `--estado`, e
      ganhou exemplos. Interface de máquina intocada: `--json` continua com exatamente um documento,
      nos mesmos canais, com os mesmos códigos, provado agora também sobre um terminal real.
      **Nenhuma TUI foi implementada** — ela foi movida para a Fase 5.0.
- [x] **Fase 4.0H — Ferramentas de desenvolvedor e automação.** O ciclo diagnosticar → verificar →
      construir passou a ter três entrypoints canônicos para uso local, e o CI passou a reutilizar
      `scripts/doctor` e os mesmos estágios de `scripts/check`, invocados um a um.
      `scripts/doctor` diagnostica o ambiente sem alterá-lo — presença e versão de `bash`, `git`,
      `cargo`, `rustc`, `pkg-config`, dos módulos `gtk4`, `gtk4-layer-shell-0` e `webkitgtk-6.0`, de
      `dbus-daemon`/`dbus-send`, de `node` e de `pnpm` —, lendo a versão mínima do Rust do
      `rust-version` do próprio `Cargo.toml` em vez de redeclará-la, e sem instalar, elevar
      privilégio ou tocar em PATH, dotfiles ou configuração. `scripts/check` virou a autoridade
      sobre os gates, com estágios atômicos e três agregados, fail-closed: para no primeiro que
      falha e propaga o código dele. `scripts/build.sh` deixou de cair para `npm` e de instalar sem
      lockfile congelado; agora exige pnpm, usa `--frozen-lockfile`, compila o workspace inteiro em
      release e confere que os binários existem antes de dizer que terminou. O workflow parou de reimplementar os comandos: cada step chama um estágio, um
      step por gate, e ganhou `cargo check --workspace`, que estava documentado como gate local e
      faltava no CI. Foram eliminadas as listas divergentes de comandos que existiam entre CI,
      `CONTRIBUTING.md` e `docs/development.md` — a do CONTRIBUTING era mais fraca que a do CI em
      quatro pontos. Nenhum arquivo de runtime, manifesto ou lockfile foi alterado, nenhuma
      dependência foi adicionada e a interface de máquina não foi tocada. Justificativa no ADR-043.
- [x] **Fase 4.0R — Auditoria de Segurança e Regressão.** Auditoria ofensiva sobre tudo o que a
      Fase 4.0 construiu, conduzida em rodadas (4.0R → R3 → R4 → R5) e fechada. Ela existiu para
      responder a uma pergunta só: um programa — e não uma pessoa — pode ser um escritor de primeira
      classe deste store sem estragá-lo? Os bloqueadores que ela encontrou foram fechados:

      **Identidade e locking (R-001, R-002/R-004).** A chave de coordenação passou a ser derivada do
      caminho *físico canônico* do diretório de notas: link simbólico, `./`, `..` e barras
      redundantes colapsam para a mesma autoridade e o mesmo lease, em vez de gerarem chaves
      distintas e dois gravadores sobre um mesmo diretório. A identidade de uma nota passou a ser
      ancorada no UUID do nome do arquivo, deterministicamente inclusive para uma nota sem front
      matter — que antes ganhava um UUID novo a cada leitura e produzia arquivos fantasmas a cada
      mutação. Uma divergência entre o nome do arquivo e o `id` do front matter passou a ser recusada
      nos dois sentidos, sem alterar nada, e as camadas de storage e write ganharam verificação
      explícita de que o documento gravado é o documento endereçado. Justificativa no ADR-044.

      **Concorrência otimista (R-016).** A questão que o lease não responde: ele serializa
      gravadores, mas não vê um gravador segurando uma base lida minutos antes — as duas gravações
      dizem "commitado" e uma das duas edições desaparece sem nada falhar. A `revision` fecha isso:
      o SHA-256 dos bytes exatos com que a nota seria persistida, publicado em toda leitura e aceito
      como precondição em toda mutação. Uma base obsoleta é recusada com `revision_conflict` e zero
      bytes alterados. `--if-revision` na CLI e `expected_revision` no protocolo; uma revisão
      malformada é erro de uso e nunca "sem precondição". A regra de releitura ficou explícita: um
      conflito exige olhar a nota de novo, nunca uma nova tentativa com a `current_revision`
      devolvida.

      **Protocolo privado v2.** O `expected_revision` havia sido adicionado ao protocolo interno sem
      mover o número da versão: os dois lados diziam "1", a checagem passava, e uma autoridade antiga
      descartava o campo em silêncio — uma gravação pedida como *condicional* era executada
      **incondicionalmente**, exatamente pelo mecanismo que deveria impedi-lo. `PROTOCOL_VERSION`
      passou a 2 e os dois sentidos recusam: nenhuma incompatibilidade transforma uma escrita
      condicional em incondicional, e não há modo degradado.
- [x] **Fase 4.1 — MCP.** Uma interface **Model Context Protocol** local, headless e tipada, para que
      um agente consulte e altere o Note-it sem possuir nenhum caminho capaz de contornar o Core, o
      writer lease, a autoridade de gravação, a identidade das notas ou a `revision`. Binário próprio
      `noteit-mcp`, em crate próprio, sobre o SDK oficial em Rust (`rmcp`), por **stdio e somente
      stdio**: o host faz `spawn` do processo e é dono do seu tempo de vida. Nenhum daemon, nenhuma
      porta, nenhum listener, nenhum HTTP, nenhuma configuração escrita em lugar nenhum — e nenhuma
      configuração de host do usuário tocada. Quinze tools de domínio: cinco de leitura, criação, oito
      mutações de nota existente e a restauração da lixeira. Deliberadamente **nenhuma** tool genérica
      de filesystem ou shell, e nenhum Resource, Prompt, sampling, elicitation ou extensão MCP Tasks —
      as tarefas Markdown do Note-it continuam sendo tools comuns.

      **A propriedade central: não existe gravação MCP incondicional sobre nota existente.** A CLI
      humana mantém o *last writer wins* sem `--if-revision`, porque quem digitou o comando está
      olhando para a nota; um agente não está. Então `expected_revision` é obrigatório no schema
      publicado de toda mutação, e o tipo que constrói uma mutação neste crate guarda um
      `NoteRevision` — não um `Option`. Um campo ausente é recusado pela desserialização antes de
      qualquer código do repositório rodar; uma revisão malformada é `invalid_input` e nunca "sem
      precondição". Um `revision_conflict` devolve as duas revisões, não devolve `revision` nem o novo
      conteúdo, e as descrições das tools dizem que a saída é reler e decidir de novo — nunca repetir.
      Um resultado `indeterminate` responde `commit_state: unknown` e nunca é repetido
      automaticamente.

      **Nada foi duplicado.** `authority.rs` mudou de `noteit-cli` para `noteit-core` e a CLI o
      reexporta: há uma única máquina de estados de "quem pode gravar agora", e o MCP a usa. O crate
      não abre um `.md`, não executa `noteit`, não interpreta a saída JSON da CLI e não reimplementa
      lease, socket, janela de retry, timeout, commit ou escrita atômica. `SCHEMA_VERSION` do
      `noteit --json` não mudou: são contratos independentes.

      Provado com processos reais, soquetes reais, stdio real e stores descartáveis: catálogo e
      schemas, pureza das leituras, toda variante de `NoteMutation` (a exaustão passou a ser
      realmente estrutural na 4.1R1, abaixo), corrida
      entre dois clientes, texto não salvo na janela sobrevivendo a um agente obsoleto, protocolo
      privado incompatível recusado sem fallback, aliases do store compartilhando um lease,
      identidade da nota, conteúdo hostil e stdout limpo. Um gate novo, `mcp-boundary`, verifica o
      limite headless — sem GTK, sem pilha HTTP/TLS/OAuth/SSE, sem banco, sem abertura de arquivo ou
      processo filho no `noteit-mcp/src`, sem escrita em stdout e com exatamente um lugar capaz de
      construir uma mutação condicional; a 4.1R1 acrescentou a ele as regras que faltavam sobre
      APIs de rede diretas. O MCP Inspector oficial confirmou o catálogo e o fluxo
      completo contra o binário de release. Contrato do agente em `docs/mcp.md`, justificativa no
      ADR-045.
- [x] **Fase 4.1R1 — Hardening da auditoria MCP.** Uma auditoria independente aceitou o
      comportamento do `noteit-mcp` e encontrou outra coisa: três lugares onde a documentação
      prometia uma garantia mecânica que o mecanismo não entregava. Nenhum era um bug — o servidor
      fazia a coisa certa — mas cada um era uma proteção que não protegia, e essas são piores que
      nenhuma, porque a próxima pessoa a mexer no código acredita nelas. As três foram reproduzidas
      antes de qualquer correção e fechadas.

      **`std::net` passava (AUD-01).** O boundary recusava crates de rede pelo nome. Um
      `std::net::TcpListener::bind("127.0.0.1:9999")` dentro de um handler compilou e o gate
      respondeu `MCP boundary OK`: a biblioteca padrão não aparece em `cargo tree`. "Sem rede"
      passou a ser verificado em quatro camadas — o grafo de dependências, as *features resolvidas*
      (`tokio` sem `net`, de modo que `tokio::net` não exista neste build), o código do crate
      (nenhum `std::net`, nenhum tipo de socket, Unix inclusive) e o **processo em execução**, que
      uma suíte nova inspeciona por `/proc/<pid>/fd`. As três primeiras descrevem o programa que foi
      escrito; a quarta pergunta ao núcleo o que ele tem aberto. (A quarta camada observava apenas
      as bordas da chamada; a 4.1R1.1, abaixo, tornou a observação contínua e estendeu a regra
      estática ao `noteit-core`.)

      **A matriz de mutações tinha duas fontes de verdade (AUD-02).** O `match` exaustivo forçava
      uma decisão sobre cada variante nova, mas a lista iterada era escrita à mão: uma variante
      remendada no `match` para compilar e esquecida na lista teria sido decidida e nunca
      exercitada. A matriz passou a ser declarada uma vez, por uma macro que gera a lista e o
      `match` das mesmas linhas — a linha que resolve o erro de compilação é agora a mesma que
      produz o valor testado. Provado injetando uma variante no Core: erro de compilação nomeando-a;
      acrescentada uma linha, os testes passaram a exercitá-la sozinhos.

      **`outcome_is_known` não era exaustiva (AUD-03).** Escrita com `matches!` e nunca chamada, ela
      prometia que um `WriteOutcomeKind` novo faria alguém olhar para a fronteira. `matches!`
      responde `false` para o padrão que não lista, então a variante nova compilava e ninguém olhava
      para nada. Saiu do crate e virou um `match` exaustivo em teste, que prende a decisão real —
      a saída MCP deliberadamente não publica `kind`, porque o agente sabe qual tool chamou.
      Demonstrado lado a lado: com uma variante injetada, o guard novo não compila e o antigo
      compilava e passava.

      Também nesta subfase: a contagem de testes MCP foi recalculada do próprio runner (60 de
      integração + 4 unitários = 64; o relatório da 4.1 dizia "55" por ter subtraído os unitários do
      total de integração em vez de somá-los); `cargo audit` foi executado de forma isolada, com
      `CARGO_HOME` e `--root` descartáveis, sem instalar nada no sistema — 183 dependências, 1239
      advisories, **zero vulnerabilidades e zero avisos**; e uma afirmação exagerada em
      `contract.rs` foi corrigida, pois aquele arquivo não importa nada do Core e o erro de
      compilação de uma renomeação aparece em `domain.rs`.

      **Nada do comportamento mudou.** Nenhuma tool foi acrescentada, removida ou alterada, e o
      catálogo publicado é byte a byte idêntico ao da 4.1 — verificado comparando a saída do MCP
      Inspector oficial antes e depois. Justificativa no ADR-046.
- [x] **Fase 4.1R1.1 — Fechamento da prova de ausência de rede.** Uma auditoria independente da
      4.1R1 encontrou um resíduo de prova, e apenas de prova: a suíte dinâmica fotografava os
      descritores **antes** e **depois** de cada chamada MCP, mas o comentário dizia que nenhum
      socket de Internet existia "em nenhum momento de uma gravação". Um socket aberto e fechado
      *dentro* do handler é invisível às duas fotografias. Reproduzido antes de qualquer correção —
      um `TcpListener` vinculado por 250 ms dentro de um handler, e os três testes passaram.

      **A observação passou a ser contínua.** Uma thread monitora amostra `/proc/<pid>/fd` durante
      toda a operação, com intervalo médio medido de 14 µs a 76 µs, e classifica cada socket no
      instante em que o vê. Com a mesma injeção reaplicada, a prova nova falha e identifica o
      socket positivamente como Internet.

      **E a frase passou a ter o tamanho do mecanismo.** Duas coisas foram medidas e mudaram o
      desenho: um socket `AF_INET` nunca vinculado **não** aparece em `/proc/net/tcp`, e um socket
      que fecha entre a leitura do descritor e a leitura da tabela já saiu dela — o laço de retry
      do caminho fail-closed produz dezenas desses, todos Unix legítimos. O classificador portanto
      não tem falsos positivos e pode ter falsos negativos, o que serve como detector adicional e
      não como garantia. Está documentado exatamente assim, e a garantia de família passou para a
      camada estática.

      **A regra estática passou a cobrir o `noteit-core`**, que é para onde o adaptador MCP delega
      quase tudo. A linha é a família do endereço e não a palavra "socket": AF_INET e AF_INET6
      proibidos nos dois crates, AF_UNIX permitido no Core — porque é assim que uma gravação chega
      à instância que segura o store —, acompanhado de uma asserção de que esse mecanismo continua
      existindo, para que a regra não possa ser satisfeita apagando o que ela protege.

      **O instrumento prova a própria sensibilidade.** A gravação pela autoridade serve de controle
      positivo: o Core abre um socket, entrega a mudança e o fecha dentro da mesma chamada, que é
      exatamente a forma que a prova anterior não enxergava. O monitor é obrigado a vê-lo; se não
      vir, o teste falha e o resultado limpo ao lado dele não é aceito.

      Sete provas negativas — `TcpListener`, `TcpStream`, `UdpSocket`, alias renomeado, as mesmas no
      Core, `tokio/net` no nível de features, e o socket transitório — todas reprovadas; e o socket
      Unix legítimo continua permitido. Nenhuma tool, nenhum schema, nenhuma dependência e nenhum
      comportamento mudaram; o `Cargo.lock` é byte-idêntico. Justificativa na ADR-047.

      **Gate técnico para início da Fase 4.2: LIBERADO.** Não resta blocker conhecido herdado da
      série 4.1.
- [x] **Fase 4.2 — IA/Segundo Cérebro.** Transformar o Note-it numa fonte de conhecimento local,
      recuperável e rastreável, para que uma IA externa use as notas como contexto através do MCP.
      **A IA continua fora do Core**: ela interpreta e raciocina, o Note-it armazena, identifica,
      busca, recupera e controla escrita. Markdown continua sendo a fonte da verdade. Arquitetura em
      `docs/second-brain.md`, justificativa na ADR-048.
  - [x] **4.2A — Arquitetura e contrato.** Gate arquitetural, sem funcionalidade: inventário do que
        existe, definição normativa do Segundo Cérebro v1, fronteiras de confiança, threat model,
        modelo de injeção de prompt, proveniência, orçamento de contexto, decisão de persistência e
        as subfases abaixo. Decidido: Context Engine somente leitura no `noteit-core`, cálculo sob
        demanda (sem índice e sem cache na 4.2), **uma** tool nova (`noteit_context`), sem Resources
        e sem Prompts, modo somente leitura delegado ao host via `readOnlyHint`, e um candidato de
        contexto que **nunca** carrega uma `revision` — porque uma revisão num candidato deixaria um
        agente gravar a partir de um trecho de 240 caracteres, e um carimbo de tempo é recusado por
        `NoteRevision::parse`, o que torna a proteção mecânica. Medido: busca custa 10 ms com 100
        notas, 48 ms com 1 000 e 435 ms com 10 000. Auditado o modelo de execução do MCP, com um
        finding registrado (ver 4.2B).
    - [x] **4.2A.R1 — Correção do contrato de staleness.** Revisão corretiva documental, sem
          funcionalidade. A 4.2A descrevia `updated_at` como a resposta a "quando a nota mudou?" e
          como o detector de staleness que substituía a `revision` ausente no candidato;
          `noteit-core/src/revision.rs` afirma o contrário desde a Fase 4.1, e os testes provam:
          uma tag, uma propriedade, uma cor, um papel ou um tamanho de fonte movem a `revision` sem
          mover `updated_at`. Corrigido: `updated_at` é sinal de recência textual, a `revision`
          continua sendo a única precondição autoritativa, e o candidato continua sem `revision` —
          a decisão de segurança não foi revertida, só explicada corretamente. Acrescentado à 4.2B
          o requisito de **coerência do candidato sob concorrência**; determinismo redefinido sem
          prometer snapshot transacional; `readOnlyHint` descrito como annotation e não como
          enforcement; findings do runtime e de `noteit_read` sem teto reconfirmados abertos.
          Nenhum `.rs`, nenhum schema, nenhuma dependência. Justificativa na ADR-048.1.
    - [x] **4.2A.R1.1 — Fechamento do contrato de autorização de escrita.** Duas portas que a R1
          deixou encostadas. Primeira: "a revisão só nasce em `noteit_read`" era largo demais —
          descreve o caminho que a D-13 protege e não o contrato MCP inteiro, porque
          `WriteResult.revision` existe para encadear a próxima escrita condicional sem leitura
          extra. A regra correta é mais estreita: nenhuma revisão autoriza escrita sobre um estado
          que o agente não conhece. Duas origens autorizam — `noteit_read` e a revisão pós-operação
          de uma mutação bem-sucedida; a `current_revision` de um conflito nunca autoriza, porque
          nomeia conteúdo que o agente não viu. A D-13 não muda: uma nota vinda do contexto exige
          `noteit_read` antes da **primeira** mutação. Segunda: a coerência do candidato era
          requisito e trazia junto a permissão de descumpri-lo com aviso; a alternativa foi removida
          e **D-27 é obrigatória**. Nenhum `.rs`, nenhum schema, nenhuma dependência. Justificativa
          na ADR-048.2.
  - [x] **4.2B — Context Engine v1 no Core.** Duas entregas, nesta ordem, e a ordem era o ponto.

        **O protocolo primeiro.** O servidor MCP rodava um runtime *current-thread* com quinze
        tools síncronas, então uma chamada ao Core acontecia na mesma thread que lê a entrada
        padrão: uma operação lenta parava o servidor inteiro. Dois comentários afirmavam o
        contrário — `main.rs` dizia que o I/O ia para uma blocking thread, e o `Cargo.toml` do
        crate dizia que ia para `spawn_blocking`. Não havia `spawn_blocking` nenhum. Agora há, e
        ele é a única porta: toda função de `domain.rs` que abre o store exige um `OffThread`,
        cujo campo é privado ao módulo e que só `off_reactor` constrói, dentro do fecho que o
        `spawn_blocking` executa — uma chamada ao Core no reactor não compila. Leituras também,
        não só escritas. O runtime continua `current_thread`, porque nunca precisou de mais de
        uma thread; precisava parar de fazer o trabalho do disco nela. Falha de `join` virou erro
        interno tipado que não cita nada do que estava em execução, porque uma mensagem de pânico
        pode carregar a nota. Sem dependência nova e `Cargo.lock` byte-idêntico.

        Provado por dois testes, nenhum por `sleep`: uma autoridade falsa abre um portão no
        instante em que recebe a escrita e só responde quando o teste abre o segundo, com o `ping`
        entre os dois; e, no caminho de leitura, que não tem autoridade para segurar, a prova é de
        ordem — uma busca sobre um store grande e um `ping` atrás dela, que precisa responder
        antes. Ambos reprovavam no commit anterior.

        **Depois o motor.** `noteit-core/src/context.rs`, somente leitura, tipado, sem tipo algum
        de MCP. Sinais de texto, tag, propriedade, tarefa e recência, cada um explicável por um
        `Reason` de conjunto fechado; nenhum score. Candidato sem `revision`, sem caminho e sem
        corpo completo: `note_id`, label, snippet de no máximo 240 caracteres, `updated_at`,
        `reason[]` e `matched_text`. Limites no Core — 10 por padrão, 50 no teto, consulta de 512
        caracteres recusada e não truncada — para que a tool da 4.2C não precise inventá-los.
        Truncamento nunca silencioso: `truncated` e `omitted_count`. Lixeira fora, symlink
        recusado pelo Core, nota ilegível vira warning e nunca candidato parcial.

        **D-27 por construção.** Uma leitura autoritativa do `NoteDocument` por candidato, uma
        `Projection` derivada dela, e todo sinal daquele candidato saindo dessa projeção — a
        varredura que enumera as notas não compõe o candidato; as funções de sinal recebem `&Projection` e
        nenhuma tem caminho até o store. Provado sob concorrência real: uma thread alterna a mesma
        nota entre duas versões que discordam de tudo — corpo, tag, propriedade e tarefa — e
        nenhum candidato mistura as duas. O teste foi verificado contra um defeito injetado de
        propósito, que ele reprovou. Sem snapshot global, sem lease de leitura, sem camada de
        coordenação nova.

        Determinismo com ordenação total: mais motivos primeiro, depois recência (sem `updated_at`
        por último), depois `note_id` — para que um empate não caia na ordem do filesystem.

        Medido em release, store sintético, medianas de 9 execuções: 6,5 ms com 100 notas, 66 ms
        com 1 000, 704 ms com 10 000; pico de 8 MiB de memória com 10 000 notas. Linear, cerca de
        1,6× a busca, porque lê o `NoteDocument` inteiro de cada nota — é disso que a coerência
        depende. Nenhum índice foi criado para melhorar o número; isso continua sendo 4.3.

        Catálogo MCP continua com 15 tools: `noteit_context` é da 4.2C.
    - [x] **4.2B.R1 — Orçamento de saída e barreira do offload.** Correção antes de publicar
          qualquer coisa. A 4.2B limitou o que a resposta *listava* e não o que cada item podia
          carregar: `tasks[]` e `warnings[]` cresciam com o conteúdo do store, e uma auditoria dos
          tipos públicos encontrou mais dois. `matched_text` era ilimitado porque a dobra descarta
          marcas combinantes — `a` mais cinquenta mil acentos mais `b` dobra para `ab`, casa com
          uma consulta de dois caracteres e publicava os cinquenta mil (medido). E a mensagem de
          warning do Core nomeia o arquivo, então caminho absoluto chegava à resposta, contra a
          regra de que a IA nunca recebe caminho. Agora: 3 tarefas por candidato, 121 caracteres
          por tarefa, 241 por `matched_text`, 20 warnings, e um warning que é `note_id` + `kind`
          sem texto livre — tamanho fixo e caminho impossível por construção. Truncamento
          contado, nunca silencioso; `task_ref` **não** é truncado, porque um identificador
          encurtado não nomeia tarefa nenhuma.

          A barreira: `OffThread` impedia esquecer o offload nas funções de `domain.rs`, mas não
          impedia abrir uma segunda porta — uma 16ª tool chamando `noteit_core` direto do handler
          satisfaria todos os tipos e ainda travaria o protocolo. O gate agora recusa acesso ao
          store nomeado fora de `domain.rs`, e exige que a porta continue sendo porta:
          `spawn_blocking` presente, `reader` e `perform` exigindo o testemunho, e exatamente uma
          fábrica de `OffThread`. Cinco violações foram injetadas e as cinco reprovaram.

          Também corrigido: "uma leitura por nota" era literal demais. A afirmação exata é uma
          leitura **autoritativa** do `NoteDocument` por candidato — a varredura que enumera as
          notas roda antes e não compõe o candidato. D-27 inalterada. Justificativa na ADR-049.1.
    - [x] **4.2B.R1.1 — Fechamento do canal de erro.** A última fresta livre da superfície de
          contexto. `ContextError::StoreUnavailable` carregava a mensagem do storage, que nomeia o
          diretório — medido, o `Display` imprimia o caminho absoluto do store. Fechado pela forma
          do tipo e não por saneamento: a variante deixou de ter payload e o `Display` é uma frase
          fixa. `QueryTooLong` mantém `limit` e `actual`, que são inteiros e não ecoam a consulta,
          e as duas recusas continuam distinguíveis. Provado com um store cujo caminho de notas é
          um arquivo regular — reproduzível em qualquer lugar, sem privilégio nenhum.
          Justificativa na ADR-049.2.
  - [x] **4.2C — Superfície MCP de conhecimento.** A tool `noteit_context`, publicada. O catálogo
        passou de 15 para 16 e essa é a única adição; `SCHEMA_VERSION` continua em 1, porque ele
        versiona o documento da interface de máquina da CLI e não o catálogo MCP — verificado antes
        de não ser tocado.

        Uma fase de tradução, e a disciplina é essa: `contract.rs` declara tipos MCP próprios,
        `domain.rs` copia campo a campo, e nada é recalculado. O adapter não lê nota, não ordena,
        não constrói snippet, não parseia tarefa e não recalcula truncamento — todo contador vem do
        Core, porque um número recomputado depois do corte só poderia ser um palpite sobre o que já
        foi descartado.

        `tags` e `properties` entram como **sinais** e o schema diz isso, em vez de reusar a
        redação do `FilterInput` — "toda tag que a nota precisa ter" seria um schema mentindo sobre
        o comportamento. As tarefas ficam **dentro** do candidato, e não numa lista global: o Core
        já as modela assim, o truncamento é por candidato, e fica evidente de qual nota cada
        conjunto nasceu.

        O que a tool nunca devolve: corpo de nota, revision de tipo algum, caminho, mensagem livre
        e score. Warning de contexto é `code` + `note_id`; recusa é `status` + `code`. Provado por
        varredura recursiva dos nomes de propriedade dos dois schemas — dos nomes, não das
        descrições, que mencionam `revision` de propósito para dizer que não existe.

        O caminho é `handler → offload → domain → context::retrieve`, e a barreira da R1 provou
        seu valor aqui: duas violações injetadas — a chamada direta e a mesma coisa escondida atrás
        de `use noteit_core::context as engine` — e a segunda **passou**, então a regra foi
        ampliada para nomear o módulo além da chamada. As duas reprovam agora.

        Concorrência provada para a tool nova nas duas direções: um `ping` ultrapassa uma consulta
        de contexto varrendo o store, e uma consulta de contexto responde enquanto uma escrita está
        presa dentro do Core.
  - [x] **4.2D — Contrato do agente.** Como um agente deve usar o Note-it, e a descoberta de que
        uma das regras não era cumprida por mecanismo nenhum. Um `revision_conflict` publicava
        `current_revision` — a revisão que a nota tem agora — e as instruções mandavam não
        reutilizá-la. Reproduzido antes de mexer em nada: o agente leu em R1, outra pessoa
        acrescentou um parágrafo criando R2, a escrita em R1 foi recusada, o conflito devolveu R2,
        e reenviar R2 sem nunca ter lido comitou. O parágrafo da pessoa sumiu.

        A regra deixou de ser uma frase e virou a ausência do campo: o adapter lê
        `current_revision` do erro do Core e a descarta. O Core continua com ela, porque é tipo de
        domínio compartilhado. Nenhum substituto — publicar a mesma capacidade como
        `latest_revision` ou `etag` não mudaria nada, já que o problema nunca foi o nome.

        `WriteResult.revision` fica exatamente como estava: depois de uma escrita bem-sucedida o
        agente conhece o estado resultante, então uma sequência de escritas não precisa de leitura
        entre elas. As `INSTRUCTIONS` foram reescritas para nomear as **duas** origens legítimas de
        revisão — a leitura e a escrita própria confirmada — em vez da única que diziam antes, que
        era a contradição registrada como 4.2D-F001.

        Máquina de estados do agente publicada em `docs/mcp.md`, com a matriz de comportamento por
        resultado e a separação explícita entre o que é mecânico e o que é normativo — a promessa
        não é que nenhum cliente jamais grave um estado não lido, e sim que o servidor não lhe
        entrega mais um token para isso. Fechado junto o `4.2C-DOC-001`: os limites documentados
        contam conteúdo selecionado, e a reticência do truncador faz a string publicada chegar a
        242. Justificativa na ADR-051.
  - [x] **4.2E — Validação ponta a ponta.** As peças foram construídas separadamente; esta fase
        perguntou se elas ainda dão as mãos. Vinte e cinco cenários contra o binário real, por
        pipes reais, sobre stores descartáveis: pergunta vira candidatos, candidato vira leitura,
        leitura vira escrita condicional, e a resposta da escrita vira a base da seguinte — ou um
        conflito a reconciliar, ou um resultado que ninguém pode afirmar.

        Nenhum defeito de produção apareceu, e a fase alterou apenas testes e documentação.

        **`4.2D-TEST-001` fechado.** O teste do no-op aceitava "revision presente ou ausente". Ela
        é presente, e agora é afirmado: um no-op nomeia o estado em que a nota já estava, e esse
        estado encadeia. Provado nos **dois** caminhos de escrita — direto e pela autoridade — e o
        fallback permissivo foi removido, porque aceitar dois comportamentos era o que deixaria os
        dois caminhos divergirem sem ninguém notar.

        **Direct e Authority publicam o mesmo.** Append, no-op e conflito executados pelos dois
        caminhos e comparados campo a campo: mesmo `status`, mesmo `commit_state`, mesmo `changed`,
        mesma presença de `revision`, mesmo corpo final. Quem segura o lease é detalhe interno.

        **`indeterminate` nas duas metades.** Uma autoridade que comita e cai antes de responder, e
        outra que cai antes de comitar. De fora são idênticos — e é exatamente por isso que repetir
        é proibido: só uma leitura distingue. No caso comitado, o parágrafo aparece uma vez.

        **Texto não salvo continua protegido.** Com uma janela segurando texto que ninguém salvou,
        a escrita do agente sobre a revisão do arquivo é recusada com `revision_conflict`, o
        arquivo não muda, e o texto não salvo não vaza pela recusa. Registrado também que o Context
        Engine descreve o store persistido e não enxerga a janela — é a arquitetura como foi
        construída, e agora há teste para que uma mudança nisso seja decidida e não descoberta.

        Duas escritas sobre a mesma revisão: exatamente um commit e exatamente um conflito, nunca
        as duas. Conteúdo hostil continua dado — inclusive uma revisão de 64 hex escrita dentro da
        nota, que é recusada. Contexto continua limitado sob store adversarial, sessão de leitura
        deixa o store byte-idêntico, e uma consulta de contexto responde com uma escrita presa
        dentro do Core. Justificativa e limites em ADR-052.
  - [x] **4.2R — Auditoria ofensiva do Segundo Cérebro.** Matriz ofensiva completa contra a
        baseline `c5fe1bb`, em store sintético, com o store real byte-idêntico do começo ao fim.
        Cinco achados materiais reproduzidos, corrigidos e provados; nenhum aberto.

        **`4.2A-002` fechado.** `noteit_read` não tinha teto: 16 MiB de nota respondiam em
        34 226 387 bytes, 7,8 s e 153 MB de processo, crescendo linearmente. A resposta agora
        tem teto de 4 MiB medido no fio — não em `content.len()`, porque o SDK publica o payload
        duas vezes e o escape JSON expande o corpo em 2,04× (ASCII) a 2,88× (aspas, contrabarras,
        emoji). Acima do teto, `response_too_large` **sem corpo e sem revision**: entregar parte
        de uma nota junto da revisão do todo autorizaria gravar sobre o que nunca foi lido, que é
        a falha que a ADR-051 fechou no conflito. O número é quatro vezes o `MAX_FRAME_BYTES` de
        1 MiB do protocolo privado, então toda nota que a escrita consegue carregar inteira a
        leitura consegue publicar inteira. Fronteira medida: o maior sucesso pesa exatamente
        4 194 304 bytes e um byte a mais de nota vira uma recusa de 533. ADR-053.

        **Quatro achados novos, todos da mesma raiz.** `noteit_list`, `noteit_search` e
        `noteit_tasks_list` publicavam o **caminho absoluto** do arquivo dentro do `message` de
        um warning, e `noteit_read` fazia o mesmo numa falha de permissão (`4.2R-001`); as mesmas
        três publicavam um warning por arquivo danificado sem teto — 2 000 symlinks viravam 920 KB
        para um `limit: 1` (`4.2R-002`); `noteit_trash_list` não tinha teto nenhum — 20 000 notas
        descartadas responderam em 9 595 659 bytes (`4.2R-003`); e mensagens públicas repetiam a
        entrada e o front matter da nota no tamanho em que chegaram, um seletor de 300 000 bytes
        voltando em 300 098 (`4.2R-004`). A correção é uma: `message` agora é `&'static str`,
        escolhido pelo `code`, então uma frase montada em tempo de execução não tem como chegar
        ao fio; um warning é `code` e `note_id` em todas as leituras, com teto de 20; a lixeira
        tem teto de 100. ADR-054.

        **O que foi atacado e não produziu achado.** Revisão de outra nota, notas de corpo
        idêntico, revisão citada dentro de uma nota, ABA, contexto obsoleto, candidato movido
        para a lixeira, JSON-RPC hostil dentro da nota, injeção de frame e de linha no stdout,
        traversal em todo seletor, symlink pendurado e para diretório, identidade divergente
        entre nome e front matter, `task_ref` de outra nota, `limit` adversarial, campos de
        entrada não publicados, YAML com chaves duplicadas e bomba de alias, Unicode combinante,
        ZWJ, RTL, emoji, CJK, I turco e ß, reinício sobre o mesmo store. Nenhum panic, nenhuma
        linha espúria no stdout, nenhum canário no stderr, nenhum estado oculto. Suíte em
        `noteit-mcp/tests/mcp_second_brain_red_team.rs`; catálogo em 16 tools, `mutation_input!`
        em 8, zero dependência nova, `Cargo.lock` byte-idêntico.
    - [x] **4.2R.R1 — Fechamento da desserialização pré-handler.** A 4.2R foi dada
          por encerrada e uma revisão posterior reabriu `4.2R-004` numa fronteira
          que ela não tinha olhado. A regra "toda mensagem pública é uma frase que
          o servidor escreveu" valia para o que o **domínio** diz, e o domínio só
          fala depois que os argumentos foram desserializados. Antes disso o
          extractor de parâmetros do SDK respondia uma falha de desserialização
          com a frase do `serde_json`, que cita o valor recusado por inteiro.

          Reproduzido no fio, contra o binário real, com store sintético e o
          store real byte-idêntico do começo ao fim: `limit` recebendo 300 KiB de
          string respondeu em 307 361 bytes com o canário; `state` como variante
          inválida em 307 387; `include_tasks` e `clear` como string em 307 367;
          `tags[]` e `properties[]` recebendo escalar em 307 368 e 307 374. E uma
          camada acima, o `method` de uma requisição que não roteia era devolvido
          pelo nome: 307 261 bytes. Classificado `S3`.

          **Por que passou pela 4.2R.** Havia um teste enviando exatamente esses
          valores — o `r16` da suíte ofensiva, que percorre `limit` adversariais —
          e ele pulava a recusa sem examiná-la. O teste que tinha a entrada certa
          na mão tinha decidido que uma recusa não precisava ser olhada. Ele agora
          afirma o tamanho e o conteúdo da recusa, que é a lição além do defeito.

          **Corrigido na fronteira, não campo a campo.** `SafeParameters<T>` em
          `noteit-mcp/src/params.rs`, importada por `server.rs` como
          `Parameters` — que é o nome que a macro `#[tool]` procura para derivar o
          `inputSchema` —, então toda tool a herda e uma tool nova não tem como
          esquecê-la. O erro é descartado sem ser lido e a recusa é uma constante.
          Argumento inválido passou a ser erro de protocolo `-32602`, que é a
          classificação do próprio MCP e o contrato mais coerente: um
          `CallToolResult` deste servidor sempre carrega `structuredContent`, e a
          recusa antiga não carregava. `on_custom_request` sobrescrito para não
          ecoar o método.

          **Medido depois:** 112 e 113 bytes; o método desconhecido, 103. E a
          forma forte da propriedade — 1 KiB, 64 KiB, 300 KiB e 1 MiB no mesmo
          campo recebem **o mesmo** número de bytes, o que um teto não provaria.
          Os 16 tools continuam; os 15 schemas foram comparados documento a
          documento contra os do tipo embrulhado; `expected_revision` continua
          exigida no schema e no tipo; `revision_conflict` inalterado. Cinco
          regras novas em `check-mcp-boundary`, sete violações injetadas e as sete
          reprovaram. Suíte em `noteit-mcp/tests/mcp_argument_boundary.rs`. Zero
          dependência nova, `Cargo.lock` byte-idêntico. Justificativa na ADR-055.

          **O que se perde, dito por inteiro:** a recusa não nomeia mais o campo
          errado. Ele vinha do `serde_json`, e o `serde_json` só o dá dentro da
          frase que repete a entrada. Os campos obrigatórios estão publicados no
          `inputSchema`.

          **Gate técnico para início da Fase 4.3: LIBERADO.** Não resta blocker
          conhecido herdado da série 4.2. A 4.2 foi dada por encerrada uma vez
          antes desta R1 e não estava; o que fechou a diferença foi uma
          reprodução no fio, e é essa a régua para a próxima.
- [ ] **Fase 4.3 — Recuperação semântica independente de fornecedor.** Liberada pela 4.2R.R1.
      Recuperação semântica *provider-neutral*, com caminho local e offline e providers remotos
      opcionais configurados explicitamente pelo usuário; índice derivado e reconstruível; ranking
      híbrido avaliado por benchmark; proveniência entre nota, revisão, chunks e vetores; e
      **nenhuma alteração na autoridade de escrita das notas**. O Note-it não está construindo uma
      IA local nem um cliente de nuvem nenhuma: está construindo uma memória semântica cuja fonte
      da verdade são as notas e cuja recuperação usa o mecanismo que o usuário escolher — a IA que
      raciocina pode mudar, o provider pode mudar, o índice pode ser apagado e reconstruído, e as
      notas continuam sendo as notas. Especificação em `docs/semantic-retrieval.md`, justificativa
      na ADR-056.

      As subfases B a R abaixo são **planejadas e sujeitas às conclusões da própria 4.3A**: se um
      benchmark mostrar que duas podem ser fundidas, funde-se com justificativa; se uma não for
      necessária, remove-se com ADR.
  - [x] **4.3A — Arquitetura, benchmark e contrato multi-provider.** Gate arquitetural, sem funcionalidade:
        nenhum `.rs` tocado, catálogo em 16 tools, `Cargo.lock` byte-idêntico, zero dependência.
        A fase mediu antes de decidir, e a medição mudou a ordem da 4.3.

        **O que foi medido.** Corpus sintético versionado em `docs/retrieval-corpus.json` — 30
        notas e 32 consultas com ground truth explícito, em 18 categorias, incluindo paráfrase,
        sinônimo, acentos, misto português/inglês, nota longa com trecho pequeno, prompt injection
        guardado como conteúdo, Unicode hostil e duas consultas sem resposta. O baseline foi medido
        contra o **binário real**, por stdio, sobre store sintético; uma reimplementação em Python
        reproduziu os 32 rankings do motor real byte a byte, e só então foi usada para variar o
        desenho.

        **O achado que reordena a fase.** O Context Engine casa a consulta inteira como *substring*:
        não há casamento por termo. Dezenove das trinta consultas com resposta voltam **vazias**, e
        o baseline é R@1 0,333 / R@3 0,367 / MRR 0,350. "hipertensão arterial" não acha a nota sobre
        pressão alta. Nenhuma dessas falhas é por falta de semântica.

        BM25 por termos leva R@3 de 0,367 a **0,767** — sem dependência, sem modelo, sem cache e sem
        superfície de privacidade nova. O passo semântico acrescenta mais 0,13, e custa um artefato
        de 100 a 512 MB. Os dois se justificam; a ordem passou a ser decidida por número.

        **Decidido.** Ampliar o Context Engine e não criar motor paralelo — ele tem hoje um único
        consumidor, o MCP. Embeddings **estáticos de token** e não transformer: 1 250–1 400 notas/s
        contra 23–29, qualidade dentro do ruído, e nenhum runtime de inferência, o que mantém ONNX
        Runtime, C++ e download de binário em tempo de build longe do Core. Encadeamento e não
        fusão: o lexical vem primeiro e o semântico preenche o resto, porque a RRF pontuou um pouco
        melhor e **rebaixou um acerto exato**. Chunk por parágrafo com teto de 800 caracteres,
        identidade `note_id` + `revision` + ordinal — a revisão canônica já é o detector de
        staleness. Índice **em memória, força bruta, sem persistência**: consulta custa 3,5 ms com
        10 000 vetores e indexar 10 000 notas custa 7 s, enquanto o store real desta máquina tem 41
        notas e custa 30 ms. Sem score publicado; um `Reason::SemanticMatch` no lugar.

        **A medição que restringe a arquitetura.** Nenhum limiar de similaridade separa "tem
        resposta" de "não tem resposta" — as faixas se sobrepõem nos três modelos testados. Hoje o
        motor devolve vazio quando nada casa, e isso é verdade; um motor semântico sempre tem
        vizinho mais próximo. Por isso candidato puramente semântico é rotulado e limitado, em vez
        de cortado por um número que não separa nada.

        **A recuperação é independente de fornecedor.** Um contrato `EmbeddingProvider` com
        `embed_document` e `embed_query` separados — porque o `e5` exige prefixos, a Voyage
        prepende instruções por `input_type` e o Gemini tem tipos de tarefa —, e um
        `EmbeddingSpaceId` que responde a única pergunta que importa: estes dois vetores podem ser
        comparados? **Dimensão igual não é compatibilidade, e isso foi medido**: truncar os vetores
        de um modelo para a dimensão de outro produz números perfeitamente calculáveis e derruba o
        R@3 de 0,933 para 0,133, sem que nada no cálculo avise.

        **Proveniência, medida.** `EmbeddingRecord` carrega `source_revision`, que é a revisão
        canônica que o Core já calcula — não se inventa um segundo detector de estado. Uma nota
        indexada com o texto A e editada para B devolve o candidato obsoleto em primeiro lugar sem
        validação; comparando `source_revision` com a revisão atual ele desaparece. A ordem é
        **ler primeiro e validar depois** — a revisão atual é `sha256` do `NoteDocument`
        serializado, e não existe o que comparar antes de carregá-lo (corrigido na 4.3A.R1 e o
        diagrama na 4.3A.R1.1). O custo é zero em I/O, porque o motor já faz exatamente uma leitura
        autoritativa por candidato (D-27) e a validação pega carona nela; **é essa leitura que
        produz o snippet publicado, nunca o cache**. `source_revision` é chave de cache e mais nada: nunca
        é publicada, nunca chega ao agente, nunca autoriza escrita — o atalho
        `embedding → revision → write` é proibido.

        **Rede sem afrouxar a fronteira.** O provider remoto vive num processo separado,
        `noteit-embed`, que é o único com cliente HTTP e o único que vê a credencial, falando com o
        Core pelo mesmo AF_UNIX que a autoridade de escrita já usa e que o gate já permite por
        nome. Assim `noteit-mcp` e `noteit-core` continuam sem crate HTTP no grafo, sem
        `std::net` e sem credencial. O worker só existe quando um provider remoto está
        configurado. Providers remotos verificados em fontes oficiais em 2026-09-04 — OpenAI,
        Gemini e Voyage, com modelos, dimensões, limites e preços registrados — e **nenhum medido**,
        por não haver credencial na sessão: documentação de fornecedor não vira benchmark interno.
        Não existe provider da Anthropic, que não tem modelo próprio de embeddings e aponta a
        Voyage.

        **Padrão recomendado:** lexical por termos, sem modelo, sem chave e sem download. Nenhuma
        credencial remota é requisito do primeiro uso, e nem o modelo local é.

        **O que não foi medido, dito por inteiro:** nada foi implementado em Rust e os números vêm
        de um protótipo Python com ONNX Runtime; **nenhum provider remoto foi medido**, por
        ausência de credencial; RSS não foi medido de forma utilizável; o corpus tem 32 consultas e
        não separa dois modelos parecidos; quantização int8 não foi avaliada em qualidade; a
        licença de `model2vec-rs` não foi verificada; e o `voyage-4-nano`, de pesos abertos, não
        foi avaliado como provider local.

        **Fechado junto:** DOC-01 e DOC-02, duas frases da 4.2R.R1 que descreviam comportamento que
        o código não tem mais.
    - [x] **4.3A.R1 — Correção do contrato arquitetural.** Uma auditoria externa
          encontrou sete contradições no contrato, e elas tinham de cair antes que a
          4.3B materializasse os tipos em Rust. Documental: nenhum `.rs`, nenhuma
          dependência, `Cargo.lock` byte-idêntico.

          **O papel da entrada saiu da identidade do espaço.** A 4.3A punha
          `task = document/query` dentro do `EmbeddingSpaceId` e ao mesmo tempo exigia
          igualdade exata para comparar dois vetores — o que se contradizia, porque uma
          busca compara justamente uma consulta com documentos: sob aquela regra,
          nenhuma busca seria válida. Agora são três coisas: o espaço, o
          `EmbeddingRole`, e a receita de preparação, versionada **em par**, porque
          mudar só a receita de consulta invalida a comparação tanto quanto mudar a de
          documento.

          **A alegação de staleness sem leitura era falsa, e foi conferida no código.**
          As únicas formas de obter uma `NoteRevision` são `for_document`, que faz
          `sha256` do documento canônico serializado inteiro, e `parse`, que não calcula
          nada; a varredura lê front matter e `mtime`, e `NoteSummary` não tem campo
          `revision`. Não existe caminho autoritativo sem carregar o `NoteDocument`, e a
          R1 recusou-se a criar um — uma segunda definição de estado é o defeito que a
          4.2A.R1 registrou, e `updated_at` não serve porque fica parado quando uma tag
          muda. O custo real, porém, é **zero em I/O**: o motor já faz exatamente uma
          leitura autoritativa por candidato (D-27), e a validação pega carona nela.

          **Identidade de artefato.** Nome de modelo não basta: pesos, tokenizer,
          normalização ou receita podem mudar mantendo nome e dimensão, e a classe de
          defeito medida na 4.3A volta inteira. No local, `sha256` dos bytes carregados,
          obrigatório. No remoto, identificador versionado quando o provider publica um;
          e quando só há alias mutável, o espaço é marcado **não verificável** e o
          usuário vê isso — em vez de fingir uma garantia que não existe.

          **"Não rebaixar acerto exato" virou invariável estrutural** e passou a valer
          contra o BM25 e não só contra o semântico: três camadas concatenadas —
          `TextMatch`, `TermMatch`, `SemanticMatch` — sem reordenação entre elas, com
          desempates deterministas terminando em `note_id`.

          **O padrão ficou inequívoco.** A 4.3A dizia "LOCAL — o padrão" numa seção e
          "DEFAULT lexical" em outra. Agora: padrão de fábrica `lexical_only`; `local` é
          o padrão apenas **dentro** de `mode: semantic`; remoto sempre nomeado. Nenhuma
          leitura permite que uma atualização passe a enviar conteúdo ou baixar modelo.

          **`SemanticMatch` é canal de admissão**, não "sem palavra em comum" — que era
          falso e transformava um fato sobre o servidor numa afirmação sobre o texto.

          **`k1` e `b` congelados antes da medição**, e o corpus declarado régua de
          regressão e não conjunto de validação: ajustar num conjunto e apresentar a
          métrica dele como validação seria medir o próprio ajuste, e com 32 consultas o
          ajuste cabe dentro do ruído.
    - [x] **4.3A.R1.1 — Fechamento da consistência documental.** Resíduo que a R1 deixou e um
          endurecimento. A R1 corrigiu o texto do fluxo de proveniência e **não corrigiu os
          diagramas**: o da seção 1 e o do pipeline ainda mostravam candidato → validação →
          leitura, ordem impossível pelo próprio contrato corrigido, já que a revisão canônica
          atual é `sha256` do `NoteDocument` serializado e não há o que comparar antes de
          carregá-lo. Havia três versões do fluxo em circulação — a correta em prosa, e duas
          invertidas em diagrama, uma delas na própria entrada da 4.3A aqui. Agora há **uma só**:
          candidato preliminar → uma leitura autoritativa → `NoteRevision::for_document` →
          validar → snippet, motivos e tarefas da mesma leitura.

          E `artifact_identity` deixou de ser `sha256` de componentes concatenados para ser o
          `sha256` de um manifesto — `ArtifactManifestV1` em JSON canônico sob separador de
          domínio versionado. Concatenar componentes de comprimento variável é ambíguo: duas
          decomposições diferentes podem dar a mesma cadeia de bytes, e portanto a mesma
          identidade para artefatos distintos, que é exatamente a classe de defeito que a
          identidade existe para fechar.
    - [x] **4.3A.R1.2 — Política de admissão e ranking, fechada.** As três camadas que a R1
          definiu cobriam `TextMatch`, `TermMatch` e `SemanticMatch` — e o motor também admite
          por `SharedTag`, `PropertyMatch`, `TaskMatch` e `Recent`, que ficaram sem lugar. A
          4.3B teria de inventar em Rust onde eles entram.

          **A política foi derivada do comportamento medido, não do conveniente.** Contra o
          binário real: hoje `TextMatch` **não** tem precedência sobre `SharedTag` nem
          `PropertyMatch` — a ordem é por contagem de motivos, e uma nota com `shared_tag` +
          `property_match` fica **acima** de uma com `text_match` sozinho. Uma fila de cinco
          camadas com `TextMatch` no topo teria mudado isso em silêncio.

          Então são quatro classes: a 1 é o conjunto de admissão que o motor já tem — os quatro
          sinais declarados, com a regra de ordenação que ele já usa; a 2 e a 3 são
          **estritamente aditivas**, acrescentando candidatos abaixo de tudo o que já existia; e
          a 4 é a recência, exclusiva, que só existe quando a requisição não tem consulta nem
          filtro. A proteção do acerto exato fica na forma em que é verdadeira: `TextMatch` nunca
          é rebaixado **por `TermMatch` nem por `SemanticMatch`**, e continua podendo ficar atrás
          de um `SharedTag` com mais motivos, como hoje.

          Registrado também o que cada forma de requisição produz — filtro sozinho não tem classe
          2 nem 3, porque não há termo a pontuar nem consulta a embutir; requisição vazia continua
          sendo só recência; e uma consulta feita de marcas combinantes dobra para vazio e devolve
          nada, sem cair em `Recent`, que é o comportamento atual e fica registrado como tal.

          Fechados junto: `k1` e `b` deixaram de aparecer como questão aberta da 4.3C, e a
          garantia atribuída ao separador de domínio passou a ser a correta — separação semântica
          entre domínios, e não impossibilidade de colisão, que é propriedade do SHA-256 e não de
          um prefixo. Dez cenários de regressão congelados para a 4.3B rodar **antes** do BM25.
  - [ ] **4.3B — Motor de recuperação provider-neutral.** Planejada. Os tipos centrais e o motor,
        sem nenhum provider remoto e sem artefato de modelo. **Começa pelo lexical**, porque é o
        que a 4.3A mediu como maior ganho e menor custo, e porque é o piso para onde tudo o mais
        degrada: casamento por termo e ranking BM25 dentro do Context Engine, sobre a dobra que o
        `search::fold` já faz, com o corpus da 4.3A como regressão. Depois, o esqueleto que a parte
        semântica vai ocupar: `EmbeddingProvider`, `EmbeddingSpaceId`, `EmbeddingRecord`, chunker
        versionado, identidade de chunk, validação de proveniência, índice abstrato em memória,
        encadeamento lexical→semântico, política de fallback e invalidação. Sem dependência nova.
  - [ ] **4.3C — Provider local.** Planejada. A implementação do provider local escolhido pela
        4.3A — embeddings estáticos de token, sem runtime de inferência —, distribuição do
        artefato, ciclo de vida, custo de CPU e memória medidos em Rust, operação offline e
        indexação incremental. As questões que a 4.3A deixou abertas são pré-requisito: qualidade
        sob quantização, licença de `model2vec-rs` e RSS real. **`k1` e `b` não estão entre
        elas**: foram congelados em 1.2 e 0.75 e a 4.3B os usa exatamente assim. Reabri-los exige
        três coisas e não duas — um conjunto de tuning novo, um conjunto de avaliação separado que
        não seja usado no ajuste, e a decisão explícita de reabrir os parâmetros. O corpus da 4.3A
        continua sendo régua de regressão e não serve para nenhuma das duas primeiras. Pode ser
        fundida com a 4.3B se a implementação mostrar que é mais simples.
  - [ ] **4.3D — Providers remotos opcionais.** Planejada. OpenAI, Gemini, Voyage ou outros
        aprovados, sempre opt-in: o processo `noteit-embed` separado, que é o único com cliente
        HTTP e o único que vê a credencial, falando com o Core por AF_UNIX — de modo que a
        fronteira de rede do MCP e do Core seja **estendida e não afrouxada**. Credenciais, lote,
        limites de taxa, timeouts, erros tipados, aviso de privacidade, troca de provider e reuso
        ou reconstrução de índice. Cache persistente é obrigatório aqui, porque no modo remoto
        reindexar custa dinheiro. Nenhum `AnthropicEmbeddingProvider`: a documentação oficial diz
        que a Anthropic não tem modelo próprio de embeddings.
  - [ ] **4.3E — Integração do Segundo Cérebro.** Planejada. `noteit_context` publicando o canal de
        recuperação como motivo, superfície de CLI, configuração, estado do índice, ranking
        híbrido, explicabilidade, fallback e comportamento na troca de provider. As 16 tools são
        preservadas salvo decisão posterior extremamente justificada.
  - [ ] **4.3R — Auditoria ofensiva da recuperação semântica.** Planejada. Vetor obsoleto, índice
        trocado, provider trocado, modelo trocado dentro do mesmo provider, dimensão errada,
        NaN/Inf, modelo ausente, cache corrompido, symlink no cache, prompt injection, resposta
        remota hostil, erros de API, segredos, concorrência, exaustão de recurso, consulta enorme,
        nota enorme, milhares de notas, queda durante reconstrução, edição durante a recuperação,
        lixeira e restauração, autoridade de escrita e orçamento de resposta do MCP.

Captura e Exportação, OCR e PDF permanecem adiados e não são puxados para a Fase 4.0A ou 4.0B.

**Recência e o CLI.** Desde a Fase 3.8R, "mais recente" é o próprio `updated_at` da nota — a última alteração em seu texto — com o `mtime` do arquivo como substituto para uma nota que não possui nenhum. É o que decide qual nota uma invocação traz de volta quando cada nota é fechada, e por qual ordem de pesquisa e troca rápida. Se uma fase futura precisar de "a nota que abri pela última vez" como distinta de "a nota que escrevi pela última vez", ela pertence a `state.json` como estado explícito, não aos carimbos de data e hora do sistema de arquivos.

## Fase 5: Empacotamento e distribuição (planejada)

Saiu da Fase 4 em vez de ser abandonada: vem depois do trabalho de Core e CLI acima.

- [ ] **Fase 5.0 — TUI completa.** Saiu da Fase 4.0G, que entregou apresentação e não interatividade.
      É aqui que uma interface de terminal de verdade seria decidida: painéis, navegação por teclado,
      seleção de notas, edição dentro do terminal. `noteit tui` é uma proposta, não um comando —
      nenhuma parte dela existe hoje, e a Fase 4.0G deliberadamente não abriu caminho para ela: nenhum
      framework de TUI foi adicionado, nenhum loop de eventos, nenhum prompt persistente.
- [ ] PKGBUILD do Arch Linux para o AUR.
- [ ] Automação de releases e artefatos binários.
- [ ] Versão v0.1.0.
