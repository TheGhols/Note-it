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
- [x] **Fase 4.3 — Recuperação semântica independente de fornecedor.** Liberada pela 4.2R.R1.
      *(Marcação corrigida em 14/09/2026: as subfases 4.3A a 4.3R estavam todas `[x]`
      e a linha-mãe continuou `[ ]`. Era erro de marcação, não de implementação —
      nenhuma entrega foi alterada, nada foi renumerado e nenhum texto das
      subfases foi reescrito.)*
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
  - [x] **4.3B — Motor de recuperação provider-neutral.** Os tipos centrais e o motor, sem nenhum
        provider, sem artefato de modelo, sem rede e **sem dependência nova** — `Cargo.lock`
        byte-idêntico, catálogo ainda em 16 tools. Detalhes e medições na ADR-057.

        **A régua entrou antes da mudança.** Um commit só de testes, verde contra o motor de
        então: os dez cenários que a R1.2 congelou, mais a posição — consulta por consulta — do
        primeiro acerto do corpus, gravada em `docs/retrieval-baseline.json`. O harness reproduziu
        exatamente o baseline publicado pela 4.3A (R@1 0,333 · R@3 0,367 · R@5 0,367 · MRR 0,350),
        que é o que torna o número de depois comparável ao de antes.

        **BM25 dentro do Context Engine, não ao lado dele.** Nenhum segundo motor: um
        `semantic_context.rs` seria um segundo lugar que lê o store, filtra, monta snippet, conta
        tarefas e ordena, e dois desses discordam na primeira semana. Termos são as sequências de
        `[0-9a-z]` sobre a dobra que o `search::fold` já produz — nenhuma segunda definição de
        palavra, nenhum stemming, nenhuma stopword. `k1 = 1.2` e `b = 0.75` como constantes, sem
        ajuste, com a fórmula fixada em teste contra a aritmética escrita à mão.

        ```text
                               R@1     R@3     R@5     MRR
        baseline (pré-BM25)   0,333   0,367   0,367   0,350
        4.3B em Rust          0,633   0,767   0,833   0,711
        protótipo 4.3A        0,667   0,767   0,833   0,728
        ```

        Zero regressões, consulta por consulta. **A única diferença para o protótipo é uma
        consulta, e ela é a garantia funcionando**: em `q08 "sono"`, `n25` casa a frase inteira e
        fica em primeiro, com o ground truth em segundo — onde já estava no baseline. Um BM25 puro
        teria rebaixado `n25`; o motor não pode, porque a classe 1 é ordenada por **sinais
        declarados** e `TermMatch` não é um deles. É isso que torna as classes 2 e 3 estritamente
        aditivas em vez de uma aposta sobre escalas numéricas.

        **O custo, medido e registrado:** sem stopwords, as duas consultas sem resposta do corpus
        passaram de 0 para 22 e 13 candidatos, inteiramente por causa de `de` e `do`. Pontuam quase
        nada e ficam no fim, mas são admitidos. Fica como diagnóstico de precisão, não como motivo
        para mexer em parâmetro.

        **A infraestrutura semântica existe e o produto não a alcança.** `EmbeddingRole` fora do
        `EmbeddingSpaceId`; identidade de artefato como digest de manifesto canônico, com um
        encoder que **recusa** em vez de escapar; chunker versionado por parágrafo com `ChunkId`
        sem concatenação ambígua; `EmbeddingRecord` sem texto de nota; índice em memória por força
        bruta; cosseno que verifica espaço antes de dimensão. Proveniência pela `NoteRevision`
        canônica, da mesma leitura que o D-27 já obriga: vetor obsoleto é descartado e o registro
        esquecido, mudança só de metadado é detectada — `updated_at` não se move e a revisão sim —
        e órfão não ressuscita nada. `RetrievalMode::LexicalOnly` **não tem campo onde um provider
        caiba**, provado por um provider de teste que entra em pânico se for chamado.

        **Desempenho em release, stores sintéticos** (p50, consulta multi-termo): 30 notas
        3,2 ms · 100 notas 11,6 ms · 1 000 notas 129 ms · 5 000 notas 642 ms. O custo é dominado
        pela varredura e pela leitura de cada nota — o preço conhecido de não ter índice (D-04) —
        e não pelo BM25.
  - [x] **4.3C — Provider local.** Implementação concluída; **o fechamento original ficou
        bloqueado pelo orçamento de carga do artefato e foi resolvido pela 4.3C.R1.** O provider
        local escolhido pela 4.3A — embeddings estáticos de token, sem runtime de inferência —,
        com distribuição do artefato, ciclo de vida, custo de CPU e memória medidos em Rust,
        operação offline e indexação incremental. Detalhes e medições na ADR-058 e em
        `docs/semantic-retrieval.md` §26.

        **O que a 4.3C mediu, e o que ela não atendeu.** Doze dos treze orçamentos da §25 passaram
        na primeira medição. Um não passou: `carga do artefato local ≤ 2 s`, medido em
        **2 077–3 475 ms**, com a verificação SHA-256 obrigatória de 489 MiB respondendo por
        83–97% do tempo. A fase fechou `BLOCKED` por esse item — a leitura correta naquele
        momento, contra a implementação de SHA-256 que existia então — e o número **não** foi
        movido para acomodá-lo. É a 4.3C.R1 que o fecha, e o registro histórico continua sendo
        esse.

        **Liberada pela 4.3B, e deliberadamente não fundida com ela.** Fundir teria custado a
        única coisa que importa quando a recuperação semântica responder mal: distinguir bug do
        motor de bug do modelo. Com as fases separadas, a 4.3B pode ser auditada sozinha.

        As questões que a 4.3A deixou abertas eram pré-requisito: qualidade sob quantização,
        licença de `model2vec-rs` e RSS real. **`k1` e `b` não estavam entre elas**: foram
        congelados em 1.2 e 0.75 e a 4.3B os usa exatamente assim. Reabri-los exige três coisas e
        não duas — um conjunto de tuning novo, um conjunto de avaliação separado que não seja
        usado no ajuste, e a decisão explícita de reabrir os parâmetros. O corpus da 4.3A continua
        sendo régua de regressão e não serve para nenhuma das duas primeiras.
    - [x] **4.3C.R1 — Fechamento do provider semântico local.** Orçamento, paridade diagnóstica e
          CI encerrados. Três resíduos objetivos e nenhuma funcionalidade nova; ADR-059 registra a
          decisão corretiva, e a ADR-058 continua representando a decisão da 4.3C.

          **O orçamento, atendido sem ser tocado.** Quatro implementações de SHA-256 foram medidas
          sobre o artefato real, com o mesmo digest saindo de todas: `noteit-core` 197–201 MiB/s,
          `sha2` 0.10 **174–178**, `sha2-asm` 0.6 **187–189**, `ring` 0.17 **324–326**. As duas
          crates puras em Rust são mais lentas que a do próprio Note-it. Com `ring` verificando os
          dois arquivos do artefato, a carga foi certificada em **doze processos independentes** —
          oito com cache de página quente, quatro com ele frio por despejo controlado e verificado
          — e mede **1 375–1 789 ms**, todas dentro dos 2 s, a pior com 211 ms de folga (10,5%). Na mesma
          medição o SHA-256 do Core sozinho custaria 1 854–2 123 ms, que é o orçamento inteiro
          antes de ler um byte: a 4.3C não tinha como caber. `ring` fica **isolada** em
          `noteit-embedding-local`, declarada num único manifesto e usada num único arquivo;
          `NoteRevision` e todo outro digest continuam na implementação do Core, que é
          byte-idêntica à do commit anterior, e `tests/digest_agreement.rs` amarra as duas nos
          vetores do FIPS 180-4, em todo comprimento de 0 a 129 bytes, num megabyte
          pseudoaleatório e no artefato real.

          **Diagnóstico e carregador deixaram de poder discordar.** `noteit status` e o relatório
          da sessão MCP perguntavam `Path::is_file`, que **segue** symlink, enquanto o carregador
          usa `symlink_metadata` e o recusa — um artefato ligado por symlink era anunciado como
          disponível e rejeitado na primeira pergunta. Os dois passaram a compartilhar a mesma
          inspeção, e ela continua custando um `stat` por arquivo.

          **O CI passou a rodar os gates que só rodavam localmente.** `embedding-boundary` e
          `embedding-tests` estavam em `scripts/check` e não no workflow. O estágio `ci-parity`
          reprova o build se qualquer estágio deixar de aparecer em `.github/workflows/ci.yml`;
          `offline` é exceção nomeada e documentada, por exigir namespaces de rede sem privilégio.

          **E uma prova frágil foi fortalecida em vez de silenciada.** `mcp_no_network.rs` afirmava
          a densidade de amostragem do seu monitor por um teto de 1 ms no intervalo **médio** —
          estatística errada, já que a afirmação da suíte é sobre o **pior** intervalo, e teto
          insuficiente, já que sob carga execuções com média de 43–122 µs deixaram de ver um
          socket que provavelmente existia. O controle positivo era o que falhava, quatro
          execuções em seis. Ele virou encontro marcado, o caminho fail-closed repete a recusa até
          o instrumento ver uma, e a densidade passou a ser asserida pelo pior intervalo mais uma
          prova de que o monitor ainda amostrava no fim: **20/20 sob a mesma carga que reprovava
          4 em 6.**

          **Gate para Fase 4.3D: LIBERADO.**
  - [x] **4.3D — Providers remotos opcionais.** OpenAI, Gemini e Voyage, sempre opt-in e sempre
        nomeados pelo usuário, atendidos pelo processo separado `noteit-embed` — o único componente
        do produto com cliente HTTP e o único que vê uma credencial. Decisão, medições e o que
        deliberadamente não foi feito na ADR-060 e em `docs/semantic-retrieval.md` §29.

        **A fronteira foi estendida e não afrouxada, e isso é verificável.** `check-mcp-boundary`,
        `check-core-boundary`, `check-cli-boundary` e `check-embedding-boundary` **não tiveram uma
        linha editada** nesta fase e continuam passando; `check-embed-boundary` é novo e diz a outra
        metade — que o único crate autorizado a ter rede não pode ter store, nota, shell nem forma
        de escrever arquivo. Medido pelo padrão `NETWORK_CRATES` do próprio gate: **8** crates
        de rede no grafo do `noteit-embed` e **0** nos
        do `noteit-mcp` (158), `noteit-core` (40), `noteit-embedding-local` (117),
        `noteit-embedding-remote` (44) e `noteit-embed-protocol` (14). O binário do servidor MCP
        **não cresceu**: ele não linka nada disso.

        **O gate foi testado quebrando o produto.** Dezessete violações injetadas uma a uma; três
        passaram e viraram correções no gate. A mais instrutiva: tirar comentários com `s|//.*||`
        transformava `"https://api.openai.com"` em `"https:`, deixando a regra de endpoint solto
        cega para exatamente o que ela procura.

        **A credencial nunca entra no processo que fala com o agente.** Quem lança o worker monta o
        ambiente do filho com `env_clear()` e uma allowlist de nove variáveis onde nenhum nome de
        credencial aparece; o worker lê a própria chave de `credentials.toml` em modo `0600`.
        Provado lendo `/proc/<pid>/environ` do worker **real**, com `OPENAI_API_KEY`,
        `GEMINI_API_KEY` e `VOYAGE_API_KEY` setadas no processo que o lançou. Nada de chave em
        `argv`. ADR-060 registra também por que o Secret Service **não** foi implementado, em vez de
        omitir a escolha.

        **SSRF fechado por construção.** O protocolo carrega um enum de três variantes e nunca uma
        URL; os três endpoints são constantes `https` num único arquivo. O caminho que sobrava — o
        Gemini põe o modelo na URL — está fechado por um alfabeto estreito verificado duas vezes.
        Zero redirects, com o teste exigindo que o destino de um `Location` receba **zero**
        requisições; nenhum proxy, e `ALL_PROXY`/`HTTPS_PROXY`/`HTTP_PROXY` não são consultadas.

        **Cache remoto obrigatório, porque reindexar remoto custa dinheiro.** Formato versionado com
        digest SHA-256 sobre o arquivo inteiro, escrita atômica pela mesma `write_atomic` das notas,
        `0600`, sem texto de nota. Medido: indexação a frio de 4 notas custa **5 requisições**; a
        **segunda sessão com cache válido custa 1** — só a consulta. Uma edição reenvia a nota
        editada e mais nada. Em release, com dim 1 536 e dois chunks por nota: mil notas são
        12,1 MiB com `save` de 69 ms e `load` de 68 ms; dez mil são 121,3 MiB com 665 ms e 726 ms.
        Spawn do worker até "pronto": mediana de 2 ms. Round trip AF_UNIX: p50 0,16–0,19 ms, e
        **igual para 1, 16 e 64 textos** — o protocolo não acrescenta nada mensurável ao lado da
        latência de rede.

        **Um defeito real foi encontrado por um teste, não por revisão.** O worker recusava
        corretamente ligar sobre um arquivo regular no caminho do socket, e então o *spawner*
        apagava o arquivo na limpeza. Corrigido: este processo não remove o que não criou.

        **O MCP não mudou de papel.** As 16 tools continuam 16, `semantic_match` continua motivo e
        não número, e a resposta continua sem vetor, sem `source_revision`, sem caminho de cache,
        sem caminho de socket, sem request ID e sem corpo de erro do fornecedor. `NoteRevision` e
        `hashing.rs` são byte-idênticos à baseline.

        **O que não foi medido, e é dito:** latência real de rede. Nenhum teste obrigatório fala com
        um fornecedor, e a linha "consulta com provider remoto — a medir" da §25 continua a medir.
        Propor um teto a partir de um mock seria derivar um limite do resultado obtido.

        **Gate para Fase 4.3E: LIBERADO.**
  - [x] **4.3E — Integração do Segundo Cérebro.** Integração ponta a ponta do sistema de recuperação contextual unindo Core, CLI e MCP como um produto único e coerente. **`noteit-core::context` e `noteit-core::semantic` permanecem a autoridade única e soberana**: toda leitura, validação de `source_revision`, pontuação BM25, ranqueamento híbrido, explicabilidade de motivos e políticas de degradação pertencem exclusivamente ao Core. CLI e MCP são estritamente consumidores de apresentação, sem reinventar a recuperação e sem duplicar lógica de sincronização.

        **O Core como autoridade de sincronização.** `noteit_core::semantic::synchronise` foi extraído e tornado público, unificando a sincronização incremental dos índices em memória sob a regra imutável: *indexa o que o índice não tem, esquece o que o store não tem mais*. O servidor `noteit-mcp` passou a consumi-lo diretamente, eliminando a duplicação entre processos e garantindo que o ciclo de vida vetorial e a invalidação por revisão canônica sejam idênticos em toda a plataforma.

        **Superfície de CLI e paridade total com o MCP.** O Note-it CLI ganha o comando `contexto` com alias bilíngue internacional `context` (`noteit contexto [consulta] [--limite N] [--tag TAG] [--propriedade CHAVE=VALOR] [--tarefas ESTADO]`). A apresentação humana entrega UUID truncado em 8 caracteres, títulos em negrito, snippets higienizados contra escape ANSI, motivos explicáveis em português (`texto`, `termos`, `tag`, `propriedade`, `tarefa`, `semântico`, `recente`) e tarefas formatadas legivelmente. Sob `--json`, a saída emite o envelope estrito da máquina sob `data.candidates`, onde a garantia por tipo proíbe qualquer vazamento interno: **zero ocorrências de `revision`, `etag`, `score`, `similarity`, `vector`, `embedding` ou `path`**. A paridade rigorosa entre `noteit --json contexto` e o tool `noteit_context` foi provada pelo teste de integração real sobre stdio `parity_between_cli_json_and_mcp_context` em `noteit-mcp/tests/mcp_context.rs`, conferindo campo a campo sob o mesmo store.

        **A régua de regressão e as fronteiras permanecem intactas.** A avaliação contra o corpus congelado (`cargo test -p noteit-core --test retrieval_corpus`) atesta preservação absoluta dos parâmetros BM25 (`k1 = 1.2`, `b = 0.75`) e das métricas de referência: **R@1 = 0,633, R@3 = 0,767, R@5 = 0,833, MRR = 0,711**, com **35 candidatos sem resposta** e **zero demotions**. O catálogo público do MCP foi mantido rigorosamente em **exatamente 16 tools**. As fronteiras arquiteturais continuam passando sem exceção (`check-core-boundary`, `check-cli-boundary`, `check-mcp-boundary`, `check-embedding-boundary` e `check-embed-boundary`). O armazenamento real do usuário permaneceu intocado, com o digest SHA-256 verificado antes e depois (`3d4e8773926342d14aca041daca70326978df6d99794b00917c1b32edca12aa9`).

        Fechamento documentado no commit `1026fed`, com CI remoto integralmente verde no run #111 (Rust 7m56s, Frontend 2m1s).

        **Gate para Fase 4.3R: LIBERADO.**
  - [x] **4.3R — Auditoria ofensiva da recuperação semântica.** Auditoria adversária e testes de estresse sistemáticos executados contra as cinco superfícies do motor de recuperação contextual e semântica (Core, Local Embedding, Remote Embedding / Protocolo, CLI e MCP), sem adição de funcionalidades especulativas e comprovando a resiliência estrita da arquitetura construída.

        **Superfície 1 — Integridade de artefato e concorrência local (`noteit-embedding-local`).** Prova de integridade criptográfica SHA-256 e recusa estrita de artefatos hostis antes de qualquer tentativa de interpretação/parse de tensores: bytes truncados ou com inversão de bits no meio do arquivo safetensors (`weights_with_flipped_bytes_in_the_middle_are_refused`) resultam infalivelmente em `ArtifactError::Unexpected`. Apenas arquivos regulares são admitidos, recusando diretórios e links simbólicos (`ArtifactError::NotARegularFile`). Carga concorrente por múltiplas threads simultâneas sobre o mesmo arquivo de pesos (`concurrent_threads_loading_the_same_artifact_simultaneously`) foi provada sem condições de corrida ou corrupção de memória.

        **Superfície 2 — Protocolo, limites e isolamento remoto (`noteit-embed` / `noteit-embedding-remote` / `noteit-embed-protocol`).** A fronteira de isolamento de rede foi auditada e testada contra anomalias severas no canal AF_UNIX: respostas do worker com dimensão divergente da acordada pelo espaço (`a_worker_returning_wrong_dimension_is_handled_cleanly`) são bloqueadas com segurança na fronteira, degradando para busca léxica sob a política `Automatic` (`semantic_status: unavailable`) e recusando com erro tipado sob `SemanticRequired`. Respostas com floats anômalos (NaN, Infinito ou vetores vazios / `WireError::InvalidResponse`) degradam com proteção idêntica. Testes de injeção de SSRF e credenciais no corpo de notas (`hostile_ssrf_and_credential_payloads_in_note_content_remain_inert` com endpoints de metadados da AWS/GCP e localhost) comprovaram que o conteúdo textual trafega como dado opaco em quadros com comprimento prefixado no socket Unix, sem disparar qualquer requisição de rede out-of-band e sem vazar caminhos, descritores ou sockets na resposta.

        **Superfície 3 — Motor de admissão e dominância de precedência (`noteit-core`).** Testes de recheio hostil de palavras-chave (*keyword stuffing* com 50.000 a 100.000 repetições de termos de busca) atestaram a imutabilidade das invariantes de ranqueamento: sinais declarados de Classe 1 (frase exata, tag compartilhada, propriedade compartilhada, tarefa correspondente) dominam rigorosamente a correspondência por termos de Classe 2 e a similaridade vetorial de Classe 3, impossibilitando que recheio massivo ultrapasse notas com casamento exato ou filtros explícitos. Entradas com Unicode hostil (RTL overrides `\u{202E}`, ligaturas ZWJ `\u{200D}`, múltiplos acentos combinantes) são preservadas e limitadas sem quebras de layout.

        **Superfície 4 — Blindagem da interface CLI (`noteit-cli contexto`).** Sob um store danificado contendo 2.000 symlinks corrompidos, a saída da CLI provou teto estrito de avisos (`warnings.len() <= 20`, `omitted_warning_count >= 1980`, `warnings_truncated: true`) sem vazar nenhum caminho absoluto de arquivo ou diretório. Parâmetros extremos (`--limite 99999999` e `--limite 0`) são normalizados com segurança para os limites canônicos (`1..=50`), limites não numéricos ou negativos são rejeitados com código 2 (erro de uso do Clap), e consultas gigantescas (> 512 caracteres) são rejeitadas pelo Core com código 1 sem ecoar o conteúdo confidencial da consulta. Varredura recursiva completa na serialização JSON atesta a ausência irrestrita de campos proibidos: zero ocorrências de `revision`, `etag`, `score`, `similarity`, `vector`, `embedding` ou `path`.

        **Superfície 5 — Paridade adversária CLI / MCP (`parity_between_cli_json_and_mcp_context`).** O teste `adversarial_parity_between_cli_json_and_mcp_context` em `noteit-mcp/tests/mcp_context.rs` submeteu a CLI (`--json contexto`) e o MCP (`noteit_context`) simultaneamente a stores hostis (notas com 50k repetições, Unicode degradado, URLs de SSRF e symlinks quebrados). A concordância foi comprovada candidato por candidato, motivo por motivo, snippet por snippet, aviso por aviso e em todos os sinalizadores de truncamento.

        **Régua e integridade preservadas sem defeitos.** Nenhum código produtivo precisou ser alterado (zero defeitos reproduzíveis). A régua congelada (`cargo test -p noteit-core --test retrieval_corpus`) mantém métricas exatas: **R@1 = 0,633, R@3 = 0,767, R@5 = 0,833, MRR = 0,711**, com **35 candidatos sem resposta** e **zero demotions**. BM25 congelado em `k1 = 1.2` e `b = 0.75`. Catálogo MCP fixado em **16 tools**. As 5 checagens de fronteira passam ilesas (`check-core-boundary`, `check-cli-boundary`, `check-mcp-boundary`, `check-embedding-boundary`, `check-embed-boundary`). O armazenamento real do usuário permaneceu estritamente intocado, com fingerprint SHA-256 verificado antes e depois (`3d4e8773926342d14aca041daca70326978df6d99794b00917c1b32edca12aa9`).

        Fechamento documentado no commit `8c26d43`, com CI remoto integralmente verde no run #113 (Rust 8m12s, Frontend 48s).

        **Gate para Fase 5.0: LIBERADO.**

Captura e Exportação, OCR e PDF permanecem adiados e não são puxados para a Fase 4.0A ou 4.0B.

**Recência e o CLI.** Desde a Fase 3.8R, "mais recente" é o próprio `updated_at` da nota — a última alteração em seu texto — com o `mtime` do arquivo como substituto para uma nota que não possui nenhum. É o que decide qual nota uma invocação traz de volta quando cada nota é fechada, e por qual ordem de pesquisa e troca rápida. Se uma fase futura precisar de "a nota que abri pela última vez" como distinta de "a nota que escrevi pela última vez", ela pertence a `state.json` como estado explícito, não aos carimbos de data e hora do sistema de arquivos.

## Fase 5: TUI, acabamento, empacotamento e distribuição

Saiu da Fase 4 em vez de ser abandonada: vem depois do trabalho de Core, CLI, MCP e Recuperação Semântica. O empacotamento permanece como o último marco: a Fase 5.0E somente começa depois que a experiência da TUI estiver acabada e validada com notas reais.

- [x] **Fase 5.0A — Arquitetura e escopo (TUI + Empacotamento).** Primeira fase dedicada à especificação formal da interface interativa de terminal e do empacotamento para distribuição. Seguindo o padrão de medição antes de implementação estabelecido na Fase 4.3A: nenhum `.rs` de produção ou teste foi escrito, nenhuma dependência foi adicionada e o `Cargo.lock` permaneceu byte-idêntico. Todas as decisões foram mensuradas em protótipos descartáveis e documentadas em `docs/tui.md`:
      - **Runtime e framework de TUI medidos com números:** Avaliação comparativa entre `crossterm` puro (v0.29.0), `ratatui` (backend `crossterm`, v0.30.2) e `cursive` (backend `crossterm`, v0.21.1) em perfil release. `crossterm` puro possui menor footprint (32 deps, 464 KiB stripped, 7,23 s compilação), mas não oferece motor de layout ou widgets. `cursive` possui modelo de visualização retida com callbacks e gerou compilação de 35,56 s e binário stripped de 811 KiB. `ratatui` entrega modelo imediato declarativo (`Terminal::draw`), compila em 20,23 s (43% mais rápido que cursive), gera binário stripped de apenas 636 KiB (+172 KiB sobre crossterm puro) e opera em loop de eventos síncrono com zero runtime assíncrono (sem `tokio`). Varredura estrita confirmou zero dependências de rede (zero HTTP/TLS) e zero dependências gráficas (zero GTK/WebKit/Wayland). Decisão: **`ratatui` com backend `crossterm`**.
      - **Isolamento de crate e binário (`noteit-tui`):** A TUI residirá no crate `noteit-tui`, produzindo o executável independente `target/release/noteit-tui`, e **não** como subcomando de `noteit-cli`. A CLI mantém seu papel como despachante batch rápido (0 ms de loop, impressão determinística e saída imediata), sem inflar seu binário com as 85 dependências de TUI. O novo gate `scripts/check-tui-boundary` garantirá de forma mecânica no CI a ausência de bibliotecas de desktop, ausência de rede e proibição de manipulação direta de arquivos do store sem passar pelo Core.
      - **Superfície de dados como invariante inegociável:** A TUI vincula-se diretamente como cliente in-process de `noteit-core` (`open_read_only` para leituras e `authority::perform_at` para gravações). Invariantes absolutos: a TUI **nunca** invoca `noteit` ou `note-it` como subprocessos, **nunca** realiza parsing de saídas textuais ou JSON da CLI, **nunca** faz bypass de I/O em disco fora do Core, e **nunca** duplica análise de front matter, regexes de tarefas ou índices de busca.
      - **Modelo de concorrência com a GUI desktop:** O modelo de retenção perpétua do writer lease consultivo (`flock`) pela TUI foi rejeitado — ele impediria a GUI desktop de abrir (violando ADR-039) ou forçaria a TUI a atuar como servidor IPC daemon. A TUI opera como **cliente transacional** via `noteit_core::authority::perform_at`, requisitando o lease pontualmente a cada mutação (ou encaminhando ao desktop via soquete Unix privado se o desktop estiver ativo). Concorrência otimista garantida por `expected_revision` (`NoteRevision`, ADR-045): se uma nota for alterada externamente na GUI ou CLI enquanto exibida na TUI, a mutação falha com `WriteError::RevisionConflict`, impedindo perda silenciosa de dados.
      - **Auditoria do empacotamento existente (`packaging/arch/`):** A execução de `makepkg` sobre `packaging/arch/PKGBUILD` revelou três bloqueadores: (1) falha imediata com HTTP 404 ao baixar o tarball de origem da tag `v0.1.0` inexistente; (2) falha no `makedepends` caso `pnpm` não esteja instalado como pacote de sistema do Arch; (3) a função `package()` instala unicamente o binário GUI `note-it` e os artefatos de `ui/dist/`, omitindo por completo `noteit` (CLI), `noteit-mcp` (MCP), `noteit-embed` (embeddings remotos) e a futura `noteit-tui`.
      - **Deliberadamente fora de escopo quando a Fase 5.0A foi decidida:** Editor de texto embutido completo no terminal (a TUI delegaria edição de corpo extenso ao `$EDITOR` externo com suspensão/restauração e validação de revisão), modo daemon de background para TUI, renderização de imagens por protocolos gráficos de terminal (Sixel/Kitty), empacotamento para distribuições não-Arch, e sincronização remota ou em nuvem. **A exclusão histórica do editor embutido foi posteriormente revogada pela decisão de produto registrada na Fase 5.0D.2; os demais limites continuam válidos.**

- [x] **Fase 5.0B — Crate `noteit-tui`, Shell de Terminal e Portão de Fronteira.** Conclusão do esqueleto fundamental da interface interativa de terminal (`noteit-tui`) e do portão mecânico de fronteira:
      - **Crate dedicado no workspace:** `noteit-tui/` adicionado aos membros do `Cargo.toml` raiz, produzindo o binário `target/release/noteit-tui`. Dependências normais estritas: `ratatui` (0.30.2 com backend `crossterm`), `crossterm` (0.29.0), `signal-hook` (0.3.18) e `noteit-core`. Zero dependências de rede, zero bibliotecas de interface desktop e zero runtime assíncrono (sem `tokio`).
      - **Guarda RAII e restauração determinística:** Implementação de `TerminalGuard` com trait `Drop` para restauração garantida de cooked mode (`disable_raw_mode()`), saída de alternate screen (`LeaveAlternateScreen`), cursor visível (`Show`) e flush de buffer sob qualquer caminho de saída.
      - **Hook de pânico blindado:** `install_panic_hook` restaura o terminal para cooked mode e tela padrão antes de invocar o hook nativo do Rust, prevenindo corrupção do terminal do usuário em caso de panic.
      - **Tratamento de sinais POSIX:** Registro seguro de `SIGINT` e `SIGTERM` via `signal-hook` atualizando flag atômica síncrona inspecionada em loop de eventos com bounded poll de 50 ms. Saída limpa por `q`, `Esc`, `Ctrl+C` e sinais de encerramento do sistema operacional.
      - **Redimensionamento responsivo:** Captura de `Event::Resize` com `terminal.autoresize()`, renderização com restrições proporcionais e guardas de dimensão mínima prevenindo falhas de layout em dimensões degeneradas.
      - **Validações de pré-voo fail-fast:** Verificação de existência do store (`paths.notes_dir.exists()`) e validação de TTY (`io::stdout().is_terminal()`) antes de entrar em raw mode; falha com código 1 e mensagem explicativa em stderr sem corromper o terminal.
      - **Portão de fronteira mecânico (`scripts/check-tui-boundary`):** Integrado a `scripts/check` (`tui-boundary`) e ao workflow do GitHub Actions, impedindo compilação com crates de GUI desktop, crates de rede e bypass de I/O no store.
      - **Suíte de testes de pseudoterminal (`tests/terminal_lifecycle.rs`):** 9 testes de integração automatizados em pseudoterminal Unix (`openpty` via `libc`) verificando ativação/desativação de raw mode via `termios` (`ICANON`, `ECHO`), redimensionamento, sinais, panic hook, stores ausentes e rejeição de não-TTY.

- [x] **Fase 5.0C — Modo Leitura, Navegação e Apresentação.** Implementação da experiência completa de navegação por teclado e visualização de Markdown somente-leitura, consumindo `noteit-core` in-process:
      - **Painéis de navegação por teclado:** `RecentNotes` (ordenado estritamente por `updated_at` canônico decrescente via `core.list_summaries`, sem mtime como critério primário), `PendingTasks` (reaproveitando o extrator de tarefas do Core via `core.list_tasks(Pending)` sem duplicar regex), e `Trash` (listagem somente-leitura de `core.list_trash()`). Alternância de painéis via `Tab`, `BackTab` e teclas numéricas `1`, `2`, `3`; navegação vertical por setas e `j`/`k`; foco no leitor via `Enter`/`l`.
      - **Busca rápida via `/`:** Consulta em tempo real delegada diretamente para `core.search_notes(&query)`, reaproveitando sem duplicação a tokenização, o `search::fold`, contagem de correspondências e ordenação por relevância/recência do Core. Resultados navegáveis por setas e seleção por `Enter`.
      - **Renderização formatada de Markdown (`markdown.rs`):** Títulos (níveis 1 a 6), listas com marcadores (`• `) e numeradas, checkboxes visuais (`☐`/`☑` com remoção automática de comentários de conclusão), citações em bloco (`│ `) e todos os 5 alertas GFM da Fase 3.5 com rótulos visíveis destacados (`[NOTA]`, `[DICA]`, `[IMPORTANTE]`, `[ATENÇÃO]`, `[CUIDADO]`). Blocos de código monoespaçados com linguagem declarada e sem syntax highlighting. Expressões matemáticas renderizadas como texto cru sem avaliação.
      - **Testes automatizados com `TestBackend`:** 7 novos testes de integração em `tests/navigation_and_rendering.rs` validando ordenação por recência, atalhos de painéis, extração de tarefas, busca delegada ao Core e asserções diretas do buffer de tela do Ratatui. Total de 21 testes na suíte `noteit-tui`.

- [x] **Fase 5.0D.R0 — Fechamento do Contrato de Mutação Concorrente.** Contrato fechado: criação com publicação atômica condicionada à ausência, descarte/restauração condicionados por revision, ciclo de descarte compartilhado com o receiver desktop e protocolo privado v3. Gates locais aprovados e [CI da implementação verde](https://github.com/TheGhols/Note-it/actions/runs/34210340965), commit `fc6d39a765ffe00ef038c467a64106aaeec87e06`. Detalhes, contagens e provas em `docs/tui.md`, seção 10. R0 posteriormente revisada e aprovada para iniciar a 5.0D sobre a baseline `2fc1eecf1073321de5a18ab3ba956c2e94ff6008`.
- [x] **Fase 5.0D — Modo Edição, Mutação e Concorrência Transacional.** O fechamento em `00140b7c95565d1424a678282c068bc25c8a460a`, após a implementação `0ce2825262f28c40c8763f83b702a1368899dd37` e [CI verde](https://github.com/TheGhols/Note-it/actions/runs/34284207606), foi prematuro: um `$EDITOR` que deixava o terminal em raw contaminava a restauração final. A fase ficou BLOCKED até a revisão `7196a2f0f0007d01967d02a8d7f5795822c3200f`, que fixa T0 como snapshot termios imutável, adiciona a regressão PTY real e passou no [CI corretivo](https://github.com/TheGhols/Note-it/actions/runs/34348671971). O histórico completo — bloqueio, correção e prova — está em `docs/tui.md`, seção 12. Core/R0 permanece fechado. **5.0E não iniciada.**
- [x] **Fase 5.0D.1 — Fidelidade de Markdown e compatibilidade com notas da GUI.** Corrigir o leitor antes de ampliar a edição. O renderer deverá consumir com segurança o subconjunto de HTML que o próprio editor gráfico persiste, sem imprimir marcação na tela e sem interpretar HTML arbitrário: `span[data-note-it-color]`/`style=color`, `mark[data-note-it-highlight]`/`background-color`, `<u>`, comentários HTML e entidades como `&nbsp;`. Cor de texto e marca-texto devem compor entre si e com negrito, itálico e riscado, inclusive dentro de títulos, listas, tarefas, citações e alertas. Os delimitadores reconhecidos (`**`, `*`, `_`, `~~`, crases e tags admitidas) não podem vazar para a apresentação; conteúdo desconhecido deve permanecer texto seguro, nunca ser executado. H1 a H6 terão hierarquia cromática perceptível — não apenas H1 a H3 — respeitando contraste e terminais com paleta limitada. A fase inclui fixtures reais anonimizadas dos casos observados, testes unitários do parser e snapshots `TestBackend` que proíbem vazamento de `<span`, `<mark`, `<u>`, `<!--`, `&nbsp;` e marcadores já reconhecidos. Concluída: o reconhecimento de bloco (`markdown.rs`) foi separado de uma varredura inline com pilha de estilos (`inline.rs`), sem nenhuma dependência nova e com `Cargo.lock` byte-idêntico. Nenhum `<span>`, `<mark>`, `<u>`, comentário, entidade ou delimitador reconhecido alcança a tela; H1–H6 ganharam seis cores próprias mais modificadores; o painel de tarefas e a prévia da lixeira passaram a usar o mesmo renderer. Os 27 testes de regressão foram escritos primeiro e falharam 24 vezes na baseline `32cbda6`; a suíte final tem 30 testes de fidelidade e 85 no crate, com prova de foreground, background e modifiers em `Span`s e células do `TestBackend`, além de captura em pseudoterminal real. Gates locais aprovados, incluindo `scripts/check rust` completo; store real verificado por fingerprint idêntico antes e depois. Detalhes, contagens e provas em `docs/tui.md`, seção 13. **5.0D.2, 5.0D.3 e 5.0E não iniciadas.**
- [x] **Fase 5.0D.2 — Editor nativo no painel direito.** Substituir o `$EDITOR` como fluxo principal por edição embutida na própria TUI. Ao abrir/selecionar uma nota para trabalhar, o painel direito entra em modo de edição direta sem exigir uma segunda tecla `e`; foco, cursor e estado de leitura/edição devem permanecer inequívocos, com atalhos visíveis e operação integral por teclado. O editor v1 cobre inserção e remoção Unicode/multilinha, navegação, seleção, quebra de linha, rolagem, desfazer/refazer com limite explícito, salvar e cancelar. Salvamento continua obrigatoriamente transacional pelo Core com a `revision` originalmente lida: conflito externo nunca sobrescreve conteúdo e oferece releitura, descarte consciente ou preservação recuperável do rascunho. Sair, trocar de nota/painel, buscar, descartar ou receber sinal com alterações pendentes exige confirmação e não pode perder texto silenciosamente. O `$EDITOR` externo pode permanecer como ação alternativa, não como requisito para a edição cotidiana. Testes com `TestBackend` e PTY real devem cobrir acentos/Unicode, colagem multilinha, terminais estreitos, conflito de revisão, cancelamento, recuperação e restauração exata do terminal. Concluída: `Enter` sobre uma nota — na lista, na busca ou no painel de tarefas — abre o painel direito editando, sem uma segunda tecla; selecionar com as setas continua pré-visualizando a leitura renderizada da 5.0D.1, e `Esc` desce do editor para a leitura e da leitura para a lista. O modelo do editor vive em `noteit-tui/src/draft.rs`, indexado por caractere e sem nenhuma dependência nova (`Cargo.lock` byte-idêntico). `Ctrl+S` grava por `authority::perform_at` com a `revision` lida ao abrir o painel e com `editor::mutation_for`, a mesma função do `$EDITOR`; conflito não sobrescreve nada e oferece preservar o rascunho, reler a nota ou mantê-lo no editor. Um rascunho pendente só existe com o foco no editor, então trocar de nota/painel, buscar e descartar são inalcançáveis com pendências — `Esc` e o `Ctrl+C` de teclado fazem a mesma pergunta — salvar, descartar conscientemente ou continuar editando — e os sinais externos `SIGINT`, `SIGTERM` e `SIGHUP`, que não têm a quem perguntar, preservam o texto em `tui-recovery` informando o caminho. O `$EDITOR` externo segue disponível na leitura. Os 31 testes de regressão foram escritos primeiro e falharam 29 vezes na baseline `86290a0`; a suíte final tem 45 testes de editor nativo e 144 no crate, com prova em `TestBackend` e em PTY real (acentuação, colagem multilinha, terminal estreito, redimensionamento, conflito, cancelamento, recuperação e restauração exata do terminal). Gates locais aprovados, incluindo `scripts/check rust` completo; store real verificado por fingerprint idêntico antes e depois. Detalhes, contagens e provas em `docs/tui.md`, seção 14. **5.0D.3 e 5.0E não iniciadas.**
- [x] **Fase 5.0D.3 — Polimento visual, responsividade e fluxo de trabalho.** Revisão integral da experiência mostrada pela TUI real: cabeçalho sem rótulo de fase de desenvolvimento obsoleto; metadados, títulos e tags com quebra/adaptação sem colisão; listas e tarefas longas sem corte silencioso; Markdown e código com wrapping e rolagem previsíveis; indicador visível de posição/mais conteúdo; seleção legível sem apagar as cores semânticas; painel de tarefas sem expor `**` e outros marcadores; estados vazios, avisos e confirmações consistentes; atalhos do rodapé adequados ao foco e à largura disponível. A aceitação exige matriz de dimensões pequenas, médias e largas, snapshots do buffer e uma rodada manual nas mesmas classes de notas usadas no diagnóstico visual. Concluída com navegação por mouse e autoria de cor/marca-texto explicitamente autorizadas: alvos semânticos derivados do layout, captura restaurada pelo guard do terminal, proteção de rascunho idêntica à navegação por teclado e paletas/markup canônicos da GUI, sem dependência ou alteração no Core. A correção 5.0D.3.R1 acrescenta estilo transitório para digitação futura com indicação visível e reset independente, rodapé contextual responsivo entre 40 e 140 colunas, limpeza determinística de avisos e checkbox de tarefa por `Space` ou alvo semântico do mouse; tarefas renderizadas não expõem markup e todas as mutações continuam revisionadas pela autoridade do Core. Evidências e limitações estão em `docs/tui.md`, seção 15. **5.0E não iniciada.**
- [x] **Fase 5.0D.4A — Arquitetura do Editor Visual.** Gate exclusivamente arquitetural para definir um modelo seguro e sem perdas de edição visual de Markdown/markup Note-it: mapeamento bidirecional fonte↔visual; semântica de cursor, seleção e mutações; preservação integral de sintaxe desconhecida ou malformada; e interação entre os modos Visual e Markdown fonte. A persistência canônica do Core permanece inalterada, salvo decisão arquitetural separadamente aprovada que prove outra necessidade. Concluída após **quatro rodadas e três revisões adversariais independentes**, registradas integralmente em `docs/tui.md`: o relatório original (seções 1–25), a correção normativa R1 (§26), a correção final R2, a correção R3 (§27) e o fechamento R4 (§28). A primeira revisão independente bloqueou o desenho com 3 BLOCKERs e 21 MAJORs; a segunda confirmou dois blockers fechados, manteve um aberto e apontou oito defeitos introduzidos pela própria correção; a terceira aprovou com dois ajustes de uma frase, ambos aplicados. Nenhuma rodada aprovou a si mesma. A arquitetura resultante fixa: `Lexeme`/`Node`/`ProjectionRun` separados, cobertura byte-exact da fonte, `Generation` monotônica por sessão, precedência de código/fence antes de qualquer candidato HTML, escopo de bloco na álgebra de seleção, três eixos ortogonais de classificação, envelope de reescrita declarado, e 28 propriedades verificáveis por máquina. **Nenhuma implementação de produção do editor visual foi autorizada antes desta aprovação.**
- [x] **Fase 5.0D.4B — Implementação do Editor Visual.** Os catorze portões da sequência obrigatória do §26.14 passaram na ordem, cada um com gate verde antes do seguinte: B.1 (projeção lossless), P0 (baseline), B.2 (source map somente leitura), P1 (gate pré-interativo), B.3 (edição mínima), B.4 (headings e fronteiras de bloco), P2 (gate pré-inline), B.5 (capacidades inline), P3 (gate pré-HTML), B.6 (HTML canônico), P4 (gate pré-blocos), B.7 (blocos estruturados), B.P (fechamento de performance) e B.R (fechamento adversarial). O editor está ligado à aplicação: `Alt+V` alterna entre Markdown e Visual, a nota sempre abre em Markdown, negrito parece negrito, cor é cor, tarefas mostram caixa, e tudo o que o gate não sabe editar continua visível como fonte e recusa toda edição com aviso nomeando a causa. A suíte da TUI foi de 165 para 426 testes. Os gates de performance encontraram **cinco** custos superlineares que nenhuma revisão de código teria visto — a maior delas fazia um `VisualDocument` de 200 KB levar 598 ms — e o custo de uma tecla caiu de 31,7 ms para 2,5 ms numa nota de 64 KB. Detalhes e medidas em `docs/tui.md`, seções 29 a 41. **5.0E não iniciada.**
- [x] **Fase 5.0D.5 — Paridade Semântica: Matemática e Flashcards.** A implementação canônica foi auditada antes de qualquer código e o motor foi **portado**, não reinterpretado: mesmo lexer, mesma gramática, mesma tabela de unidades com os mesmos fatores exatos, mesmos sete códigos de erro com as mesmas palavras, mesma formatação pt-BR. Flashcards leem a **projeção lossless** da 5.0D.4B, que é o equivalente terminal do documento ProseMirror que o editor gráfico lê — e é o que permite saber que um `::` dentro de fence, de código inline ou de destino de link não é cartão. A paridade é provada por duas fixtures geradas da implementação canônica e afirmadas pelos **dois** lados em suas próprias suítes, não por comparar as implementações entre si. A única divergência encontrada foi a forma exponencial acima de 1e21, exatamente o tipo de diferença que uma reimplementação "parecida" deixaria passar. Detalhes em `docs/tui.md`, seção 42. **5.0E não iniciada.**
- [x] **Fase 5.0D.R5 — Correção comportamental do editor Visual.** Nove commits, de `59b2d99` a `aa082c8`, endereçando o que o editor Visual da 5.0D.4B fazia errado em uso real: caret único e quebras de linha, nota em branco editável no Visual, digitação com estilo sem partir marcações, `mark_path` por bisseção em vez de varredura de nós, ida e volta CLI → Visual → CLI, fechamento adversarial, e a sessão isolada informando que continua ativa. **A fase passou nos testes automatizados e falhou no reteste manual** — o registro dessa falha e o que ela ensinou estão em `docs/tui.md`, seção 44, escritos pela R6. Inclui também `e1143a7` e `aa082c8`, que acrescentam o botão de atalhos na barra da nota e separam comando de frase nesse painel — a mesma superfície que a 5.0E-GUI viria a corrigir.
- [x] **Fase 5.0D.R6 — Fechamento comportamental do editor Visual.** Treze commits, de `9c08642` a `16a182e`. A R5 tinha passado nos testes e falhado no reteste manual, e a R6 foi atrás de exatamente por onde os defeitos passaram: layout do editor Visual em linhas desenhadas, o cursor visual mandando no viewport visual, o caret sobrevivendo à releitura da nota, setas/páginas/seleção total andando por linha visual, formatação e Enter estilizado no editor certo, o Visual como editor padrão com a fonte se identificando, o caret no vão de uma quebra por grafema largo, o quadro do editor Visual deixando de crescer com a nota, canários de tela, a sequência manual num terminal de verdade, revisões adversariais e o contrato definitivo de `Alt+F` no modo Markdown/Fonte. Relatório completo, medidas e limitações em `docs/tui.md`, seção 44.
- [x] **Fase 5.0E-GUI — Ativação a frio e o pacote diário.** Recorte da 5.0E entregue antes dela, pela razão que o próprio `packaging/arch/PKGBUILD` registra: o usuário precisava de uma versão gráfica instalável para uso diário, e não havia tag `v0.1.0` para fixar. `159fe2d` corrige a ativação a frio pelo barramento de sessão — a ligação global `gapplication action io.github.theghols.NoteIt toggle-layer` só funcionava enquanto o Note-it já estivesse rodando, porque o barramento não tinha como iniciá-lo; o arquivo `resources/io.github.theghols.NoteIt.service` fecha isso, e `DBusActivatable=true` foi deliberadamente **não** posto no `.desktop`, cujas ações são linhas `Exec=` e não GActions registradas. `a59864a` e `ef035ad` fixam o pacote nesse commit e fecham ícone, checagem e lint. O pacote resultante instala quatro binários — `note-it`, `noteit`, `noteit-mcp`, `noteit-tui` —, roda `scripts/check` como `check()` e escreve exclusivamente dentro de `$pkgdir`, sem tocar `~/.local/share/note-it`, `~/.config/note-it` ou `~/.local/state/note-it`. `50b40a5` acrescenta a correção do painel de atalhos, que era um defeito visível na GUI diária. **Esta fase não fecha a 5.0E**, que continua devendo os cinco binários, o `PKGBUILD-git`, o chroot limpo, a tag e o AUR.
- [x] **Fase 5.1A — A ponte para um cliente de IA externo.** `a7eddfc`. O crate `noteit-agent-bridge` inicia um cliente de IA de linha de comando que a pessoa já instalou e já autenticou — Claude Code, Gemini CLI, Codex — como subprocesso oculto, e converte o que ele transmite em eventos tipados que uma interface gráfica pode desenhar como conversa. Deliberadamente **não** é cliente de modelo (sem HTTP, sem SDK, sem chave, sem token), **não** é gravador (não depende de `noteit-core`, e é esse o ponto: um componente que não consegue ligar o store não pode ser dono dele — ler e gravar continua sendo pelo `noteit-mcp`, com `expected_revision` e a autoridade de escrita), e **não** é terminal (nenhum pseudoterminal, nenhum ANSI, nenhum spinner raspado). Cada adaptador dirige seu cliente num modo headless documentado que emite um JSON por linha; um cliente sem modo não interativo estruturado é recusado pelo nome, com a razão. `scripts/check-agent-bridge-boundary` é o gate mecânico dos três limites. Especificação em `docs/second-brain.md`. **Nenhum consumidor gráfico foi construído**: a ponte existe como biblioteca e a superfície que a usaria não pertence a esta fase.
- [ ] **Fase 5.0E — Empacotamento Completo, Auditoria e Distribuição (fase final).** Só começa depois de 5.0D.1–D.5 concluídas. Atualização de `packaging/arch/PKGBUILD` contemplando os 5 binários executáveis (`note-it`, `noteit`, `noteit-mcp`, `noteit-embed`, `noteit-tui`), arquivos desktop, ícones e licença; criação de `PKGBUILD-git` para trunk; atualização de `scripts/build.sh`; e validação de integridade em chroot limpo. O fechamento absorve a antiga 5.0R: prova de zero violações em todos os gates de fronteira, zero regressões em Core, CLI, MCP, TUI e GUI, validação instalada do pacote, verificação integral do CI remoto, corte da versão `v0.1.0` e publicação no AUR. **Não há nova fase funcional depois da 5.0E; melhorias futuras entram em um novo ciclo de versão.**

      **Acrescentado em 14/09/2026, sem alterar nada acima.** Dois critérios
      novos, decididos pelo dono no fechamento do Ciclo 5:

      1. **O botão INFO e o painel de atalhos têm de estar integralmente
         utilizáveis** em uso normal, incluindo janelas próximas de
         `MIN_NOTE_WIDTH`. Painel cortado = 5.0E BLOCKED. O defeito foi
         reproduzido, medido no WebKitGTK real em 220x300, 420x360 e 760x560, e
         corrigido em `50b40a5`.
      2. **A 5.0E precede a implementação da Fase 6.** Ver 6.0.PRE, opção (a).
         Nenhum código da Fase 6 — nem a 6.0 — começa antes do fechamento
         formal desta fase.

## Fase 6: Rede de notas, navegação e refatoração segura (novo ciclo de versão)

A Fase 5.0E fecha o ciclo `v0.1.0` e diz que "melhorias futuras entram em um novo
ciclo de versão". A Fase 6 é esse ciclo. Ela converte o escopo mestre de 22
features (`Note-It_Escopo_Mestre_Novas_Features_e_Roadmap.md`, v1.0, 14/09/2026)
em fases executáveis, auditadas contra o repositório real em `1186224` e não
contra a suposição de como o Note-it é feito.

**Nada nesta fase foi implementado.** O que segue é plano. Cada subfase existe
para receber, depois de aprovada, um prompt fechado de implementação.

### 6.BASELINE — O que já existe e não será reconstruído

Auditado em `1186224`, com a árvore limpa.

**A aplicação gráfica existe, está empacotada e está instalada para uso diário.**
`note-it 0.1.0.r214.g159fe2df-1` foi construída do commit `159fe2df` e instalada
em 14/09/2026. O `PKGBUILD` fixa o commit, roda `scripts/check` como `check()` e
escreve exclusivamente dentro de `$pkgdir` — a instalação não toca
`~/.local/share/note-it`, `~/.config/note-it` nem `~/.local/state/note-it`.
Entre `159fe2df` e `1186224` **nenhum arquivo de `src/` ou `ui/` mudou**: os três
commits de diferença são `noteit-agent-bridge`, `noteit-tui`, packaging, scripts
e docs. A GUI instalada é, byte a byte de código-fonte, a GUI do HEAD.

**A arquitetura gráfica é GTK4 + WebKitGTK 6.0 + `gtk4-layer-shell`, não Tauri.**
O host nativo é `src/` (`app.rs`, `note_window.rs`, `webview_bridge.rs`,
`write_authority.rs`, `layer_shell.rs`) e o editor é um WebView por nota,
servindo `ui/dist` (TypeScript + Vite + ProseMirror/Tiptap 3). A ponte é um
contrato JSON tipado em `ui/src/bridge/types.ts`
(`HostToWebviewMessage` / `WebviewToHostMessage`), com correlação por
`requestId` e um protocolo de escrita externa
(`begin_external_write` → `external_write_ready` → `apply_external_document` →
`external_write_applied` / `abort_external_write`). **Toda feature gráfica nova
é uma extensão desse contrato — não há comandos Tauri neste projeto e planejar
sobre eles seria planejar sobre outro produto.**

**Cada nota é uma janela, não uma aba de uma biblioteca.** Não existe janela de
biblioteca, sidebar permanente ou shell único. `MIN_NOTE_WIDTH = 220`
(`src/layer_shell.rs:6`) e o orçamento responsivo verificado da 3.14R.1 vai de
220 a 900 px. Tudo o que o escopo mestre chama de "painel lateral" já tem uma
forma neste produto — **painel interno, exclusivo, dentro da própria nota** — e
é assim que `SearchPalette`, `TrashPanel`, `TimerPanel`, `ShortcutsPanel`,
`FlashcardPanel`, `StudyHub` e `MetadataPanel` já funcionam.

**O Core é a autoridade e já é compartilhado.** `noteit-core` é headless, com
gate de fronteira mecânico (`scripts/check-core-boundary`) que impede GTK, GDK,
WebKitGTK, layer-shell, Wayland e Niri de entrarem. GUI, `noteit-cli`,
`noteit-tui` e `noteit-mcp` consomem o mesmo domínio. Exatamente um gravador por
store, garantido por lease `flock` e socket Unix privado (`coordination.rs`), com
`WriteOperation` / `NoteMutation` / `expected_revision` tipados em `write.rs`.

**Fundações reutilizáveis que o escopo mestre pede e que já estão prontas:**

| Fundação | Onde | Para quê serve nesta fase |
| --- | --- | --- |
| Identidade normalizada | `metadata::semantic_identity` — minúscula Unicode + dobra de acento, compartilhada com a busca | Resolução de wikilink e alias sem inventar uma terceira normalização |
| Metadados do usuário | `metadata.rs` — `NoteTags`, `NoteProperties` no front matter | Casa dos aliases e dos estados favorita/fixada/arquivada |
| Filtro tipado | `filter.rs` — `NoteFilter`, AND de tags e propriedades | Núcleo do modelo de query das Saved Views |
| Motor de recuperação | `context.rs` + `semantic.rs` — BM25, canal semântico, `Candidate`, `Reason`, degradação declarada | Notas relacionadas e duplicadas, sem um segundo motor |
| Índice incremental em memória | `semantic::synchronise` — indexa o que falta, esquece o que sumiu, proveniência por revisão | Padrão exato a copiar para o índice de relações |
| Escrita transacional | `atomic_file.rs`, `write.rs`, `authority.rs`, `revision.rs` | Extract, merge e reescrita de links |
| Reversibilidade | `trash.rs` (lixeira recuperável), `backup.rs` (7 snapshots, manifesto v3) | Rede de proteção das operações compostas |
| Projeção de texto visível | `visible_text.rs` + `ui/src/markdown/visibleText.ts` | Saber que um `[[` dentro de fence ou código não é link |

### 6.CONFLITOS — O que a auditoria encontrou antes de qualquer plano

Registrado aqui porque §23 do mandato manda registrar, não corrigir por conta
própria. **Nada abaixo foi alterado nesta execução.**

**C-1 — A visão publicada contradiz o escopo mestre. Decisão do dono, não do agente.**
`docs/vision.md` diz, em duas passagens: *"não pretende substituir bases de
conhecimento abrangentes como Obsidian ou Notion"* e *"A interface não muda. A
GUI continua sendo notas adesivas rápidas na área de trabalho."* As 22 features
do escopo — wikilinks, backlinks, aliases, referências de bloco, transclusão,
outline, breadcrumbs, inspector, templates, saved views — são, somadas, o
movimento em direção a essa categoria. O escopo mestre (§10) e o mandato proíbem
"clone integral do Obsidian", e essa proibição continua valendo para cada feature
individualmente; o conflito é sobre o **agregado** e sobre uma frase publicada
que deixaria de ser verdadeira. **ADR obrigatório antes da 6.A.1**, decidindo se
`docs/vision.md` é emendado conscientemente ou se o escopo é reduzido.

**RESOLVIDO em 14/09/2026 pela ADR-061.** O dono decidiu: a GUI é a experiência
principal, o Core continua sendo a autoridade do domínio, CLI/TUI/MCP continuam
superfícies complementares de primeira classe, paridade semântica é obrigatória
e paridade visual não é. `docs/vision.md` recebeu a emenda mínima — a frase "a
interface não muda" virou "a interface não ganha uma tela de IA", que é o que
ela queria dizer no contexto do Segundo Cérebro, e um princípio de evolução
incremental foi acrescentado. **O que permanece intacto:** local-first, sem
nuvem, sem contas, Markdown como fonte da verdade, privacidade, captura rápida,
e a recusa explícita de virar clone integral do Obsidian ou do Notion.

**C-2 — A nota não tem título. A sintaxe `[[Título da Nota]]` não tem alvo.**
`NoteFrontMatter` (`noteit-core/src/model.rs:40`) carrega `id: Uuid`, cor, papel,
intensidade, fonte e timestamps — e **nenhum título**. O nome legível é
`search::label_for(&content)`: a primeira linha visível não vazia, truncada. Esse
rótulo **não é único** (o próprio roadmap da 3.8 registra: "nunca pelo rótulo,
que duas notas podem compartilhar") e **muda quando o usuário edita a primeira
linha**. Um wikilink resolvido por rótulo quebra sozinho. Esta é a decisão
arquitetural central da Fase 6 e está isolada na 6.0.A.

**RESOLVIDO em 15/09/2026 pela ADR-063.** A auditoria estava certa no
diagnóstico e a medição o endureceu: 39% dos rótulos do corpus colidem após a
dobra semântica, e o rótulo muda até quando a edição não toca na primeira linha
do arquivo. A nota passa a ter um nome declarado — `title` no topo do front
matter, opcional — e o rótulo derivado fica sendo o que sempre foi na prática:
apresentação. Nada foi implementado: `model.rs`, `search.rs` e `metadata.rs`
continuam byte-idênticos.

**C-3 — ADR-027 decidiu, com medição, que não há índice. Backlinks precisam de um.**
ADR-027 recusa índice persistente com número em mão (mil notas varridas, dobradas
e transformadas em trechos em ~40 ms) e nomeia a condição de revisão: *"o dia em
que ela falha é o dia em que essa decisão deve ser revista — com o número em
mãos."* A 6.0.C tem que trazer o número antes de propor qualquer índice, e o
precedente correto já existe no próprio repositório — `semantic::InMemoryIndex`,
que é derivado, incremental, em memória e reconstruível, e **não** um arquivo a
mais para versionar, migrar e incluir no backup.

**C-4 — Não existe operação transacional entre duas notas.** `WriteOperation`
opera sobre uma nota por vez. Extrair seleção (F15) e mesclar notas (F16) são,
por definição, duas ou mais notas mudando juntas. Pelo §9 do escopo mestre, essas
features estão **BLOCKED até existir estratégia compensatória testável** — que é
exatamente por que 6.D.1 (histórico de versões) vem antes delas.

**C-5 — `MANIFEST_VERSION = 3` é estrito, e todo artefato novo do store o afeta.**
`backup.rs` copia `notes/`, `trash/`, `assets/`, `config.toml`, `state.json` e
`study.json`, com cópia estrita e falha fechada. Templates (F11), histórico de
versões (F14) e Saved Views (F21) introduzem artefatos persistentes: cada um
exige manifesto v4 e prova de restauração, ou não é feito.

**C-6 — Deriva documental: três frentes existem em git e não existem no roadmap.**
`git log` em `1186224` mostra 28 commits depois da 5.0D.5 marcados
`5.0D.R5` (9), `5.0D.R6` (13), `5.0E-GUI` (3) e `5.1A` (1). Nenhum desses
identificadores aparece em `docs/roadmap.md` ou `CHANGELOG.md`. O crate
`noteit-agent-bridge` existe, tem testes e gate de fronteira
(`scripts/check-agent-bridge-boundary`), e nenhuma fase o declara. **Não foi
corrigido aqui**: reescrever o passado a partir de mensagens de commit seria
inventar histórico. A 6.0.E faz a reconciliação a partir de `docs/tui.md` e do
git, com o dono confirmando o recorte.

**RESOLVIDO em 14/09/2026.** A deriva era mais estreita do que a primeira
auditoria disse: a documentação profunda **existia** — `docs/tui.md` seção 44
cobre a 5.0D.R6 e cita a R5, e `docs/second-brain.md` especifica a 5.1A. O que
faltava era o índice. As quatro entradas foram escritas acima a partir dessas
fontes e do git, sem renumerar nada e sem mover trabalho concluído para fases
novas. Onde uma fase não tem relatório próprio — a 5.0D.R5 — isso está dito, com
a faixa de commits, em vez de um relatório que ninguém escreveu.

**C-7 — A Fase 4.3 está `- [ ]` com 4.3A…4.3R todas `- [x]`.** Inconsistência de
marcação, não de implementação. **RESOLVIDO em 14/09/2026**: a linha-mãe passou a
`[x]` com a correção anotada nela mesma. Nenhuma entrega mudou.

**C-8 — Dois commits locais não estão em `origin/main` e não passaram por CI.**
`16a182e` e `1186224` estão à frente de `origin/main` (`a7eddfc`). O CI está
verde no `a7eddfc` e nos quatro commits anteriores. Nenhuma fase da 6 deve abrir
sobre uma baseline sem CI.

**C-9 — A 5.0E continua aberta e o pacote diário saiu antes dela.** A 5.0E prevê
5 binários (incluindo `noteit-embed`), `PKGBUILD-git`, chroot limpo, tag `v0.1.0`
e AUR. O pacote instalado tem 4 binários e nenhuma tag foi criada — deliberado e
documentado no próprio `PKGBUILD`. Ver a regra de precedência em 6.0.PRE.

### 6.PROTEGIDO — Áreas que nenhuma fase da 6 pode tocar sem autorização nominal

Esta seção é dirigida a qualquer agente futuro e vale para todas as subfases
abaixo, sem exceção e sem precisar ser repetida em cada uma.

1. **A instalação diária.** `/usr/bin/note-it`, `/usr/bin/noteit`,
   `/usr/bin/noteit-mcp`, `/usr/bin/noteit-tui`, `/usr/share/note-it/`,
   o `.desktop` e o serviço D-Bus. Nenhum `makepkg -i`, `pacman -U`,
   `install`, `cp` para `/usr` ou substituição de binário. **Ausência de
   autorização explícita de release significa NÃO INSTALAR.**
2. **Os dados reais.** `~/.local/share/note-it/{notes,trash,assets,backups}`,
   `~/.config/note-it/config.toml`, `~/.local/state/note-it/state.json`,
   `study.json`. Teste usa `scripts/note-it-isolated` — barramento D-Bus privado
   **e** XDG isolado, porque o Note-it é uma `GApplication` de instância única e
   XDG sozinho já deixou uma nota de teste cair no store real (Fase 3.7R).
3. **O que já está estável.** Notas existentes, atalhos, storage, busca,
   retrieval, flashcards, matemática, conversões, properties, tags, tarefas,
   timer, AutoPaste, imagens, lixeira e backup. Uma feature nova nunca justifica
   regressão silenciosa em nenhum deles.
4. **A fronteira do Core.** Os gates `scripts/check-*-boundary` não são
   formalidade: nenhuma regra de domínio nova pode nascer na GUI, e nenhum
   símbolo gráfico pode entrar no Core.
5. **Versão, packaging, tag e release.** Só a 6.G.4 os toca, e só com
   autorização nominal no prompt daquela fase.

### 6.PROMOCAO — Como código vira versão diária

Os três estados são distintos e o roadmap não os mistura:

```text
DESENVOLVIMENTO            VALIDAÇÃO                    PROMOÇÃO
6.0 → 6.A → 6.B → 6.C  →   6.G.1 regressão integral  →  6.G.4 packaging
6.D → 6.E → 6.F            6.G.2 migração                    ↓
   (código no repo)        6.G.3 build candidato        upgrade testado
   nenhuma instalação      em store descartável              ↓
                                                        rollback provado
                                                              ↓
                                                        nova baseline
```

**Uma subfase concluída não gera instalação.** Várias macrofases podem fechar
antes de existir uma atualização gráfica. A instalação diária só muda na 6.G.4,
depois de 6.G.1–6.G.3 passarem, e a pergunta *"se der errado, como volto sem
perder minhas notas?"* precisa ter resposta testada antes — não depois.

`dev build` ≠ `candidate build` ≠ `release build` ≠ versão instalada.

### 6.VERSAO — Unidades de promoção e a versão instalada

Política completa na **ADR-062**. Uma fonte canônica, e este roadmap não a
repete: o que segue é só o recorte que a Fase 6 precisa.

A versão do aplicativo é `0.MINOR.PATCH`, avançando `0.1.1 → 0.1.2 → … →
0.1.100 → 0.2.0`. Ela anda quando uma **unidade de promoção** fecha com PASS —
não por commit, não por subfase, não por fase documental.

**Unidades de promoção da Fase 6.** Cada linha abaixo, ao receber PASS, exige
bump de versão, build, pacote, atualização da instalação diária e smoke test na
instalação real. Nenhuma outra subfase exige.

| Unidade | O que entrega | Promove? |
| --- | --- | --- |
| 6.0 inteira (A, A.2, B, C, D, E) | Contratos e ADRs, nenhum `.rs` | **Não** — documental, ADR-062 |
| 6.A.1 … 6.A.6 | Wikilinks utilizáveis ponta a ponta, com aliases | **Sim**, ao fechar 6.A.6 |
| 6.A.7 | Backlinks | **Sim** |
| 6.A.8 + 6.A.9 | Links de seção e referências de bloco | **Sim**, juntas |
| 6.A.10 + 6.A.11 | Embeds e preview | **Sim**, juntas |
| 6.A.12 | Menções não vinculadas | **Sim** |
| 6.A.R | Auditoria adversarial | **Não** — auditoria |
| 6.B.1 … 6.B.4 | Outline, histórico, breadcrumbs, inspector | **Sim**, cada uma |
| 6.C.1 + 6.C.2 | Templates utilizáveis | **Sim**, juntas |
| 6.C.3, 6.C.4 | Slash commands; Quick Capture | **Sim**, cada uma |
| 6.D.1 + 6.D.2 | Histórico de versões utilizável | **Sim**, juntas |
| 6.D.3, 6.D.4 | Extract; Merge | **Sim**, cada uma |
| 6.E.1, 6.E.2 + 6.E.3 | Estados; Saved Views | **Sim** — 6.E.2 sozinha não, é modelo |
| 6.F.1, 6.F.2 | Relacionadas; Duplicadas | **Sim**, cada uma |
| 6.G.* | Validação e promoção final do ciclo | **Sim**, na 6.G.4 |

**6.A.1 a 6.A.5 não promovem sozinhas.** Um parser sem resolução, ou uma
resolução sem navegação, não é uma feature que alguém possa usar — é meio
caminho, e meio caminho não vira versão instalada.

**O gate de promoção**, idêntico para toda unidade marcada **Sim**:

```text
implementação -> testes -> auditoria -> PASS funcional
   -> bump de versão -> build -> pacote -> validação do pacote
   -> upgrade da instalação diária -> smoke test na instalação real
   -> rotação de pacotes (fica N e N-1; N-2 sai só depois de N validada)
   -> baseline registrada -> PASS de promoção
```

Uma unidade não está **Done** quando o código está mergeado: está Done quando a
GUI que o usuário abre todo dia tem a feature dentro e o smoke test provou isso.

### 6.DEPENDENCIAS — Por que cada fase vem onde vem

```text
                    6.0 contratos (nenhum código de produção)
                     │
        ┌────────────┴──────────────┬──────────────┬───────────────┐
        ▼                           ▼              ▼               ▼
  6.A.1 identidade           6.C.1 templates   6.E.1 estados   6.D.1 versões
        │                          │                │               │
  6.A.2 parser                6.C.2 aplicar    6.E.2 query      6.D.2 UI
        │                          │                │               │
  6.A.3 aliases               6.C.3 comandos / 6.E.3 saved      6.D.3 extract ──┐
        │                          │              views             │          │
  6.A.4 índice de relações    6.C.4 quick                       6.D.4 merge ────┤
        │                        capture                                        │
  ┌─────┴─────┐                                                                 │
  ▼           ▼                                                                 │
6.A.5 GUI   6.A.6 TUI/CLI                                                       │
  │                                                                             │
6.A.7 backlinks ──────────┬──────────────┐                                      │
  │                       ▼              ▼                                      │
6.A.8 headings      6.B.2 histórico  6.B.4 inspector                            │
  │                   navegação                                                 │
6.A.9 blocos              │                                                     │
  │                  6.B.3 breadcrumbs                                          │
6.A.10 embeds                                                                   │
  │                                                                             │
6.A.11 preview        6.B.1 outline (depende só de 6.A.8)                       │
  │                                                                             │
6.A.12 menções        6.F.1 relacionadas → 6.F.2 duplicadas                     │
  │                                                                             │
  └──────────────────────► 6.G validação global e promoção ◄────────────────────┘
```

| Aresta | Por quê |
| --- | --- |
| 6.0.A → tudo em 6.A | Sem decidir o que um link aponta, o parser não tem alvo (C-2) |
| 6.0.B → 6.A.2 | Sintaxe decidida antes de espalhar parser por Core, GUI e TUI (§9 do escopo) |
| 6.0.C → 6.A.4 | ADR-027 precisa ser revisada com número antes de qualquer índice (C-3) |
| 6.0.D → 6.A.5 | Onde um painel cabe numa nota de 220 px é contrato, não improviso |
| 6.A.1 → 6.A.2 | Resolver identidade é independente de reconhecer sintaxe; separá-los deixa o resolvedor testável sozinho |
| 6.A.3 → 6.A.4 | O índice indexa destinos resolvidos; alias muda o que "resolvido" quer dizer |
| 6.A.4 → 6.A.7 | Backlink é a leitura inversa do mesmo índice, não uma segunda varredura |
| 6.A.2 → 6.A.5 e 6.A.6 | GUI e TUI consomem o mesmo parser; se a GUI for primeiro e sozinha, a semântica diverge (§13 do mandato) |
| 6.A.8 → 6.A.9 → 6.A.10 | Embed de seção precisa de heading indexado; embed de bloco precisa de ID de bloco |
| 6.A.10 → 6.A.11 | Preview e embed renderizam conteúdo de outra nota; o segundo reusa o primeiro, não o contrário |
| 6.A.7 → 6.A.12 | Menção não vinculada só faz sentido contra o que já está vinculado |
| 6.A.8 → 6.B.1 | Outline é a mesma AST de headings, apresentada |
| 6.A.5 → 6.B.2 | Só há o que empilhar depois que um link navega |
| 6.A.7 → 6.B.4 | O inspector agrega contagens que o índice já tem |
| 6.D.1 → 6.D.3 e 6.D.4 | C-4: operação composta sem recuperação está BLOCKED |
| 6.E.2 → 6.E.3 | Uma view é uma query salva; sem modelo de query não há o que salvar |
| 6.E.1 → 6.E.2 | Estado é mais um campo filtrável; defini-lo depois obrigaria a mudar o modelo duas vezes |
| 6.F.1 → 6.F.2 | Duplicada é o caso extremo de relacionada, com limiar e comparação |
| tudo → 6.G | Promoção só acontece sobre um conjunto aprovado |

Não há ciclo. A única aresta que a intuição sugere e que foi **deliberadamente
cortada** é 6.A.11 preview → 6.A.5 GUI: o preview não pode ser pré-requisito do
link, senão o link não navega até existir popover.

### 6.0.PRE — Precedência entre a 5.0E e a Fase 6 (decisão pendente do dono)

O roadmap diz que não há fase funcional depois da 5.0E. A 5.0E está aberta, e
mesmo assim já existe um pacote diário instalado (`5.0E-GUI`, C-9). São dois
caminhos legítimos e a escolha **não é do agente**:

- **(a) Fechar a 5.0E primeiro** — tag `v0.1.0`, 5 binários, chroot, AUR — e só
  então abrir a 6.A. Vantagem: o ciclo fecha como foi prometido e a baseline
  passa a ter um nome estável para rollback.
- **(b) Abrir a 6.0 em paralelo** e manter a 5.0E como o marco que precede a
  primeira promoção da 6.G.4. Vantagem: a 6.0 não escreve `.rs` nenhum, então não
  compete com a 5.0E por código.

**DECIDIDO em 14/09/2026: opção (a).** O dono determinou que a 5.0E é fechada
formalmente antes de qualquer implementação da Fase 6. A política do projeto
passa a ser:

```text
Ciclo 5 -> concluir -> validar -> fechar 5.0E -> baseline -> então a Fase 6
```

Nem `6.0.A`, nem `6.A.1`, nem qualquer outro código da Fase 6 começa antes disso.
A Fase 6 **pode permanecer planejada** — este documento é o plano — e **não pode
entrar em implementação**. A 6.0 é documental e mesmo ela espera: o gate não é
sobre escrever `.rs`, é sobre encerrar um ciclo antes de abrir o próximo.

---

## Fase 6.0 — Contratos e reconciliação (gate arquitetural)

**Nenhum `.rs`, `.ts` ou `.css` de produção é escrito nesta macrofase.**
`Cargo.lock` e `pnpm-lock.yaml` permanecem byte-idênticos. É o padrão que a 4.3A
e a 5.0D.4A estabeleceram neste repositório: medir e decidir antes de implementar.

### 6.0.A — Identidade nomeável da nota

**Objetivo.** Decidir, com evidência, o que `[[algo]]` resolve — e registrar em
ADR (ADR-063 sugerido) antes de qualquer parser existir.

**Motivação.** C-2: a nota tem `id: Uuid` e nenhum título; o rótulo é derivado,
mutável e não único. Sem essa decisão, toda a macrofase 6.A é chute.

**Escopo.** Levantar a evidência real (quantas notas do store de teste têm
rótulos colidentes; quantas teriam rótulo alterado por uma edição de primeira
linha); avaliar as quatro opções abaixo contra Markdown portável, estabilidade do
link, migração de notas antigas e paridade GUI/TUI/CLI; escolher uma; registrar
ADR com o contraexemplo que mata cada alternativa recusada.

| Opção | Custo | Risco principal |
| --- | --- | --- |
| (a) Resolver por rótulo derivado | Zero migração | Link quebra ao editar a primeira linha; colisão silenciosa |
| (b) Campo `title` novo no front matter | Migração de todas as notas; `NoteFrontMatter` versionado | Duas ideias de nome (título vs. rótulo) convivendo |
| (c) Nome declarado em `properties` (usa `metadata.rs` como está) | Nenhum campo canônico novo | Propriedade é `String` única — aliases precisam de lista (ver 6.0.A.2) |
| (d) `[[uuid]]` com texto de exibição | Estabilidade máxima | Markdown ilegível fora do Note-it — fere §9 do mandato |

**Fora de escopo.** Escrever o resolvedor; tocar `model.rs`; escolher sintaxe de
link (é 6.0.B); decidir alias (depende desta, e é 6.0.A.2).

**Dependências.** Nenhuma. É a primeira.

**Componentes.** `docs/decisions.md`, `docs/roadmap.md`. Leitura de
`noteit-core/src/model.rs`, `search.rs`, `metadata.rs`.

**Contratos a fixar.** Identidade canônica; identidade nomeável; normalização
(reusar `semantic_identity`, não criar a terceira); política de colisão —
**desambiguação explícita ou não resolvido, nunca escolha por ordem de
filesystem**; o que acontece quando o nome muda.

**Riscos.** Escolher (b) sem plano de migração transforma notas antigas em notas
sem nome. Escolher (a) entrega links que quebram sozinhos e culpa o usuário.

**Testes.** Nenhum código. A evidência é medição sobre um store sintético
versionado, no padrão de `docs/retrieval-corpus.json`.

**Aceite.** ADR escrita, com: a opção escolhida, o contraexemplo concreto que
elimina cada uma das outras três, a regra de colisão, a regra de renomeação, e a
migração descrita passo a passo se houver campo novo. Revisão adversarial
independente aprovando, no padrão da 5.0D.4A.

**Parada.** Se a opção escolhida exigir alterar `NoteFrontMatter` sem migração
testável para notas sem o campo, PARAR: isso é mudança de contrato canônico
estabilizado e §9 do escopo manda apresentar evidência antes de codar.

**CONCLUÍDA em 15/09/2026 — PASS. Decisão na ADR-063.** Venceu a opção (b), com
o campo no **topo** do front matter e não dentro de `note_it`: identidade
canônica continua sendo o UUID do arquivo, identidade nomeável é um `title`
opcional irmão de `tags` e `properties`, e o rótulo derivado é rebaixado
formalmente a apresentação — ele nunca resolve. Colisão tem três resultados
declarados (`RESOLVIDO`, `NÃO RESOLVIDO`, `AMBÍGUO`) e nenhuma superfície pode
escolher entre candidatos; o espaço de nomes é o das notas vivas, e restaurar da
lixeira pode criar ambiguidade sem que a restauração seja bloqueada. **Sem
migração em massa**: a ausência do campo é o estado definido de "nota sem nome".

Evidência em `docs/naming-identity-measurement.md`, sobre o corpus versionado
`docs/naming-corpus.json` (36 notas), medida com o binário real de `328698be`:
39% dos rótulos derivados colidem após `semantic_identity`, 11 de 12 mudam
quando a primeira linha é editada, e 4 de 10 mudam por edições que **não** tocam
nela. A medição que decidiu a posição do campo: `title` no topo sobrevive a uma
gravação do binário atual em todas as formas testadas; `note_it.title` é apagado
em silêncio, porque o bloco reservado não preserva chave desconhecida — achado
registrado como bloqueio para qualquer campo futuro ali dentro.

Revisão adversarial independente em duas passagens: a primeira devolveu 1
blocker, 3 major, 3 minor e 1 nit, e o blocker reabriu a escolha de onde o campo
morava, que foi medida e invertida; a segunda fechou com 0 blocker e 0 major.
Nenhum `.rs`, `.ts` ou `.css` alterado; `Cargo.lock` e `ui/pnpm-lock.yaml`
byte-idênticos; sem bump de versão, sem pacote, sem instalação — a 6.0 não é
unidade de promoção (ADR-062).

### 6.0.A.2 — Semântica de alias

**Objetivo.** Decidir como uma nota carrega vários nomes, dado que
`NoteProperties` é um mapa chave→**String única** (`metadata.rs:189`).

**Escopo.** Escolher entre: propriedade com valor delimitado (barato, feio,
ambíguo com vírgula no nome); tipo de propriedade com lista (muda o serializador
de properties e o contrato 4.0B); ou chave de front matter própria fora de
`properties`. Definir limite de aliases por nota, normalização, e o que acontece
quando um alias de A é o nome de B.

**Fora de escopo.** Implementar; UI de chips no inspector.

**Dependências.** 6.0.A.

**Contratos.** Alias é nome, nunca cópia; alias nunca é inferido nem persistido
sem ação do usuário; conflito alias×título×alias tem resposta declarada.

**Aceite.** ADR com o formato YAML exato que uma nota com três aliases terá em
disco, e a garantia de que uma nota antiga sem o campo continua abrindo.

**Parada.** Se a opção escolhida criar um segundo formato de metadados
concorrendo com tags/properties, PARAR — §9 do escopo proíbe a terceira fonte.

### 6.0.B — Contrato de sintaxe de links

**Objetivo.** Fixar a gramática de `[[nota]]`, `[[nota#seção]]`, `[[nota^bloco]]`
e `![[nota]]` antes de qualquer parser, em ADR (ADR-065 sugerido).

**Motivação.** §9 do escopo: "Se houver ambiguidade de sintaxe Markdown,
registrar decisão arquitetural antes de espalhar o parser por várias camadas." E
aqui são três camadas reais: `noteit-core`, o Tiptap da GUI e o
`markdown.rs`/`inline.rs` da TUI.

**Escopo.** Precedência contra o que já existe: fenced code, code span, escape,
comentário HTML, o subconjunto HTML canônico (`span[data-note-it-color]`,
`mark[data-note-it-highlight]`, `<u>`), autolink, imagem gerenciada
`note-it-asset:`, delimitador de flashcard `::` / `:::`, e a sintaxe de
matemática (`=` inicial, `nome :=`). Escaping. Nome contendo `]]`, `#`, `^`, `|`,
quebra de linha, ou vazio. Limite de comprimento. Comportamento de `[[` sem
fechamento até o fim do documento.

**Fora de escopo.** Implementar o parser; decidir aparência.

**Dependências.** 6.0.A, 6.0.A.2.

**Componentes.** `docs/decisions.md`; leitura de `ui/src/markdown/sanitizer.ts`,
`ui/src/editor/flashcardMark.ts`, `noteit-tui/src/inline.rs`,
`noteit-core/src/visible_text.rs`.

**Riscos.** `::` de flashcard e `[[` convivem no mesmo parágrafo; um wikilink
dentro de um bloco de código tem que continuar sendo texto; a projeção
lossless da 5.0D.4B tem um envelope de reescrita declarado que um nó novo pode
violar.

**Testes.** Nenhum código. A entrega é uma tabela de casos — pelo menos 40
entradas, cada uma com a fonte e o resultado esperado — que vira fixture
compartilhada pelas três implementações na 6.A.2.

**Aceite.** ADR aprovada por revisão adversarial independente; tabela de casos
completa, incluindo todos os adversariais acima; declaração explícita de que a
gramática não altera nenhum documento existente (todo `.md` atual do store de
teste produz exatamente os mesmos bytes ao ser lido e reescrito).

**Parada.** Se a sintaxe escolhida colidir com flashcards, matemática ou o
envelope de reescrita da 5.0D.4B e a colisão não puder ser resolvida por
precedência declarada, PARAR e apresentar evidência.

### 6.0.C — Contrato do índice de relações

**Objetivo.** Decidir se existe índice, de que tipo, e com que orçamento —
revisando ADR-027 **com número em mãos**, como a própria ADR-027 exige.

**Motivação.** C-3. Backlinks (F02), menções (F04), inspector (F22) e duplicadas
(F20) leem relação inversa. Varrer o store inteiro a cada abertura de painel pode
ser aceitável; a cada tecla, nunca é.

**Escopo.** Medir a varredura real em stores sintéticos de 100, 1.000, 5.000 e
20.000 notas: tempo de extrair links de todas, memória do mapa
destino→origens, e custo de atualizar após uma edição. Comparar com o precedente
`semantic::InMemoryIndex` (em memória, incremental, com proveniência por
revisão, reconstruível, **sem arquivo**). Definir orçamentos: abertura de nota,
abertura de painel de backlinks, custo por tecla (alvo: zero — nada de
reindexação síncrona no caminho de digitação).

**Fora de escopo.** Implementar o índice; persistir qualquer coisa em disco.

**Dependências.** 6.0.B (só se sabe o que indexar depois de saber o que é link).

**Contratos.** Índice é derivado e descartável; apagá-lo e reconstruí-lo não
perde informação; falha de índice degrada a UI, nunca torna a nota inacessível;
atualização é incremental; invalidação é por `NoteRevision`.

**Riscos.** Um índice persistente traria invalidação, versão de formato,
migração, entrada no manifesto de backup (C-5) e uma segunda implementação para
a CLI concordar — precisamente os custos que ADR-027 recusou.

**Aceite.** ADR com os quatro números medidos, o orçamento declarado por
superfície, e a decisão. Se a decisão for "em memória, como o semântico",
dizer explicitamente que nenhum arquivo novo entra em `StorePaths` — e então
C-5 não se aplica a esta macrofase.

**Parada.** Se a medição mostrar que a varredura sob demanda excede o orçamento
de abertura de painel já em 1.000 notas, o índice em memória vira obrigatório e
a subfase 6.A.4 ganha escopo; se exceder mesmo em memória, PARAR e reprojetar
antes de 6.A.7.

### 6.0.D — Contrato de superfície gráfica

**Objetivo.** Decidir onde backlinks, outline, inspector, relacionadas,
breadcrumbs e preview aparecem numa janela de nota de 220 a 900 px, sem
transformar a nota adesiva num IDE.

**Motivação.** O escopo mestre descreve "painel lateral" seis vezes. Este produto
não tem sidebar: tem painéis internos exclusivos. Seis painéis novos coordenados
à mão em `ui/src/main.ts` — que hoje fecha cada painel citando-o pelo nome — é
dívida no primeiro dia.

**Escopo.** Auditar o padrão atual (`SearchPalette`, `TrashPanel`, `TimerPanel`,
`ShortcutsPanel`, `FlashcardPanel`, `StudyHub`, `MetadataPanel`) e a coordenação
em `main.ts`; decidir se nasce um host de painéis com exclusividade declarativa;
definir o orçamento do cabeçalho (a 3.12R.1 já registra que o clipe some abaixo
de 300 px e que as ações rápidas somem numa nota recolhida); decidir qual
informação é painel, qual é linha fina, qual é popover e qual é só um atalho;
fixar tokens de movimento, foco, `Escape` e `prefers-reduced-motion` reusando os
da 3.14R.1.

**Fora de escopo.** Redesenho da nota; qualquer alteração visual em produção;
segunda janela GTK; glassmorphism, gradiente decorativo, sombra pesada, gauge ou
gráfico — todos proibidos pelo §14 do mandato.

**Dependências.** Nenhuma técnica; conceitualmente informa 6.A.5 em diante.

**Componentes.** `docs/decisions.md`; leitura de `ui/src/main.ts`,
`ui/src/ui/*.ts`, `ui/src/styles/theme.css`, `src/layer_shell.rs`.

**Riscos.** Painel permanente numa nota de 220 px come a nota. Seis painéis
exclusivos coordenados manualmente produzem estados impossíveis.

**Aceite.** ADR nomeando, para cada uma das features gráficas da Fase 6, a forma
que ela assume e a largura mínima em que ela aparece; matriz de larguras
(220/300/400/600/900) dizendo o que está visível em cada uma; declaração de que
nenhuma informação essencial existe só em hover (§14 do mandato e F08 do escopo).

**Parada.** Se a conclusão for que a arquitetura de janela por nota não comporta
a feature, **não implementar mesmo assim**: registrar problema, evidência,
limitação, impacto, alternativas e proposta, como manda o §11 do complemento, e
abrir uma subfase própria para a mudança estrutural.

### 6.0.E — Reconciliação documental

**Objetivo.** Fechar a deriva C-6, C-7 e C-9 para que a Fase 6 comece sobre um
roadmap verdadeiro.

**Escopo.** Reconstruir, a partir de `docs/tui.md` e do git (não de memória), as
entradas de 5.0D.R1–R6, 5.0E-GUI e 5.1A; declarar `noteit-agent-bridge` no
roadmap com a fase que o produziu; corrigir a marcação da 4.3; registrar a
decisão 6.0.PRE; registrar a decisão C-1 sobre `docs/vision.md`.

**Fora de escopo.** Renumerar, reescrever ou invalidar qualquer fase concluída.
Editar `docs/vision.md` sem ADR aprovada.

**Dependências.** Nenhuma.

**Aceite.** Todo identificador de fase que aparece em `git log` aparece no
roadmap; nenhum número antigo mudou; `CHANGELOG.md` e `docs/roadmap.md` concordam;
o dono confirmou o recorte das frentes que não documentou.

**Parada.** Se o histórico não permitir reconstruir uma fase com honestidade,
registrá-la como "executada, não documentada na época" com a lista de commits —
e não inventar um relatório que ninguém escreveu.

---

## Fase 6.A — Linking Core

**Features cobertas:** F01 wikilinks, F03 aliases, F02 backlinks, F05 links para
headings, F06 referências de bloco, F07 embeds, F08 preview, F04 menções não
vinculadas.

**Princípio da macrofase.** Um único modelo de resolução e relacionamento, no
Core, antes de qualquer açúcar de UI. A GUI apresenta; o domínio decide. Nenhuma
das doze subfases abaixo pode acrescentar uma segunda opinião sobre o que um link
significa.

**Gate comum a todas as subfases da 6.A** (além do gate de cada uma):
`scripts/check rust` e `scripts/check frontend` completos, incluindo os gates de
fronteira; CI verde no SHA de fechamento; store real verificado por fingerprint
idêntico antes e depois de qualquer execução manual; `Cargo.lock` e
`pnpm-lock.yaml` byte-idênticos salvo dependência aprovada nominalmente.

### 6.A.1 — Resolvedor de identidade no Core

**Objetivo.** Uma função no Core que, dado um nome, devolve exatamente uma de
três respostas: resolvido para um `Uuid`, ambíguo com a lista de candidatos, ou
não resolvido. Sem parser, sem sintaxe, sem UI.

**Motivação.** Separar "quem é essa nota" de "como o texto pede por ela" deixa o
resolvedor testável sozinho e impede que a regra nasça na GUI (§13 do mandato).

**Escopo.** Tipo de resultado tipado (nada de `Option<Uuid>`, que apaga a
diferença entre ambíguo e inexistente); normalização por `semantic_identity`;
construção do mapa nome→notas a partir do store; política de colisão da 6.0.A.

**Fora de escopo.** Reconhecer `[[...]]`. Aliases (6.A.3). Índice incremental
(6.A.4). Qualquer escrita.

**Dependências.** 6.0.A, 6.0.A.2 e a decisão 6.0.PRE registrada.

**Componentes.** `noteit-core` (módulo novo, provavelmente `link.rs`);
possivelmente `model.rs` se a 6.0.A tiver escolhido campo novo.

**Contratos.** Ambiguidade é um valor, nunca um `unwrap`. Resolver não escreve
nada e não move `updated_at`.

**Riscos.** Reintroduzir uma normalização paralela à de tags/busca.

**Testes.** Unitários de normalização (maiúscula, acento, espaço, Unicode
composto vs. decomposto); colisão de duas notas com o mesmo nome; nome vazio;
nome só de espaços; store vazio; nota ilegível no meio do store (tem que virar
`ReadWarning`, não erro fatal). Property test: resolver duas vezes dá o mesmo
resultado.

**Aceite.** Resolver nunca escolhe entre duas notas. Nenhum caminho de leitura
escreve — provado por fingerprint do store antes/depois, como a 3.8 já faz.
Cobertura dos casos da tabela da 6.0.A.

**Parada.** Se a 6.0.A tiver escolhido campo novo no front matter e a migração
não estiver escrita e testada, PARAR.

### 6.A.2 — Parser de wikilinks no Core

**Objetivo.** Reconhecer a gramática da 6.0.B sobre Markdown, produzindo
referências tipadas com posição na fonte, sem alterar um byte do documento.

**Escopo.** Implementar a tabela de casos da 6.0.B como fixture compartilhada;
reconhecer nota, `#seção`, `^bloco` e a forma de embed; respeitar precedência de
fence, code span, escape e HTML admitido; devolver offsets reais na fonte.

**Fora de escopo.** Resolver (é 6.A.1, que esta consome). Renderizar. Escrever.
Reescrever links.

**Dependências.** 6.0.B, 6.A.1.

**Componentes.** `noteit-core`; fixture em `tests/fixtures/`.

**Contratos.** Parsing é puro e total: toda entrada produz saída, nenhuma
entrada produz pânico. Markdown continua a fonte da verdade — o parser lê.

**Riscos.** Divergir do que a GUI (Tiptap) e a TUI (`inline.rs`) entendem por
"dentro de código". A 5.0D.5 já provou que a única defesa é fixture afirmada
pelos dois lados, nunca comparar implementações entre si.

**Testes.** A fixture das 40+ entradas da 6.0.B rodando no Core. Property test:
para todo documento, concatenar os trechos entre referências com as próprias
referências reproduz a fonte byte a byte. Adversarial: `[[` sem fechar em nota de
2 MB; 10.000 links numa nota; Unicode em nome; `[[a]]]]`; `[[]]`.

**Aceite.** Cobertura byte-exact provada por property test. Nenhum `.md` do store
sintético muda ao ser lido e reescrito. Custo de parsear uma nota de 64 KB
medido e registrado, no padrão que a 5.0D.4B estabeleceu.

**Parada.** Se algum caso da fixture não puder ser satisfeito sem mudar a
gramática, voltar à 6.0.B — não ajustar a expectativa do teste para o CI ficar
verde (§9 do escopo).

### 6.A.3 — Aliases

**Objetivo.** Uma nota responde por vários nomes; `[[HAS]]` e `[[Hipertensão]]`
chegam à mesma nota quando ela os declara.

**Escopo.** Persistência no formato decidido em 6.0.A.2; validação (limite,
normalização, caractere proibido); extensão do resolvedor 6.A.1; detecção de
conflito alias×nome×alias entre notas diferentes; leitura por CLI e TUI.

**Fora de escopo.** Chips no inspector (6.B.4). Inferir alias. Renomear nota.

**Dependências.** 6.0.A.2, 6.A.1.

**Componentes.** `noteit-core` (`metadata.rs`, `link.rs`), `noteit-cli`,
`noteit-tui`, ponte GUI.

**Contratos.** Alias nunca é inferido nem gravado sem ação explícita. Alias não é
cópia. Conflito é reportado, nunca resolvido por sorte.

**Riscos.** Se aliases mudarem o serializador de properties, notas antigas
precisam continuar abrindo — e `unknown_front_matter` (`model.rs:100`) já é o
mecanismo que preserva YAML de terceiros; ele não pode ser quebrado.

**Testes.** Nota com 0, 1 e N aliases; alias igual ao próprio nome; alias de A
igual ao nome de B; alias duplicado entre duas notas; nota antiga sem o campo;
front matter com YAML de outra ferramenta preservado. Round-trip de serialização.

**Aceite.** Múltiplos aliases resolvem para uma nota. Conflito detectado em teste,
com a resposta declarada na 6.0.A.2. Busca e wikilink usam a mesma semântica —
provado por teste que exercita os dois caminhos com a mesma entrada.

**Parada.** Se preservar `unknown_front_matter` e adicionar aliases forem
incompatíveis no formato escolhido, PARAR e voltar à 6.0.A.2.

### 6.A.4 — Índice de relações

**Objetivo.** Origem→destinos e destino→origens, derivado, incremental e
reconstruível, dentro do orçamento medido na 6.0.C.

**Escopo.** Estrutura decidida na 6.0.C (esperado: em memória, no padrão
`semantic::InMemoryIndex`); sincronização incremental — indexar o que falta,
esquecer o que sumiu; proveniência por `NoteRevision`; trecho contextual da
ocorrência para o backlink poder mostrar onde o link foi usado; degradação
declarada quando uma nota não pode ser lida.

**Fora de escopo.** Persistir em disco (salvo se a 6.0.C tiver decidido o
contrário — e então esta subfase ganha manifesto de backup v4 e prova de
restauração, C-5). Qualquer painel.

**Dependências.** 6.0.C, 6.A.2, 6.A.3.

**Componentes.** `noteit-core`.

**Contratos.** Apagar o índice e reconstruí-lo não perde informação. Índice
indisponível degrada a feature, nunca a nota. Nenhuma reindexação no caminho de
digitação.

**Riscos.** Índice discordar das notas — o custo exato que ADR-027 recusou.
Crescimento de memória em biblioteca grande.

**Testes.** Reconstrução total igual à incremental, sobre a mesma sequência de
edições (property test). Nota removida some do índice. Nota restaurada da lixeira
volta. Nota ilegível vira `ReadWarning` sem derrubar a sincronização. Benchmark
nos quatro tamanhos da 6.0.C, com os orçamentos como asserção — não como
comentário.

**Aceite.** Os orçamentos da 6.0.C viram testes que reprovam quando estourados.
Índice reconstruído de zero é idêntico ao incremental. Memória medida e
registrada em 20.000 notas.

**Parada.** Se o orçamento estourar, PARAR antes da 6.A.7: um painel de backlinks
que trava a nota é pior do que nenhum painel (§12 do mandato).

### 6.A.5 — Wikilink na GUI: apresentação e navegação

**Objetivo.** O link parece nativo do texto, distingue resolvido de não
resolvido, e abrir leva à nota certa.

**Escopo.** Nó/decoração no Tiptap consumindo o parser do Core via ponte;
mensagens novas na ponte (`resolve_links_requested` / `link_resolution_result` /
`open_note_requested`, nomes a fixar na implementação); ativação por clique e por
teclado; abertura reusando o caminho que `open_search_result` já tem em
`src/app.rs`; criação de nota ausente **somente** por ação explícita, com
confirmação; estado visual sutil para link quebrado.

**Fora de escopo.** Preview (6.A.11). Backlinks (6.A.7). Histórico
voltar/avançar (6.B.2). Criar nota automaticamente ao digitar `[[`.

**Dependências.** 6.A.2, 6.A.3, 6.0.D.

**Componentes.** `ui/src/editor/`, `ui/src/markdown/sanitizer.ts`,
`ui/src/bridge/types.ts`, `src/webview_bridge.rs`, `src/app.rs`.

**Contratos.** O link é texto Markdown portável no arquivo — nada de nó
proprietário serializado de outro jeito. Digitar `[[` não cria nada. O
sanitizador conhece a forma canônica ou descarta.

**Riscos.** Round-trip Markdown quebrando (o risco histórico número um deste
editor, ver Fases 1 e 3.5). Link dentro de bloco de código virando link. Criação
acidental de nota.

**Testes.** Round-trip: documento com links entra e sai idêntico. Undo depois de
qualquer inserção restaura exatamente o anterior. Link em fence continua texto.
Abrir link para nota fechada, nota na lixeira, nota inexistente, nota ambígua.
Teclado equivalente ao mouse. `updated_at` não se move ao navegar.

**Aceite.** Arquivo continua Markdown legível fora do Note-it — verificado abrindo
o `.md` em outro editor no roteiro manual. Nenhuma ação implícita cria, apaga ou
reescreve nota. Round-trip provado por teste.

**Parada.** Se a serialização de ida e volta não for exata, PARAR: um editor que
altera o arquivo ao abrir é perda de dados silenciosa.

### 6.A.6 — Paridade TUI e CLI

**Objetivo.** A mesma semântica de resolução nas outras duas interfaces, sem uma
segunda implementação.

**Escopo.** TUI: reconhecer e apresentar links no leitor e no editor Visual
(5.0D.4B), navegar por teclado. CLI: comando de leitura que lista links de uma
nota e resolve um nome; `--json` no contrato da 4.0F.

**Fora de escopo.** Backlinks (6.A.7 entrega as três interfaces juntas). Edição
de link por comando.

**Dependências.** 6.A.2, 6.A.3.

**Componentes.** `noteit-tui`, `noteit-cli`, `noteit-mcp` (se a 6.0.B tiver
decidido expor links no contrato de tools).

**Contratos.** Regra no Core; interfaces variam, semântica não (§13 do mandato).

**Riscos.** O editor Visual tem envelope de reescrita declarado (5.0D.4A §26) —
um nó novo pode violá-lo e a violação é silenciosa até uma fixture pegá-la.

**Testes.** Fixture cruzada no padrão da 5.0D.5: um conjunto de notas com links,
afirmado pelas três implementações em suas próprias suítes. `TestBackend` e PTY
real para a TUI. Snapshot de saída da CLI.

**Aceite.** Três interfaces, uma resposta, provado por fixture — não por
comparação entre implementações. Zero violação nos gates de fronteira.

**Parada.** Se o editor Visual não puder exibir link sem violar o envelope,
exibir como fonte e recusar a edição com aviso nomeando a causa, exatamente como
a 5.0D.4B já faz — nunca esconder o que não sabe editar.

### 6.A.7 — Backlinks

**Objetivo.** Cada nota mostra quem aponta para ela, com trecho de contexto, sem
escrever nada na nota.

**Escopo.** Leitura inversa do índice 6.A.4 no Core; painel interno na GUI no
formato decidido em 6.0.D; equivalente na TUI; comando na CLI; contagem no
cabeçalho apenas se couber no orçamento da 6.0.D.

**Fora de escopo.** Menções não vinculadas (6.A.12) — que são outra coisa e não
podem se misturar. Gravar seção "Backlinks" no Markdown. Grafo visual.

**Dependências.** 6.A.4, 6.0.D.

**Componentes.** `noteit-core`, `ui/src/ui/` (painel novo), `src/webview_bridge.rs`,
`noteit-tui`, `noteit-cli`.

**Contratos.** Backlink é derivado e nunca conteúdo. Edição não espera
reindexação síncrona. Link explícito e menção textual nunca aparecem na mesma
lista sem rótulo distinto.

**Riscos.** Painel roubando largura numa nota estreita. Reindexação bloqueando a
digitação.

**Testes.** Criar link → backlink aparece; apagar link → some; renomear alvo →
segue a regra da 6.0.A; nota na lixeira não aparece como origem; apagar o índice
e reconstruir dá a mesma lista. Teste de responsividade em 220/300/400/900 px.
Teste de que digitar não dispara reindexação.

**Aceite.** Backlinks refletem o estado real depois de salvar. Índice apagado e
reconstruído produz lista idêntica. Nenhum byte escrito na nota de destino —
fingerprint antes/depois.

**Parada.** Se manter os backlinks atualizados exigir trabalho no caminho de
digitação, PARAR e mover para sob demanda (§12 do mandato: não aceitar como
trade-off).

### 6.A.8 — Headings e links de seção

**Objetivo.** `[[Nota#Seção]]` resolve determinístico e posiciona na seção.

**Escopo.** AST de headings H1–H6 derivada do Markdown no Core; slug estável e
testado; regra declarada para headings duplicados na mesma nota; navegação com
scroll curto e destaque transitório respeitando `prefers-reduced-motion`;
autocomplete de headings ao criar o link, se couber no orçamento da 6.0.D;
heading inexistente tratado explicitamente — **nunca scroll para posição
aproximada**.

**Fora de escopo.** Outline (6.B.1, que consome esta AST). Embed de seção
(6.A.10).

**Dependências.** 6.A.2, 6.A.5.

**Contratos.** Posição nunca é linha: heading é identificado por slug + ordinal,
e edição de texto vizinho não pode mover o alvo.

**Riscos.** Slug divergente entre Core, GUI e TUI. Nota grande travando ao
navegar.

**Testes.** Headings duplicados; heading com acento, emoji, pontuação, só
números, vazio; heading que muda de texto; nota de 2 MB. Fixture de slug afirmada
pelas três implementações.

**Aceite.** Duplicados tratados por regra declarada e testada. Heading inexistente
produz diagnóstico claro e não navega. Navegação em nota grande medida.

### 6.A.9 — Referências de bloco

**Objetivo.** Um trecho granular tem identidade explícita, portável e legível.

**Escopo.** Sintaxe de ID de bloco da 6.0.B; comando "Copiar referência do bloco"
em menu de contexto ou gutter discreto; indexação apenas de blocos com ID
explícito; detecção de ID duplicado; link direto a bloco.

**Fora de escopo.** Gerar ID em massa em todas as linhas. Embed de bloco
(6.A.10). Posição de caractere como identidade.

**Dependências.** 6.0.B, 6.A.8.

**Contratos.** O ID vive no Markdown e é legível. Editar o texto do bloco não
invalida a referência. Editar texto adjacente não a quebra em silêncio.

**Riscos.** Poluir o arquivo do usuário com identificadores. Referência quebrando
ao mover o bloco.

**Testes.** Editar conteúdo do bloco → referência continua válida; apagar o bloco
→ referência vira não resolvida com diagnóstico; duplicar o bloco → ID duplicado
detectado; arquivo continua utilizável como Markdown comum em outro editor.

**Aceite.** Referência sobrevive à edição do conteúdo. IDs duplicados detectados.
Nenhum ID é criado sem ação do usuário.

**Parada.** Se a sintaxe escolhida gerar ruído visível que o usuário não pediu,
voltar à 6.0.B.

### 6.A.10 — Embeds e transclusão

**Objetivo.** Ver o conteúdo de outra nota, seção ou bloco dentro da atual, sem
duplicá-lo.

**Escopo.** Resolução reusando 6.A.1/6.A.8/6.A.9; renderização como visão
incorporada com origem identificável e caminho para abrir a fonte; atualização
quando a fonte muda; **detecção de ciclo e limite de profundidade**; política
para conteúdo pesado, mídia e embed aninhado.

**Fora de escopo.** Editar a fonte pelo embed. Copiar o texto embutido para o
arquivo atual.

**Dependências.** 6.A.5, 6.A.8, 6.A.9, 6.0.D.

**Contratos.** Uma única fonte de verdade: o embed lê, nunca escreve. Ciclo
A→B→A produz mensagem segura e compacta, jamais recursão.

**Riscos.** Explosão de memória em ciclo; nota de 2 MB embutida; embed dentro de
embed dentro de embed.

**Testes.** Ciclo direto A→A; ciclo A→B→A; cadeia até o limite e um além;
embed de nota inexistente, na lixeira, ambígua; fonte muda → visão atualiza;
fonte de 2 MB. Teste de memória com limite como asserção.

**Aceite.** Ciclos tratados com mensagem, nunca com travamento. Alterar a fonte
reflete. Nenhum dado duplicado no arquivo — fingerprint da nota que contém o
embed inalterado.

**Parada.** Se o limite de profundidade não puder ser provado por teste, a
feature está BLOCKED: recursão infinita numa nota é perda de sessão.

### 6.A.11 — Preview por mouse e por foco

**Objetivo.** Consultar o destino de um link sem sair da nota, com caminho de
teclado equivalente.

**Escopo.** Atraso intencional antes de abrir; conteúdo limitado (título, trecho,
metadados mínimos) reusando o pipeline de leitura e o renderizador seguro que o
`FlashcardPanel` já usa; cancelamento de leitura anterior; cache leve; abrir o
destino a partir do popover; equivalente de teclado; sanitização do conteúdo.

**Fora de escopo.** Editar no preview. Carregar a nota inteira quando um trecho
basta.

**Dependências.** 6.A.5, 6.A.10, 6.0.D.

**Contratos.** Nenhuma informação essencial existe só em hover. O popover
desaparece de modo previsível e nunca cobre permanentemente o ponto de leitura.

**Riscos.** Enxame de popovers ao mover o cursor; flicker; travar em nota grande.

**Testes.** Cursor atravessando dez links em 200 ms abre zero popovers. Foco de
teclado abre o mesmo conteúdo. Nota de 2 MB não congela. Leitura cancelada não
desenha. Conteúdo com HTML desconhecido é renderizado como texto seguro.

**Aceite.** Zero flicker medido. Paridade teclado/mouse provada por teste. Custo
de abertura medido e dentro do orçamento da 6.0.D.

### 6.A.12 — Menções não vinculadas

**Objetivo.** Sugerir texto que provavelmente se refere a uma nota, sem tocar em
nada até o usuário aceitar.

**Escopo.** Heurística determinística sobre nomes e aliases, com limites
declarados; apresentação com nota provável, trecho e justificativa; ação
"Vincular" com preview da modificação; ignorar ocorrência e ignorar candidato,
persistidos de forma simples; execução sob demanda ou em background com debounce.

**Fora de escopo.** Camada semântica (fica para 6.F, opcional e derivada).
Reescrever texto em segundo plano. Misturar com backlinks sem rótulo distinto.

**Dependências.** 6.A.7.

**Contratos.** Nenhum conteúdo muda sem ação explícita. Falso positivo é
ignorável e o ignore não vira ruído repetido.

**Riscos.** Processamento pesado durante a digitação; sugestão em massa
transformando a nota num formulário.

**Testes.** Nenhuma escrita sem aceite — fingerprint do store após uma sessão
inteira de sugestões ignoradas. Ignorar impede repetição. Desempenho de edição
medido antes e depois, com o delta como asserção.

**Aceite.** Zero mutação sem aceite explícito. Degradação de digitação não
perceptível, medida. Ignores sobrevivem a reinício.

**Parada.** Se a heurística exigir varredura a cada tecla, PARAR e mover para sob
demanda.

### 6.A.R — Auditoria adversarial da macrofase

**Objetivo.** Provar que a rede de links não perde, não inventa e não trava.

**Escopo.** Auditoria ofensiva no padrão da 4.3R e da 5.0D.4B/B.R: notas
adversariais, ciclos, Unicode, nomes colidentes, store danificado, índice
corrompido, nota de 2 MB, 20.000 notas, biblioteca antiga sem nenhum link.
Revisão independente que não aprova a si mesma.

**Aceite.** Zero regressão em Core, CLI, MCP, TUI e GUI. Todos os orçamentos de
6.0.C e 6.0.D como asserções verdes. Nenhum `.md` de uma biblioteca pré-6.A é
alterado por abrir, navegar ou indexar — provado por fingerprint de árvore
inteira. CI verde no SHA final.

**Parada.** Qualquer perda de conteúdo, qualquer mutação silenciosa, qualquer
orçamento estourado: BLOCKED, sem exceção e sem "trade-off aceitável".

---

## Fase 6.B — Navegação estrutural

**Features:** F09 outline, F18 histórico de navegação, F10 breadcrumbs,
F22 inspector. Todas reusam os mesmos IDs, headings e índices derivados da 6.A —
nenhuma recalcula o que já existe.

### 6.B.1 — Outline da nota

**Objetivo.** Navegação hierárquica pelos headings da nota atual.

**Escopo.** Consumir a AST de headings da 6.A.8; clicar e navegar; destaque
discreto da seção ativa durante a rolagem quando for tecnicamente seguro;
recolher níveis profundos com o estado guardado **como preferência de UI**;
drawer/overlay em viewport estreita conforme 6.0.D.

**Fora de escopo.** Alterar headings para construir o outline. Painel permanente
em nota estreita. Recalcular a árvore inteira a cada tecla.

**Dependências.** 6.A.8, 6.0.D.

**Componentes.** `ui/src/ui/`, `ui/src/editor/`, ponte; equivalente na TUI se a
6.0.D concluir que cabe.

**Contratos.** Estado visual nunca é persistido no Markdown.

**Riscos.** Jitter de layout durante a digitação; recomputação cara.

**Testes.** Outline acompanha edição com debounce, sem defasagem perceptível;
clique leva ao heading correto; zero jitter medido em digitação contínua;
nenhuma escrita no `.md`.

**Aceite.** Nenhum byte escrito. Custo por atualização medido. Painel some abaixo
da largura declarada na 6.0.D.

### 6.B.2 — Histórico de navegação (voltar/avançar)

**Objetivo.** Explorar links em profundidade sem perder o caminho.

**Escopo.** Pilha por contexto/janela — e "janela" aqui é **uma nota**, porque é
assim que este produto é feito; entrada modelada como `note_id` + localização
opcional + contexto mínimo; botões discretos no cabeçalho, desabilitados quando
não aplicáveis; atalhos; restauração de posição quando viável; limite de tamanho
em memória.

**Fora de escopo.** Confundir com histórico de versões (6.D). Persistir rastro de
navegação indefinidamente. Contar cada scroll como entrada.

**Dependências.** 6.A.5, 6.0.D.

**Componentes.** `ui/src/`, `src/app.rs` (a ativação de nota já existe pelo
caminho de `open_search_result`), `src/note_window.rs`.

**Contratos.** Abrir uma rota nova depois de voltar descarta o ramo "avançar".
Navegar nunca move `updated_at`.

**Riscos.** Semântica confusa quando a nota de destino é outra janela; vazamento
de memória na pilha.

**Testes.** Ordem de back/forward correta em sequência de 20 saltos; ramo forward
descartado; limite respeitado; posição restaurada sem salto errático; nota
destino fechada/na lixeira/apagada entre a ida e a volta.

**Aceite.** Ordem correta provada por teste. Limite como asserção. Nenhuma
escrita.

### 6.B.3 — Breadcrumbs contextuais

**Objetivo.** Contexto real de localização, sem inventar hierarquia.

**Escopo.** Auditar primeiro **quais fontes determinísticas de contexto este
produto realmente tem** — o store é plano (`notes/<uuid>.md`, sem pastas), então
"pasta atual" não existe aqui; sobram cadeia de navegação (6.B.2) e heading
atual (6.A.8). Definir semanticamente cada segmento antes de desenhar. Linha fina
e discreta, truncamento elegante, navegação por segmento quando houver destino.

**Fora de escopo.** Inventar hierarquia por IA. Misturar pasta, tag e backlink
numa falsa árvore. Barra alta e pesada.

**Dependências.** 6.B.2, 6.A.8, 6.0.D.

**Contratos.** Cada segmento corresponde a estado real. Nenhum caminho fictício.

**Riscos.** Sem hierarquia de pastas, o breadcrumb pode não ter o que mostrar —
e então a resposta honesta é não mostrá-lo.

**Testes.** Cada segmento navega para destino real; nome longo trunca; nota sem
contexto não exibe barra vazia.

**Aceite.** Zero caminho fictício. A auditoria de fontes de contexto está escrita
e, se a conclusão for que não há o que mostrar, **a feature é entregue como
"avaliada e deliberadamente não implementada"**, com a razão — no padrão que a
3.8 usou para renderização compacta de links.

### 6.B.4 — Note Inspector

**Objetivo.** Concentrar informação da nota num painel discreto, agregando o que
já existe.

**Escopo.** Modelo de view derivada somando fontes existentes: `created_at` /
`updated_at` (`model.rs`), contagem de palavras/caracteres via
`visible_text.rs`, links e backlinks (6.A.4), tags e properties
(`metadata.rs`), flashcards (`study.rs`), tarefas (`task.rs`), aliases (6.A.3).
Distinguir claramente campo editável de métrica derivada. Debounce na atualização.

**Fora de escopo.** Inventar métrica de "qualidade" ou pontuação. Dashboard
permanente. Duplicar controle que já tem lugar melhor. Recalcular o que o índice
já tem.

**Dependências.** 6.A.7, 6.0.D.

**Contratos.** Nenhuma métrica exige mutação do arquivo. Onde o dado não é
confiável, **documentar a limitação em vez de fabricar** — `created_at` é
`Option` e uma nota antiga pode não tê-lo (`model.rs:58`); o inspector diz
"desconhecido", não uma data inventada.

**Riscos.** Virar dashboard; recalcular tudo a cada tecla.

**Testes.** Métricas batem com conteúdo real em fixtures; painel some por
completo; nota sem `created_at` mostra desconhecido; nenhuma escrita.

**Aceite.** Números conferem com o conteúdo. Painel ocultável. Zero mutação.

---

## Fase 6.C — Captura e criação

**Features:** F11 templates, F12 comandos "/", F13 Quick Capture + Inbox.

### 6.C.1 — Motor de templates no Core

**Objetivo.** Templates como Markdown comum, com substituição de variáveis
mínima e segura, no Core.

**Escopo.** Decidir onde o template mora — `StorePaths` hoje tem `notes/`,
`trash/`, `backups/`, `assets/` e nada mais; um diretório novo exige entrada no
manifesto de backup (C-5, manifesto v4) e prova de restauração, **ou** templates
são notas marcadas por tag/property e nada novo entra no store. Motor de
substituição com conjunto fechado de variáveis (título, data), escaping definido,
comportamento documentado para variável desconhecida. CRUD de templates.

**Fora de escopo.** Linguagem de script. Executar shell, JavaScript ou qualquer
código. Vínculo oculto entre nota criada e modelo. Slash commands (6.C.3).

**Dependências.** 6.0.D para a superfície; nenhuma da 6.A.

**Componentes.** `noteit-core`, possivelmente `backup.rs` (manifesto v4),
`storage.rs` (`StorePaths`), `noteit-cli`.

**Contratos.** Nenhum código arbitrário é executado — isso é requisito de
segurança, não preferência. Variável desconhecida tem comportamento documentado e
testado. Aplicar template nunca sobrescreve nota existente sem fluxo explícito.

**Riscos.** Diretório novo fora do backup = dado do usuário que um restore não
traz de volta. Motor de variáveis virando avaliador.

**Testes.** Substituição correta; variável desconhecida; template vazio;
template com `{{` sem fechar; tentativa de injeção; round-trip Markdown;
**se houver diretório novo: backup + restore em segunda árvore XDG vazia**, no
padrão que a 3.12R provou para `assets/`.

**Aceite.** Markdown previsível. Zero execução de código. Se store novo:
manifesto v4 com restauração provada.

**Parada.** Se o motor precisar de condicional, laço ou chamada, PARAR: isso é
linguagem de programação e o escopo a proíbe na primeira versão.

### 6.C.2 — Templates nas interfaces

**Objetivo.** Criar nota a partir de template em 1–2 ações.

**Escopo.** Galeria/lista compacta com preview e busca na GUI; comando
equivalente na CLI e na TUI; template padrão por ação explícita, se a auditoria
mostrar que faz sentido.

**Fora de escopo.** Modal gigante. Vínculo com o modelo depois de criada.

**Dependências.** 6.C.1, 6.0.D.

**Testes.** Criar de template em GUI, CLI e TUI produz o mesmo Markdown —
fixture cruzada. Cancelar não cria nada.

**Aceite.** Mesma saída nas três interfaces, provada por fixture.

### 6.C.3 — Comandos "/" no editor

**Objetivo.** Inserção rápida de estruturas que **já têm contrato** no Note-it.

**Escopo.** Palette contextual ao digitar `/` em contexto válido; registry
declarativo de comandos, separando descoberta da transformação textual; filtro
por texto; teclado integral; toda inserção numa transação de editor. Conjunto
inicial restrito ao que existe: heading, checklist, callout (3.5), código (3.5),
tabela, comentário (3.5), flashcard (3.13), data, imagem (3.12), template (6.C.1).

**Fora de escopo.** Bloco invisível ou proprietário. Interceptar `/` dentro de
código, URL ou math sem regra clara. Virar central de recursos fora de notas.

**Dependências.** 6.C.1, 6.0.B (a regra de contexto válido mora junto com a
precedência de sintaxe).

**Componentes.** `ui/src/editor/`, `ui/src/ui/`.

**Contratos.** Undo restaura exatamente o anterior. Todo comando produz Markdown
válido.

**Riscos.** `/` em URL, em fence, em expressão matemática (`10/2`), em caminho.

**Testes.** `/` em URL, em fence, em code span, em math, em texto normal; undo
depois de cada comando; Markdown resultante round-trip; teclado completo.

**Aceite.** Undo exato provado para todo comando do registry. Zero interferência
em URL, código e matemática — caso por caso em teste.

### 6.C.4 — Quick Capture e Inbox

**Objetivo.** Capturar uma ideia em segundos sem interromper o fluxo.

**Escopo.** Atalho global configurável reusando o mecanismo que já existe — o
Note-it é `GApplication` de instância única com despachante CLI e GActions
(`note-it new`, `toggle-layer`, o serviço D-Bus de ativação a frio da 5.0E-GUI);
janela mínima; título opcional e conteúdo rápido; gravação pela mesma camada
segura (`WriteOperation::CreateNote`, publicação atômica condicionada à ausência);
local de Inbox definido — **tag/property, não diretório novo**, salvo decisão em
contrário com manifesto v4; confirmação discreta; abrir a captura completa depois.

**Fora de escopo.** Segundo daemon ou processo. Capturar clipboard, áudio ou
contexto externo (o AutoPaste da 3.11 já é outra feature e continua sendo).
Manter texto só em memória.

**Dependências.** 6.C.1 (se a captura aplicar template), 6.0.D.

**Componentes.** `src/app.rs`, `src/cli.rs`, `src/layer_shell.rs`,
`resources/*.desktop`/`.service`, `noteit-core/src/write.rs`.

**Contratos.** Captura nunca sobrescreve nota existente. Falha de gravação é
informada e o texto não desaparece. Atalho repetido não deixa processo órfão.

**Riscos.** Segundo daemon. Roubo de foco. Texto perdido em falha de escrita.

**Testes.** Cem invocações seguidas sem processo órfão — no padrão do harness de
isolamento da 3.7R, com barramento D-Bus privado. Falha de escrita simulada
preserva o texto. Captura em store cheio. Colisão de identificador.

**Aceite.** Zero processo órfão medido. Zero sobrescrita. Texto recuperável em
falha, provado.

**Parada.** Se a captura exigir um segundo processo permanente, PARAR: contraria
"daemon ocioso não trabalha", que é propriedade medida deste produto desde a
Fase 2.

---

## Fase 6.D — Segurança editorial e refatoração

**Features:** F14 histórico de versões, F15 extrair seleção, F16 mesclar notas.

**Por que nesta ordem.** C-4: não existe operação transacional entre duas notas.
Pelo §9 do escopo, extract e merge estão **BLOCKED até existir estratégia
compensatória testável**. 6.D.1 é essa estratégia.

### 6.D.1 — Histórico de versões: contrato e armazenamento

**Objetivo.** Poder voltar uma nota a um estado anterior, com política explícita.

**Escopo.** Auditar primeiro a persistência atual — `atomic_file.rs`,
`save_note_atomic`, o ponto de commit da 3.4R.2, e o que a lixeira (3.9) e o
backup (3.9/3.12R) já cobrem, para não construir um terceiro mecanismo de
recuperação. Decidir snapshot vs. delta, gatilho, retenção e limite de
armazenamento, em ADR. Impacto em SSD medido, porque este produto trata escrita
como custo real. Formato compatível com o backup: **artefato novo exige manifesto
v4 e restauração provada** (C-5).

**Fora de escopo.** UI (6.D.2). Cópia integral a cada tecla. Confundir com backup
externo. Apagar histórico em silêncio por limite.

**Dependências.** 6.0.C (orçamento), nenhuma da 6.A.

**Componentes.** `noteit-core` (módulo novo), `backup.rs`, `storage.rs`.

**Contratos.** Restaurar **cria um novo estado atual** e preserva o histórico
anterior — nunca destrói a versão que estava valendo. Retenção é documentada e
testada. Falha de histórico nunca bloqueia o salvamento de uma nota.

**Riscos.** Degradar o salvamento normal; encher o disco; histórico divergindo do
arquivo.

**Testes.** Retenção como asserção; restaurar preserva a versão anterior;
histórico corrompido não impede salvar; custo de salvamento medido antes/depois
com o delta como asserção; backup + restore trazem o histórico se ele for parte
do store.

**Aceite.** Restauração nunca perde a versão corrente anterior. Política de
retenção testada. Degradação de salvamento medida e dentro do orçamento.

**Parada.** Se o histórico atrasar o salvamento de forma perceptível, PARAR: o
editor tem prioridade sobre recurso derivado (§12 do mandato).

### 6.D.2 — Histórico de versões: interface

**Escopo.** Timeline compacta com data/hora; ver versão; diff; restaurar como
ação deliberada, **visualmente separada** de "ver versão", com confirmação.

**Fora de escopo.** Restauração destrutiva sem confirmação. Merge de versões.

**Dependências.** 6.D.1, 6.0.D.

**Testes.** Restaurar pede confirmação; cancelar não altera nada; diff
corresponde ao conteúdo real; nota sem histórico mostra estado vazio explicativo.

**Aceite.** Nenhuma restauração sem confirmação. Preview corresponde ao aplicado.

### 6.D.3 — Extrair seleção para nova nota

**Objetivo.** Trecho selecionado vira nota nova e o original vira link.

**Escopo.** Operar só sobre seleção explícita; título proposto editável antes de
concluir; **criar a nota e confirmar a persistência antes de tocar no original**;
compensação se qualquer etapa falhar; preservar a formatação Markdown da seleção.

**Fora de escopo.** Inferir título e concluir sem revisão. Apagar a seleção antes
da confirmação.

**Dependências.** 6.D.1, 6.A.5.

**Componentes.** `noteit-core/src/write.rs` (operação composta), GUI, TUI, CLI.

**Contratos.** Ordem é criar → confirmar → substituir. Falha parcial deixa o
texto original intacto — nunca uma referência quebrada e um trecho perdido.

**Riscos.** Falha entre as duas escritas; seleção multilinha com lista, heading,
código, imagem gerenciada, flashcard.

**Testes.** Falha injetada em cada etapa, verificando que nada se perde; seleção
com lista, heading aninhado, fence, tarefa, imagem, callout, flashcard;
undo/restore previsível; link final aponta exatamente para a nota criada.

**Aceite.** Nenhuma falha intermediária perde texto — provado com injeção de
falha em cada ponto. Formatação preservada por round-trip.

**Parada.** Se a compensação não puder ser provada por teste, BLOCKED (§9 do
escopo).

### 6.D.4 — Mesclar notas

**Objetivo.** Consolidar notas redundantes com revisão e sem perder fonte.

**Escopo.** Selecionar fontes e destino, ou criar nota consolidada; preview do
resultado antes de aplicar; separadores e ordem explícitos; destino das fontes
após o merge — **manter, arquivar ou mover para a lixeira, nunca apagar**;
conflito de metadata, títulos e IDs coberto; atualização de wikilinks apenas se
houver mecanismo seguro e confirmação explícita.

**Fora de escopo.** Concatenar e apagar num clique. Deduplicação semântica
destrutiva. Reescrever referências globais em silêncio.

**Dependências.** 6.D.1, 6.D.3, 6.E.1 (para "arquivar" existir como estado),
6.A.4 (para saber quem aponta para as fontes).

**Contratos.** Nenhuma fonte é perdida por padrão. Preview corresponde ao salvo.
Falha parcial deixa estado recuperável.

**Riscos.** O pior risco de perda de dados da Fase 6 inteira.

**Testes.** Falha injetada em cada etapa; conflito de tags, properties e aliases;
merge de nota com imagens gerenciadas; merge de nota com flashcards; preview
comparado byte a byte com o resultado; fontes intactas quando a política é
"manter".

**Aceite.** Preview idêntico ao resultado. Nenhuma fonte apagada automaticamente.
Toda falha parcial recuperável, provada por injeção.

**Parada.** BLOCKED enquanto 6.D.1 não estiver fechada e provada.

---

## Fase 6.E — Organização não destrutiva

**Features:** F17 favoritas/fixadas/arquivadas, F21 saved views.

### 6.E.1 — Estados: favorita, fixada, arquivada

**Objetivo.** Três estados com semântica separada, reusando metadata existente.

**Escopo.** Definir: favorita = importante; fixada = acesso prioritário;
arquivada = fora do fluxo normal, mas preservada. Persistir em `properties`
(`metadata.rs`) — **reusar, não criar taxonomia nova**. Filtro por estado.
Paridade GUI/CLI/TUI. Ícones pequenos e monocromáticos; fixadas no topo com
separação discreta; arquivadas fora da lista padrão mas **encontráveis na busca
global com opção clara**.

**Fora de escopo.** Misturar com lixeira. Dezenas de estados derivados. Esconder
arquivada da busca sem opção.

**Dependências.** Nenhuma da 6.A.

**Contratos.** Arquivar nunca equivale a deletar. Estados sobrevivem a reinício.

**Riscos.** Competir com tags — que é exatamente o que o escopo proíbe.

**Testes.** Estados sobrevivem a reinício; filtro consistente entre as três
interfaces; arquivada continua encontrável; arquivar não move arquivo.

**Aceite.** Zero movimentação de arquivo. Semântica idêntica nas três interfaces,
provada por fixture.

### 6.E.2 — Modelo de query unificado

**Objetivo.** Um único modelo de filtro reusado por GUI, CLI e views.

**Escopo.** Estender `filter.rs` — hoje `NoteFilter` é AND de tags e properties —
com estado (6.E.1), texto e período, mantendo a semântica existente intacta.
Serialização versionada da definição.

**Fora de escopo.** DSL complexa. Duplicar o sistema de filtro existente.
Snapshot de resultados como fonte de verdade.

**Dependências.** 6.E.1.

**Componentes.** `noteit-core/src/filter.rs`, `search.rs`, `noteit-cli`.

**Contratos.** Compatibilidade: todo uso atual de `NoteFilter` (CLI 4.0D, MCP,
contexto 4.2) continua funcionando sem alteração de comportamento.

**Testes.** Regressão completa dos filtros existentes de CLI e MCP; serialização
round-trip; definição de versão futura desconhecida recusada com mensagem.

**Aceite.** Zero regressão nos consumidores atuais, provada pelas suítes de
`noteit-cli` e `noteit-mcp` sem alteração de expectativa.

### 6.E.3 — Saved Views

**Objetivo.** Salvar consultas reutilizáveis, sem pastas artificiais.

**Escopo.** Persistir **apenas a definição**; resultados dinâmicos; renomear,
reordenar e excluir sem tocar em nota; deixar evidente que é visão, não pasta.
Local de persistência: se for artefato novo no store, manifesto v4 e restauração
provada (C-5).

**Fora de escopo.** Mover arquivo ao adicionar/remover nota da view. Snapshot de
resultados.

**Dependências.** 6.E.2, 6.0.D.

**Testes.** Mudar property de uma nota muda o resultado da view; excluir view não
afeta nota alguma — fingerprint do store; definição serializável e versionável;
backup/restore traz as views se forem parte do store.

**Aceite.** Excluir view não altera um byte de nenhuma nota. Resultados dinâmicos
provados. Definição versionada.

---

## Fase 6.F — Inteligência derivada

**Features:** F19 notas relacionadas, F20 duplicadas/parecidas.

**Restrição da macrofase.** Reusar `noteit-core::context` e
`noteit-core::semantic`, que são a autoridade única desde a 4.3E. **Nenhum
segundo motor de embedding, busca ou similaridade.** Toda sugestão é não
destrutiva.

### 6.F.1 — Notas relacionadas

**Objetivo.** Sugerir notas semanticamente próximas da atual, sem tocar no
conteúdo.

**Escopo.** Usar a nota como consulta — o motor hoje responde a
`ContextRequest { query, filter, ... }`, então a subfase precisa definir **como
uma nota vira consulta** (trecho, resumo lexical, ou vetor do próprio documento)
e registrar a escolha. Limite pequeno (3–5), ranking consistente, execução após
debounce/idle ou sob demanda, cache por revisão da nota, degradação graciosa
quando o provider semântico não estiver disponível — o motor já tem
`SemanticStatus::Unavailable` e fallback lexical declarado.

**Fora de escopo.** Segundo sistema de embeddings. Gravar links sugeridos.
Mostrar score técnico como verdade. Bloquear a abertura da nota esperando ranking.

**Dependências.** 6.A.5 (para "criar wikilink manualmente" a partir da sugestão).

**Componentes.** `noteit-core/src/context.rs`, `semantic.rs`, GUI, CLI, TUI.

**Contratos.** Abertura de nota não depende da busca relacionada. Provider
indisponível degrada, nunca quebra. `Reason` já é um conjunto fechado de
observações auditáveis, não um score — a UI apresenta motivo, não número.

**Riscos.** Latência entrando no caminho de abertura; sugestão apresentada como
certeza.

**Testes.** Provider ausente, provider lento, provider com erro — editor continua
funcional nos três. Tempo de abertura da nota medido com e sem a feature, com o
delta como asserção. Nenhuma escrita — fingerprint.

**Aceite.** Funciona com provider indisponível. Zero mutação de arquivo. Abertura
de nota não regride, medida.

### 6.F.2 — Notas duplicadas ou muito parecidas

**Objetivo.** Apontar provável redundância e oferecer comparação antes de
qualquer consolidação.

**Escopo.** Usar o índice semântico/lexical existente; pares candidatos com
preview e diferenças; ações "Comparar", "Ignorar" e — **somente depois que 6.D.4
estiver fechada** — "Mesclar"; ignores persistidos de forma simples; thresholds
definidos empiricamente sobre dataset de teste e **documentados**; análise
assíncrona ou sob demanda.

**Fora de escopo.** Apagar, mesclar ou renomear automaticamente. Varredura O(n²)
ingênua. Afirmar "duplicada" sem sinal forte — a linguagem é "possivelmente
parecidas".

**Dependências.** 6.F.1; 6.D.4 para habilitar a ação de merge.

**Riscos.** Varredura bloqueante em biblioteca grande; falso positivo levando a
merge destrutivo.

**Testes.** Thresholds sobre dataset versionado, no padrão de
`docs/retrieval-corpus.json`; biblioteca de 20.000 notas sem varredura
bloqueante; ignorar impede repetição; nenhuma operação destrutiva sem confirmação.

**Aceite.** Thresholds documentados com o dataset que os produziu. Zero operação
destrutiva automática. Biblioteca grande medida.

---

## Fase 6.G — Validação global e promoção da versão diária

**Esta é a única macrofase autorizada a tocar versão, packaging e instalação — e
somente quando o prompt daquela subfase autorizar nominalmente.**

### 6.G.1 — Regressão integral e gate da GUI

**Objetivo.** Provar que a Fase 6 não regrediu nada do que já funcionava.

**Escopo.** `scripts/check` completo; suítes de Core, CLI, MCP, TUI, embed,
agent-bridge e frontend; todos os gates de fronteira; e o **gate de regressão da
GUI**, adaptado à arquitetura real deste produto:

1. inicia pelo despachante de instância única e pela ativação a frio do
   barramento (5.0E-GUI);
2. resolve o store correto por `StorePaths`;
3. lista notas e abre uma existente;
4. cria, edita e persiste;
5. fecha e reabre sem perda, com `updated_at` obedecendo a 3.4R;
6. move para a lixeira e restaura byte a byte (3.9);
7. busca global (`Ctrl+K`), localizar e substituir (`Ctrl+F`/`Ctrl+H`);
8. atalhos: `Ctrl+N`, `Ctrl+W`, `Ctrl+Shift+M`, `Ctrl+Shift+Space`, zoom;
9. renderiza conteúdo existente — blocos inteligentes, matemática, conversões,
   imagens gerenciadas, flashcards, tarefas;
10. preserva tema, escala de interface, papel, cor e geometria;
11. timer e AutoPaste inalterados;
12. **nenhuma nota antiga é alterada** — fingerprint de árvore inteira de uma
    biblioteca pré-Fase 6, antes e depois de uma sessão completa.

Tudo em store descartável, com `scripts/note-it-isolated` (barramento D-Bus
privado **e** XDG isolado).

**Fora de escopo.** Qualquer build de release. Qualquer instalação.

**Aceite.** Doze itens verdes. Zero regressão. CI verde no SHA.

**Parada.** Um único item vermelho bloqueia a 6.G.2.

### 6.G.2 — Migração e compatibilidade com biblioteca antiga

**Objetivo.** Provar que uma biblioteca criada antes da Fase 6 continua correta.

**Escopo.** Fixture de biblioteca antiga — notas sem os campos novos, front
matter com YAML de terceiros, notas sem `created_at`, notas com imagens, com
flashcards, com tarefas, danificadas. Verificar: abrem; nada é reescrito ao
abrir; campos ausentes degradam declaradamente; se houver campo novo no front
matter, a migração é testada nos dois sentidos e o rollback é descrito.

**Fora de escopo.** Migração automática irreversível sobre dados reais.

**Contratos.** Nenhuma migração roda sobre a biblioteca real do usuário sem
aprovação explícita e sem backup verificado antes.

**Aceite.** Biblioteca antiga íntegra por fingerprint. Toda migração reversível
ou explicitamente declarada como irreversível com backup obrigatório antes.

### 6.G.3 — Build candidato e validação isolada

**Objetivo.** Um artefato identificável, validado sem tocar na instalação diária.

**Escopo.** Build release do commit exato; execução em store descartável;
roteiro manual das classes de nota do gate 6.G.1; registro de SHA, data e
conteúdo. **`candidate build` não é `release build` e não substitui nada.**

**Fora de escopo.** `makepkg -i`, `pacman -U`, qualquer escrita em `/usr`,
qualquer alteração de atalho do sistema.

**Aceite.** Artefato roda a partir de um diretório qualquer, contra um store
descartável, sem que a instalação em `/usr/bin` seja tocada — verificável porque
o pacote instalado continua respondendo `pacman -Qi` com a versão antiga.

### 6.G.4 — Promoção: packaging, upgrade e nova baseline

**Objetivo.** Transformar um conjunto aprovado na nova versão diária.

**Pré-condições, todas obrigatórias.** Todas as macrofases incluídas em PASS;
CI verde no SHA final; nenhuma regressão conhecida aberta; documentação
sincronizada; migrações testadas (6.G.2); 6.G.1 e 6.G.3 verdes; **e a resposta
escrita para: "se a nova versão der problema, como volto sem perder minhas
notas?"**. Sem essa resposta, a release não está pronta.

**Escopo.** Número de versão decidido conscientemente **neste momento e não
antes** — nada nesta auditoria inventa versão; `PKGBUILD` atualizado com o commit
e a contagem; build reproduzível; teste de **instalação limpa** e, principalmente,
teste de **upgrade a partir da versão instalada**, que é o cenário real;
verificação de que notas, preferências, configuração, estado, atalhos e tema
sobrevivem; rollback ensaiado a partir do pacote anterior; rastreabilidade
registrada entre versão instalada, commit, build e artefato (C-9 e §18 do
complemento).

**Fora de escopo.** Qualquer coisa que não seja a promoção. Publicar sem os
pré-requisitos.

**Contratos.** A atualização não apaga nem substitui notas, preferências,
configuração, atalhos ou estado — o `PKGBUILD` atual já garante isso por
construção (escreve só em `$pkgdir`) e a garantia tem que continuar verdadeira.

**Riscos.** Upgrade que perde dado; rollback impossível; versão publicada só
porque compila.

**Testes.** Instalação limpa em chroot; upgrade da versão anterior para a nova
com store real **copiado**, nunca o original; rollback para a versão anterior com
as notas intactas.

**Aceite.** Upgrade testado, rollback ensaiado, dados preservados por fingerprint,
rastreabilidade registrada. **Só então** a versão nova passa a ser a baseline
protegida, e este roadmap é atualizado para dizê-lo.

**Parada.** Qualquer pré-condição faltando: não promove. "Compila" não é critério.

### 6.G.R — Fechamento do ciclo

Auditoria final no padrão da 4.0R, 4.1R1 e 4.3R: revisão independente que não
aprova a si mesma, zero violação de fronteira, zero regressão nas cinco
superfícies, CI verde no SHA final, árvore limpa, e o roadmap dizendo a verdade
sobre o que foi entregue e o que foi deliberadamente deixado de fora.

---

## 6.MAPA — As 22 features do escopo mestre, auditadas contra o código

Classificação verificada em `1186224` por inspeção de implementação e testes, não
por nome de arquivo. Buscas por `wikilink`, `backlink`, `transclu`, `outline`,
`breadcrumb`, `favorit`, `saved view`, `inspector`, `quick capture`,
`version history` e `template` retornaram **zero ocorrência de produção**; os 153
hits de `alias` são aliases de unidade (conversões), de linguagem de bloco de
código, de subcomando da CLI e de provider de embedding, e os 70 de `pinned` são
digest fixado e geometria de janela — nenhum deles é alias de nota.

**Uma etiqueta por feature.** A primeira redação desta tabela misturava o estado
do código com a razão da dependência — `AUSENTE — depende de fundação`,
`AUSENTE — conflitante`, `AUSENTE — BLOCKED` — e o resumo em prosa contou cinco
parciais onde a tabela listava seis. A etiqueta agora diz **uma** coisa: o que
existe no código hoje. Por que ela está assim e o que a destrava ficam nas
colunas de evidência e de destino, que toda linha já tem.

- **EXISTENTE** — entregue e em uso.
- **PARCIAL** — há código em produção que entrega parte da feature.
- **AUSENTE** — não há código de produção para ela.
- **BLOCKED** — não pode ser construída com segurança enquanto faltar uma
  primitiva, e o §9 do escopo mestre proíbe construí-la assim mesmo.
- **PRECISA DE AUDITORIA MAIS PROFUNDA** — não dá para classificar nem
  especificar antes de responder uma pergunta aberta sobre o produto.

| # | Feature | Estado | Evidência | Onde entra |
| --- | --- | --- | --- | --- |
| F01 | Wikilinks | **AUSENTE** | Nenhum parser, nenhuma sintaxe, nenhum resolvedor | 6.0.A, 6.0.B, 6.A.1, 6.A.2, 6.A.5, 6.A.6 |
| F02 | Backlinks | **AUSENTE** | Sem índice de relações; ADR-027 recusa índice (C-3) | 6.0.C, 6.A.4, 6.A.7 |
| F03 | Aliases | **AUSENTE** | `NoteProperties` é chave→`String` única (`metadata.rs:189`); alias precisa de lista | 6.0.A.2, 6.A.3 |
| F04 | Menções não vinculadas | **AUSENTE** | — | 6.A.12 |
| F05 | Links para headings | **AUSENTE** | Nenhuma AST de headings no Core | 6.A.8 |
| F06 | Referências de bloco | **AUSENTE** | — | 6.0.B, 6.A.9 |
| F07 | Embeds / transclusão | **AUSENTE** | — | 6.A.10 |
| F08 | Preview em hover/foco | **AUSENTE** | `FlashcardPanel` já renderiza fragmento seguro com `DOMSerializer` | 6.A.11 |
| F09 | Outline | **AUSENTE** | Headings existem no Markdown, não como AST consultável | 6.B.1 |
| F10 | Breadcrumbs | **PRECISA DE AUDITORIA MAIS PROFUNDA** | O store é plano (`notes/<uuid>.md`); não há pasta que sirva de segmento | 6.B.3 |
| F11 | Templates | **AUSENTE** | Nenhum store de templates; `StorePaths` tem 4 diretórios e nenhum é este | 6.C.1, 6.C.2 |
| F12 | Comandos "/" | **AUSENTE** | Todos os hits de `slash` são o operador de divisão do motor matemático | 6.C.3 |
| F13 | Quick Capture + Inbox | **PARCIAL** | `note-it new`, instância única, GActions e ativação a frio D-Bus já existem; falta a janela de captura e o conceito de Inbox | 6.C.4 |
| F14 | Histórico de versões | **PARCIAL** | Lixeira recuperável (3.9) e 7 snapshots de backup (3.9/3.12R) existem; histórico **por nota** não | 6.D.1, 6.D.2 |
| F15 | Extrair para nova nota | **BLOCKED** | Sem operação transacional entre duas notas (C-4) | 6.D.3, após 6.D.1 |
| F16 | Mesclar notas | **BLOCKED** | Mesma causa (C-4) | 6.D.4, após 6.D.1 |
| F17 | Favorita/fixada/arquivada | **AUSENTE** | `metadata.rs` properties suporta o dado; falta semântica e UI | 6.E.1 |
| F18 | Histórico de navegação | **AUSENTE** | `open_search_result` já ativa, abre, expande e rola até a nota | 6.B.2 |
| F19 | Notas relacionadas | **PARCIAL** | `context::retrieve` com BM25, canal semântico, `Reason` auditável e degradação declarada (4.2/4.3); falta nota-como-consulta e superfície | 6.F.1 |
| F20 | Duplicadas/parecidas | **PARCIAL** | Mesmo motor; faltam thresholds, pares e UI | 6.F.2 |
| F21 | Saved Views | **PARCIAL** | `filter.rs` `NoteFilter` (AND de tags e properties) é o núcleo; falta estado, texto, período e persistência da definição | 6.E.2, 6.E.3 |
| F22 | Note Inspector | **PARCIAL** | Todos os dados existem (`model`, `metadata`, `task`, `study`, `visible_text`); falta o agregado e o painel. `created_at` é `Option` e a limitação tem que ser dita, não fabricada | 6.B.4 |

### Totais

| Classificação | Quantidade | Features |
| --- | --- | --- |
| EXISTENTE | 0 | — |
| PARCIAL | 6 | F13, F14, F19, F20, F21, F22 |
| AUSENTE | 13 | F01, F02, F03, F04, F05, F06, F07, F08, F09, F11, F12, F17, F18 |
| BLOCKED | 2 | F15, F16 |
| PRECISA DE AUDITORIA MAIS PROFUNDA | 1 | F10 |
| **Total** | **22** | — |

**Nenhuma feature desapareceu e nenhuma aparece duas vezes.** 0 + 6 + 13 + 2 + 1
= 22, e o conjunto das cinco linhas é exatamente F01–F22.

Correção registrada: a redação anterior desta seção dizia "cinco parciais" e
"catorze ausentes" enquanto a tabela listava seis parciais. A tabela estava
certa quanto às evidências e o resumo estava errado quanto à contagem; as
etiquetas foram reduzidas a uma por feature e os totais passaram a ser
derivados da tabela, não escritos ao lado dela.

## 6.ADR — Decisões arquiteturais obrigatórias antes de implementar

**ADR-061 e ADR-062 foram escritas** no fechamento do Ciclo 5 e já estão em
`docs/decisions.md`: a direção de produto (GUI principal, Core canônico,
paridade semântica ≠ paridade visual) e a política de versionamento e promoção.
Entre elas, ADR-061 resolve o conflito C-1 — `docs/vision.md` foi emendado na
frase mínima, e a reserva que este roadmap tinha feito para uma ADR só sobre a
visão deixou de ser necessária.

As nove reservas da Fase 6 foram deslocadas para **ADR-063 a ADR-070** por causa
disso. Delas, a ADR-063 foi escrita na 6.0.A; as demais continuam sendo reservas
de ADRs **ainda não escritas**. Nenhum número já publicado mudou, e escrever ADR
superficial só para preencher documentação continua proibido pelo §17 do
mandato.

| ADR | Assunto | Bloqueia | Origem |
| --- | --- | --- | --- |
| ADR-061 | GUI principal, Core canônico, paridade semântica ≠ visual | — | **escrita** |
| ADR-062 | Versionamento `0.MINOR.PATCH` e promoção da versão diária | — | **escrita** |
| ADR-063 | Identidade nomeável da nota | Toda a 6.A | C-2, 6.0.A — **escrita** |
| ADR-064 | Semântica e formato de alias | 6.A.3 | C-2, 6.0.A.2 |
| ADR-065 | Gramática de wikilink, seção, bloco e embed | 6.A.2 em diante | 6.0.B |
| ADR-066 | Índice de relações e revisão de ADR-027 com número | 6.A.4, 6.A.7 | C-3, 6.0.C |
| ADR-067 | Superfície gráfica: painéis numa janela de nota | 6.A.5 em diante | 6.0.D |
| ADR-068 | Política de histórico de versões e retenção | 6.D.1 | 6.D.1 |
| ADR-069 | Operação composta entre notas e compensação | 6.D.3, 6.D.4 | C-4 |
| ADR-070 | Artefatos novos no store e manifesto de backup v4 | 6.C.1, 6.D.1, 6.E.3 | C-5 |

## 6.GATES — O mínimo que cada macrofase tem que provar

O `scripts/check` é a autoridade e nenhuma fase restata a lista. Toda macrofase
da Fase 6 fecha com:

- `scripts/check rust` completo — `ci-parity`, `rust-format`, `rust-check`,
  `rust-clippy -D warnings`, os sete gates de fronteira, e as suítes de core,
  cli, mcp, embedding, embed, remote, tui, agent-bridge e workspace;
- `scripts/check frontend` completo — install, lint, test, build;
- CI remoto verde **no SHA de fechamento**, com o link registrado, como todas as
  fases 4 e 5 já fazem;
- árvore limpa e `Cargo.lock`/`pnpm-lock.yaml` byte-idênticos, salvo dependência
  aprovada nominalmente no prompt da fase;
- fingerprint do store real idêntico antes e depois de qualquer execução manual;
- para macrofases que tocam a GUI: o gate de regressão de doze itens da 6.G.1;
- para subfases com orçamento de desempenho: o orçamento como **asserção de
  teste**, nunca como comentário.

Nenhuma fase é PASS por "funciona na máquina", e nenhum teste tem expectativa
ajustada para deixar o CI verde.

## Integração futura com o GholsOS

O repositório `TheGhols/GholsOS` foi iniciado como uma camada de integração pessoal que, no futuro, conectará Note-it, Diamond, Sodiz e outros aplicativos. O GholsOS Core ainda não existe. O Note-it foi identificado como o primeiro e mais preparado candidato para essa integração externa, graças ao seu Core bem definido, CLI madura, servidor MCP com 16 ferramentas, contratos internos estritos e suíte abrangente de testes. Nenhuma integração prática ou código foi iniciado nesta fase.

### Pré-requisitos (o que precisa existir ANTES de qualquer trabalho de integração real)
1. **GholsOS Core:** Registry unificado, arquitetura de identidade de objetos e barramento de eventos.
2. **Contrato mínimo de objetos:** Definição formal dos tipos `Note`, `Document` e `Link` no repositório `TheGhols/GholsOS`.

### Candidatos a Mapeamento (registro conceitual preliminar, sem implementação)
- **Note-it Note → Objeto `Note` do GholsOS:**
  - Mapeamento de campos: `id`, `source = "note-it"`, `title`, `text` (corpo Markdown), `created_at`, `updated_at`, `links`.
- **Eventos candidatos:**
  - `note.created`
  - `note.updated`

### Restrição Arquitetural Explícita
O Note-it permanece o único dono e autoridade sobre os seus arquivos Markdown no disco local (`StorePaths`). O GholsOS nunca duplica o conteúdo em armazenamento secundário ou proprietário, limitando-se a referenciar as entidades via identificador e fonte (`id`, `source`).

- [ ] **não iniciado — aguardando Fase G2 do GholsOS (contrato mínimo)**
