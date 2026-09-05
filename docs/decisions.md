# Registros de decisão de arquitetura (ADRs)

## ADR-001: Separação do Shell Nativo e do Editor Web WYSIWYG
- **Decisão:** Use Rust + GTK4 + `gtk4-layer-shell` para o ciclo de vida da janela nativa e WebKitGTK 6.0 incorporação Tiptap/ProseMirror para o editor.
- **Justificativa:** O suporte nativo Wayland Layer Shell não está disponível no Electron ou Tauri padrão sem ponte C/Rust de baixo nível. GTK4 e WebKitGTK 6.0 fornecem renderização nativa de Wayland com pouca sobrecarga de memória, enquanto Tiptap fornece um mecanismo de editor WYSIWYG rico e modular.

## ADR-002: Arquivos Markdown individuais para persistência de notas
- **Decisão:** Armazene cada post-it como um arquivo `.md` separado com YAML front matter nomeado por UUID.
- **Justificativa:** Garante propriedade de dados, portabilidade, facilidade de backup e interoperabilidade com outras ferramentas, evitando arquivos de banco de dados com ponto único de falha.

## ADR-003: Desacoplamento do estado da UI
- **Decisão:** Armazene as coordenadas da janela, largura, altura e atribuições de exibição em `$XDG_STATE_HOME/note-it/state.json`, não nos arquivos Markdown.
- **Justificativa:** preserva a limpeza e a portabilidade do Markdown em diferentes configurações de tela.

## ADR-004: @tiptap/markdown oficial e ecossistema Tiptap 3
- **Decisão:** Use Tiptap 3 com a extensão oficial `@tiptap/markdown` (todos os pacotes fixados na versão de correspondência exata `3.30.5`).
- **Justificativa:** As extensões de descontos de terceiros estão obsoletas e não recebem manutenção. O módulo de marcação oficial do Tiptap 3 fornece tokenizadores bidirecionais integrados, manipulação AST estável e renderizadores de marca extensíveis para elementos HTML controlados (`<u>`, `<mark>`, `<span>`).

## ADR-005: O recolhimento reutiliza o pipeline de geometria existente
- **Decisão:** Recolher uma nota mantém `width`/`height` como a única fonte de verdade para a superfície ativa e registra o tamanho anterior em `expanded_width`/`expanded_height`. A altura mínima é relaxada até a altura da barra de cabeçalho apenas enquanto `collapsed` for verdadeiro e o redimensionamento será desabilitado nesse estado.
- **Justificativa:** Um segundo sistema de geometria independente para notas recolhidas duplicaria a lógica de fixação, persistência e multimonitor estabilizada na Fase 3.0R.1. Reutilizar um pipeline significa que uma nota recolhida é arrastada, fixada e persistida exatamente pelo mesmo caminho de código que uma nota expandida, e a expansão restaura o tamanho gravado em qualquer posição em que a barra foi deixada. O redimensionamento é desativado enquanto recolhido porque não há geometria expandida coerente que um redimensionamento vertical de uma barra de cabeçalho possa produzir; a affordance é ocultada em vez de mostrada e ignorada.
- **Observação:** Enquanto o popover está aberto em uma nota recolhida, o host empresta altura extra à superfície para que o menu não seja cortado por uma superfície que tenha apenas a altura de uma barra de cabeçalho. Essa altura é apenas de apresentação — nunca é gravada em `state.json`.

## ADR-006: Os avisos da tabela de composição do GTK são externos e permanecem inalterados
- **Decisão:** Mantenha `GTK_IM_MODULE=simple` e não suprima o burst `Gtk-WARNING **: Can't handle >16bit keyvals` / `Can't handle Unicode codepoint …`.
- **Lógica:** Os avisos vêm do próprio GTK — as strings existem apenas em `libgtk-4.so`, não em Note-it ou WebKitGTK. `gtk_im_context_simple` analisa o arquivo X11 Compose do sistema no primeiro uso e avisa sobre algumas entradas cujos valores-chave ou pontos de código não se ajustam ao formato de tabela de composição de 16 bits (sequências de composição de emoji) e, em seguida, armazena em cache a tabela analisada em `$XDG_CACHE_HOME/gtk-4.0/compose/`. Um aplicativo GTK4 padrão com uma entrada de texto focada e um cache frio reproduz a explosão idêntica sem nenhum código Note-it envolvido. A explosão, portanto, aparece uma vez por geração de cache, somente na inicialização e nunca durante a digitação.
- **Impacto:** Nenhum para pt-BR. Chaves mortas e caracteres acentuados são todos pontos de código BMP e são analisados ​​normalmente; apenas as entradas não BMP são ignoradas. A remoção de `GTK_IM_MODULE=simple` interromperia os avisos, mas regrediria a composição de chaves mortas em Niri, e um manipulador de log global também ocultaria avisos reais do GTK.

## ADR-007: A superfície hospedeira carrega a cor do papel da nota
- **Decisão:** Voltar cada janela de nota com uma regra de folha de estilo GTK pintando a cor do papel e o mesmo raio de canto que a página usa, mantendo o próprio WebView transparente. A classe é trocada quando a cor da nota muda.
- **Lógica:** Um WebView é redesenhado de forma assíncrona. Quando um redimensionamento rápido aumenta a superfície, o compositor apresenta à superfície maior um quadro antes que a página a pinte, e a faixa que ainda não foi pintada mostra o fundo escuro padrão da janela – a faixa preta relatada após a Fase 3.1. Preenchê-lo a partir do host significa que a lacuna já tem a cor certa. Pintar o fundo na janela em vez de no WebView mantém os cantos arredondados: um fundo opaco WebView os teria quadrado.
- **Consequência:** O host precisa de sua própria cópia da paleta. Um teste o compara com `ui/src/styles/theme.css` para que os dois não possam se separar.

## ADR-008: Carimbos de data e hora de conclusão da tarefa viajam com sua tarefa
- **Decisão:** Armazene o carimbo de data/hora de uma tarefa concluída em um comentário HTML anexado à linha Markdown da própria tarefa: `- [x] Comprar material <!-- note-it:completed_at=2026-08-27T11:32:00-03:00 -->`.
- **Lógica:** O padrão Markdown não possui sintaxe para isso. Manter a linha principal simples `- [x] …` deixa a nota legível em qualquer outra ferramenta, enquanto o comentário fica invisível em Markdown renderizado. Como os metadados ficam na própria linha da tarefa, eles se movem com a tarefa quando as tarefas são reordenadas, o que uma tabela de front matter codificada pela posição da tarefa não poderia fazer.
- **Auditoria:** O sanitizador retirou todos os comentários de HTML e o lexer Markdown os descartou antes que Tiptap os visse. Ambos foram estendidos de forma restrita: o sanitizador mantém este formulário de comentário após validar o carimbo de data/hora, e os próprios ganchos Markdown do item de tarefa o leem em um atributo do nó e o retiram do conteúdo visível.
- **Datas desconhecidas permanecem desconhecidas:** uma tarefa que chega já verificada — carregada de Markdown, colada ou restaurada por desfazer — nunca recebe um carimbo de data/hora, então `- [x]` escrito fora de Note-it não mostra data.

## ADR-009: Zoom é uma escala de visualização, tamanho do texto é conteúdo
- **Decisão:** O zoom dimensiona o editor de acordo com o tamanho da fonte que o conteúdo herda, é armazenado como `zoom_percent` em `state.json` e nunca afeta o documento. O tamanho do texto é uma marca embutida separada que faz parte do conteúdo da nota.
- **Justificativa:** Eles respondem a perguntas diferentes — "tornar esta nota mais fácil de ler agora" versus "tornar esta palavra grande". A implementação por meio do outro gravaria as preferências de visualização no Markdown ou faria com que uma escolha de formatação desaparecesse quando a janela fosse reaberta. Uma transformação CSS foi rejeitada para o zoom: ela dimensiona os pixels pintados enquanto deixa as coordenadas do cursor e do ponteiro na geometria não dimensionada, para que o cursor do texto se afaste dos caracteres.
- **Consequência:** `Ctrl+=` / `Ctrl+-` agora direcionam o zoom em vez do tamanho base da fonte da nota. A base `font_size` em front matter ainda é respeitada quando uma nota é carregada; ele simplesmente não possui mais uma ligação de teclado.

## ADR-010: A invocação passa pela linha de comando, não pelo WebView
- **Decisão:** Uma invocação global é um atalho de teclado do compositor que gera `note-it`, que atinge a instância em execução por meio do despachante de instância única existente. Os atalhos no aplicativo permanecem como estão, para quando a nota já estiver em foco.
- **Justificativa:** Os atalhos dentro da nota são eventos-chave comuns em seu WebView, e um cliente Wayland só recebe eventos-chave enquanto mantém o foco do teclado. Eles nunca podem ser acionados enquanto o navegador estiver na frente – nenhuma quantidade de trabalho dentro do aplicativo muda isso. O compositor é o único componente que vê a chave, então o caminho confiável deve começar aí.
- **Manuseio de camadas:** uma superfície `bottom` está sempre abaixo de janelas comuns, portanto, uma nota na área de trabalho não pode ser mostrada sobre outro aplicativo sem movê-la para `overlay`. A invocação o eleva, mas mantém a preferência armazenada, então `note-it toggle`, `Ctrl+Shift+Space` e a próxima reinicialização ainda refletem o que o usuário escolheu. `note-it show` continua sendo a mudança de modo explícita e persistente.
- **Não é uma invocação:** iniciar o aplicativo respeita a preferência armazenada em vez de puxar a nota para a frente, portanto, iniciar Note-it na camada da área de trabalho deixa-a na área de trabalho.

## ADR-011: O fechamento de uma nota deve deixar um caminho para trás
- **Decisão:** Com cada nota fechada, uma invocação reabre a nota salva mais recentemente em vez de criar uma em branco. Uma nota só é criada quando não existe nenhuma ou em `note-it new`.
- **Justificativa:** O botão `×` salva a nota e registra `is_open = false`, mantendo o Markdown, a geometria e todas as outras propriedades armazenadas. Mas a inicialização apenas restaurou notas marcadas como abertas; portanto, quando a última nota foi fechada, ela ficou inacessível e o aplicativo respondeu com uma nota vazia. Nada foi perdido no disco; simplesmente não havia caminho de volta para isso.
- **Ordenação:** a atualidade vem da hora de modificação do arquivo de notas, portanto, nenhuma nota precisa ser analisada para decidir qual reabrir, e a ordem ainda reflete o último salvamento. *(Substituído por ADR-027.1: a chave de ordenação agora é a própria `updated_at` da nota, com `mtime` como substituto, porque uma alteração na aparência reescreve o arquivo sem ser uma edição.)*
- **Consequência:** a restauração também registra as notas como abertas novamente, de modo que uma nota reaberta não fique contradizendo seu próprio arquivo de estado.

## ADR-012: Uma nota recolhida se expande antes de seu menu ser aberto
- **Decisão:** clicar em uma nota recolhida a expande. O botão `☰` expande a nota e abre o menu com um clique. O mecanismo temporário de crescimento de superfície adicionado ao menu recolhido foi removido.
- **Lógica:** O popover de configurações estava sendo cortado em uma nota recolhida. Não é um problema de CSS: a superfície Wayland de uma nota recolhida tem apenas a altura da barra de cabeçalho e nada pode ser pintado fora de uma superfície, então `overflow` e `z-index` são irrelevantes. A Fase 3.1 contornou isso emprestando a superfície 120px enquanto o menu estava aberto, o que foi suficiente para um menu de duas entradas. A Fase 3.2 aumentou o menu para sete entradas – cerca de 234px – e a solução alternativa parou silenciosamente de cobri-lo.
- **Por que não simplesmente dar mais altura:** o número teria que ser reajustado toda vez que o menu muda, e uma barra que se transforma em um retângulo alto para mostrar um menu é algo estranho de se olhar. De qualquer forma, expandir a nota é o que o usuário deseja, não precisa de um número mágico e reutiliza o caminho de recolhimento que já existe.
- **Consequência:** a mensagem `menu_overlay` e sua constante de altura desapareceram, deixando uma maneira para uma nota mudar de tamanho.

## ADR-013: O texto destacado carrega seu próprio primeiro plano
- **Decisão:** `.ProseMirror mark` define um primeiro plano escuro para o texto destacado, em cada cor de papel. Uma cor de texto explícita é um estilo embutido e ainda vence.
- **Justificativa:** No papel escuro, o texto padrão é claro e todos os destaques na paleta são claros, portanto, o texto destacado era claro sobre claro e pouco legível. Fixá-lo na folha de estilo mantém uma preocupação de renderização: nada é escrito no Markdown, então uma nota não ganha uma marca de cor que nunca teve apenas por causa do papel em que está colocada, e ela viaja de ida e volta inalterada.
- **Paleta:** em vez de decidir em tempo de execução se a cor de um usuário "ainda é legível" e substituí-la, a paleta em si foi tornada segura - laranja, amarelo e verde foram escurecidos para que cada cor de texto criasse um contraste legível em cada realce e em cada cor de papel. A intenção do usuário é sempre preservada, porque nenhuma combinação na paleta é ilegível.

## ADR-014: A marca de destaque pinta seu próprio primeiro plano
- **Decisão:** `NoteItHighlight` substitui o atributo `color` `renderHTML` para emitir `background-color: <highlight>; color: #1E293B`, e a folha de estilo não tenta mais colorir o texto destacado.
- **Causa raiz corrigida:** a extensão Highlight upstream renderiza `style="background-color: X; color: inherit"`. Esse `color: inherit` é um **estilo embutido**, portanto supera qualquer regra de folha de estilo - incluindo o `.ProseMirror mark { color: … }` adicionado na Fase 3.3. O texto destacado, portanto, continuou herdando a cor do papel, que no papel escuro é branco em realce claro. A correção da Fase 3.3 nunca foi aplicada; apenas sua aritmética de contraste foi testada, e a aritmética sobre uma paleta não prova nada sobre o que o DOM realmente pinta.
- **Testes:** os testes agora afirmam a cor que o elemento realmente resolve por meio de `getComputedStyle` e que nenhum `inherit` é deixado na marca, em vez de calcular as taxas de contraste isoladamente.
- **Cor do texto explícito:** ProseMirror aninha o realce dentro do intervalo de cores, de modo que o primeiro plano embutido da marca ganha enquanto o realce está presente — a legibilidade é preservada — e a cor do usuário ainda é registrada no Markdown, reaparecendo assim que o realce é removido. Nada sobre a cor do papel é escrito no documento.

## ADR-015: Papel é uma propriedade de nota, o tema é uma propriedade de aplicativo
- **Decisão:** `paper_type` e `paper_intensity` ficam no YAML front matter da nota ao lado de `color`; a interface `theme` reside em `config.toml`.
- **Justificativa:** o papel é o que *é* uma nota — ele pertence à nota e acompanha o arquivo, exatamente como sua cor já fez, e passa pelo mesmo caminho de salvamento, que nunca toca em `updated_at`. O tema é a aparência do *aplicativo*: uma preferência, compartilhada por cada nota, portanto ela pertence às outras preferências globais em vez de ser copiada em cada arquivo.
- **Não está no corpo Markdown:** nada sobre o papel está escrito no documento. Nenhum elemento wrapper, nenhuma classe, nenhuma decoração - o corpo percorre byte por byte através de cada tipo e intensidade de papel.
- **Strings, não serde enums:** ambos os campos são armazenados como strings simples e resolvidos em relação ao conjunto suportado na leitura. Um serde enum falharia em toda a análise de um valor escrito por uma versão mais recente ou manualmente, custando a nota ao usuário; resolver o padrão custa-lhes um padrão.
- **Retrocompatibilidade:** uma nota escrita antes desta fase não contém nenhum campo e abre como papel comum em intensidade normal. `paper_intensity` é mantido par para `blank`, portanto, alternar o papel para frente e para trás nunca descarta silenciosamente a escolha.

## ADR-016: Um padrão de papel parametrizado, composto onde é pintado
- **Decisão:** os cinco artigos são um sistema CSS, não cinco implementações. O tipo seleciona um padrão e `--paper-pattern-spacing`, a intensidade seleciona `--paper-pattern-alpha` e a cor do papel seleciona `--paper-pattern-ink` e `--paper-pattern-gain`. Ambas as grades seguem a mesma regra em dois espaçamentos.
- **Onde a cor é composta:** `--paper-pattern-color` é declarado em `.editor-wrapper`, o elemento que a pinta — deliberadamente, e não em `:root`.
- **O defeito que o forçou:** `var()` é substituído onde está a declaração, usando os próprios valores desse elemento. Compor a cor em `:root` congelou a tinta e a opacidade da raiz nela, de modo que as substituições por papel e por intensidade em `body` nunca atingiram a tinta: cada intensidade renderizada em "normal", e o papel escuro foi desenhado com a tinta escura dos papéis *pálidos*, que é invisível em `#18181B`. Medir o real WebView pegou - as regras do papel preto saíram em `#17181D` contra o papel `#18181B`. Declará-lo ao consumidor permite que os três insumos herdem primeiro as escolhas reais da nota.
- **Contraste:** o papel escuro traz um ganho de `0.72` em vez de um aumento. Medir a luminosidade perceptiva, em vez de assumir, mostrou o oposto da intuição: um papel quase preto fica na parte íngreme da curva de luminosidade, então o mesmo alfa o eleva *mais* do que escurece um papel claro. O ganho atrai todas as três intensidades para a força que elas têm em todos os outros lugares.
- **Zoom:** o espaçamento é em pixels e nunca faz referência a `--note-zoom` ou `--note-font-size`, portanto, o conteúdo é dimensionado e o plano de fundo permanece no mesmo lugar. Verificado no WebView: o papel pautado mediu exatamente 24px entre as linhas em 75% e 300%.
- **Onde é pintado:** na superfície de rolagem com `background-attachment: local`, para que ele acompanhe o texto, enquanto `#app` mantém seu preenchimento de cor plano embaixo — um redimensionamento rápido pode expor o papel, mas nunca uma faixa não pintada. Ocultar essa superfície ao recolhê-la leva consigo o padrão, deixando a barra como uma faixa limpa da cor da nota, sem nenhum código extra.

## ADR-017: O tema veste o chrome, nunca o papel
- **Decisão:** um conjunto de tokens `--ui-*` (`surface`, `surface-hover`, `text`, `text-muted`, `border`, `shadow`, `focus-ring`) veste menus, popovers e estados de foco. Os tokens `--paper-*` continuam enfeitando tudo o que está desenhado no papel. A paleta de luz é definida em `:root` simples e apenas os mesmos tokens são redefinidos em `:root[data-theme="dark"]`.
- **Justificativa:** o popover usado para retirar `--popover-bg` do *papel* e seu primeiro plano de `--paper-text`. Isso não sobreviveria a um tema: um popover escuro sobre uma nota amarela herdaria o texto escuro daquele papel e seria ilegível. Dividir os dois significa que o menu é legível sobre uma nota preta e outra amarela, em qualquer tema, e uma nota ainda mantém sua própria cor.
- **O que é deixado deliberadamente no papel:** os botões do cabeçalho, a alça de redimensionamento, a barra de rolagem do editor e tudo dentro de `.ProseMirror`. Eles sentam no papel e o seguem.
- **A Fase 3.3R permanece intacta:** o texto destacado ainda carrega seu próprio primeiro plano escuro embutido, que supera ambos os conjuntos de tokens, de modo que permanece legível em todos os papéis em qualquer tema.
- **Preferência do sistema:** resolvido na página com `matchMedia('(prefers-color-scheme: dark)')`, assistido ao vivo para que o esquema de troca de desktop chegue a uma nota aberta. `matchMedia` é tratado como opcional - um WebView que não relata nenhum esquema de cores resolve `Sistema` para o tema claro em vez de terminar sem nenhum tema.

## ADR-018: `updated_at` é comparado, não assumido
- **Decisão:** `save_content` compara o texto recebido com o conteúdo já armazenado antes de gravar qualquer coisa. Conteúdo idêntico não atualiza nada, não escreve nada e ainda retorna `Ok`.
- **O defeito:** cada caminho que carrega o conteúdo de volta da página – salvamento automático, liberação antes de ocultar e sair e salvar e fechar – canalizado para `save_content`, que atribuiu o conteúdo e chamou `touch_content_modified()` incondicionalmente. Todos os três chegam rotineiramente com conteúdo que não foi alterado: fechar e liberar envia tudo o que o editor contém, independentemente de ter sido tocado ou não, e o salvamento automático pode ser acionado em uma edição que serializa de volta para o mesmo Markdown. Assim, apenas abrir uma nota e fechá-la mudou sua data de modificação, o que contrariava o contrato `docs/storage.md` já declarado. Medido na versão anterior: uma nota intocada passou de `15:31:25` para `15:31:35` em um ciclo de abertura/encerramento.
- **Onde reside a correção:** nesse único funil, não em cada chamador. Os três chamadores não precisam concordar sobre o que conta como uma edição, e nenhum segundo mecanismo de rastreamento sujo foi introduzido — o próprio campo `content` do documento já *é* o último texto persistido.
- **Por que ainda retorna `Ok`:** save-and-close espera por esse resultado antes de finalizar o fechamento, e as liberações de ocultar e sair esperam por ele antes de destruir superfícies ou sair. “Nada mudou” nunca deve se tornar “nada respondido”, ou uma nota intocada se recusaria a ser fechada.
- **Sem gravação:** quando o conteúdo corresponde, o arquivo é deixado inteiramente sozinho - sem arquivo temporário, sem renomeação, sem fsync. As alterações somente de metadados (cor do papel, tipo, intensidade, tamanho da fonte) seguem seu próprio caminho direto de salvamento e não são afetadas, portanto, nada que deva ser persistido é ignorado.
- **Consequência para atualidade:** o `mtime` do arquivo decide qual nota uma invocação traz de volta quando cada nota é fechada. Agora ele rastreia a última edição real em vez do último fechamento. Essa é a melhor leitura de “a nota usada por último”, e é coberta por um teste e não deixada ao acaso. *(Substituído por ADR-027.1: `mtime` era apenas um proxy para ele, e uma mudança de aparência moveu o proxy sem ser uma edição. A ordem agora é `updated_at` diretamente.)* A introdução de um `last_active_note` em `state.json` não foi feita deliberadamente aqui: nada aprovado depende do significado antigo, e inventar o estado para ele teria sido uma mudança maior do que o defeito justificava.

## ADR-019: Um documento só é adotado depois de escrito
- **Decisão:** cada alteração em uma nota — o conteúdo que chega da página e a cor do papel, tipo de papel, intensidade do padrão e tamanho da fonte que chega de seu menu — é preparada em uma *cópia* do `NoteDocument`. `save_note_atomic` é executado nessa cópia e somente uma gravação bem-sucedida torna o documento mantido na memória. Uma falha deixa a nota na memória exatamente como estava.
- **O defeito que isso fecha:** ADR-018 se baseia em uma premissa — "o próprio campo `content` do documento já *é* o último texto persistido" — e `save_content` quebrou essa premissa em si. Ele atribuiu o conteúdo e carimbou `updated_at` *antes* de chamar `save_note_atomic`, portanto, uma gravação com falha deixou a memória segurando B enquanto o arquivo ainda continha A. O atalho de conteúdo idêntico então comparou a próxima carga útil com B: salvamento automático, ambos liberam e salvam e fecham, todos reenviam tudo o que o editor contém, então o mesmo B chegou novamente, combinou e retornou `Ok` sem escrever nada. Salvar e fechar espera exatamente esse resultado, para que a nota possa ser encerrada em uma edição que nunca chegou ao disco. A otimização não causou a divergência, mas transformou-a de uma inconsistência transitória em uma perda silenciosa de conteúdo.
- **Por que transacional em vez de um sinalizador sujo:** uma segunda parte do rastreamento de estado "o que foi persistido pela última vez" teria que ser mantida em sintonia com o documento manualmente, em cada um dos quatro caminhos de gravação, e obter *isso* errado reproduz a mesma classe de defeito um nível acima. Preparar um candidato e trocá-lo em caso de sucesso não precisa de nenhum novo estado: o documento *é* o registro do que está no disco, que é o que o ADR-018 já presumia e agora realmente contém.
- **A aparência também é salva:** a cor do papel, o tipo de papel, a intensidade e o tamanho da fonte alteram o próprio documento com o qual a comparação de conteúdo é feita, de modo que eles seguem o mesmo caminho através de `save_metadata`. Uma cor que não pôde ser escrita não fica na memória como se estivesse, e escolhê-la novamente a escreve. Eles ainda não tocam em `updated_at`; aparência não é conteúdo.
- **O que o ADR-018 mantém:** conteúdo idêntico e já persistente ainda não grava nada e ainda retorna `Ok`, `updated_at` ainda se move apenas em uma edição real, `created_at` ainda é imutável e fechar e liberar ainda são bem-sucedidos quando não há realmente nada pendente. Somente uma carga útil que coincide com uma gravação *com falha* agora é tratada como pendente, porque está.
- **A cópia do editor não está em jogo.** A página possui o texto ativo e o reenvia a cada salvamento automático, liberação e fechamento; o `NoteDocument` é o registro do arquivo. Dois testes anteriores afirmaram o oposto - que uma falha ao salvar deixa o texto mais recente na memória - e nada o lê de volta: nenhum caminho recupera o conteúdo desse campo, `save_now` apenas persiste novamente e o `LoadNote` enviado em uma recarga de página deve descrever a nota armazenada de qualquer maneira. Essa expectativa era o perigo, por isso foi substituída em vez de preservada.
- **A criação de novas notas já é segura:** `create_new_note` grava o documento antes que qualquer janela exista e retorna em caso de falha, portanto, não resta nenhuma nota na memória alegando estar armazenada.
- **Um salvamento com falha é limpo depois de si mesmo:** o arquivo temporário é removido quando qualquer coisa, inclusive a renomeação, falha. Nada mais coletou um, então uma série de falhas costumava deixar detritos `.tmp.*` no diretório de notas permanentemente.
- **Testando falha de E/S sem tocar no armazenamento:** o diretório de notas é movido para o lado e um arquivo simples colocado em seu lugar, então o kernel recusa qualquer criação e renomeação abaixo dele com `ENOTDIR`. Essa é a resolução do caminho em vez de um bit de permissão, portanto também falha para o root, que é como o trabalho Rust CI é executado - um `chmod` teria passado silenciosamente por lá. As notas permanecem intactas no diretório que foi movido para o lado, o que permite que os testes afirmem que a nota armazenada sobreviveu inalterada à falha no salvamento.

## ADR-020: A renomeação é o ponto de commit
- **Decisão:** `save_note_atomic` relata falha em qualquer coisa que aconteça **antes ou durante** a renomeação e sucesso a partir da renomeação. A sincronização do diretório de notas ocorre após o ponto de confirmação, portanto, uma falha é relatada como um aviso de durabilidade no stderr e o salvamento ainda retorna `Ok`.
- **O defeito que isso fecha:** ADR-019 faz com que o chamador adote um documento somente quando o salvamento retornar `Ok`, o que é adequado para cada falha que deixa o arquivo em paz. A sincronização de diretório não é uma delas. Ele é executado *após* `rename` já ter substituído o destino e estar dentro da mesma cadeia `?`, portanto, sua falha foi relatada como falha no salvamento. O chamador então manteve o documento antigo enquanto o arquivo continha o novo - memória e disco descrevendo versões opostas, que é exatamente a divergência que o ADR-019 existe para evitar, espelhada. Um salva-e-fecha também teria se recusado a fechar uma nota que realmente havia sido escrita.
- **Por que renomear:** é o momento em que a mudança se torna visível. Cada leitor a partir de então recebe a nova nota, e nada mais tarde na função pode colocar a antiga de volta. Qualquer relatório diferente de "salvo" seria falso, e agir de acordo com ele significa descrever um arquivo que não existe mais dessa forma.
- **O que a sincronização de diretório realmente compra:** os *bytes* da nota já estão em armazenamento estável — o arquivo temporário é `fsync`ed antes da renomeação. Sincronizar o diretório é o que faz com que o próprio *rename* sobreviva a uma perda de energia. Sem ele, uma falha no momento errado pode deixar o nome ainda apontando para a nota anterior. Isso é uma atualização perdida, nunca um arquivo rasgado ou corrompido: o leitor vê a nota antiga ou a nova, nunca metade de nenhuma delas.
- **Por que não há estado de durabilidade pendente:** um `fsync` em um diretório libera todas as entradas pendentes nele, não apenas a mais recente, de modo que o próximo salvamento bem-sucedido de *qualquer* nota no diretório de notas torna a renomeação anterior durável também. Não há nada para lembrar, nada para tentar novamente manualmente, e o atalho de conteúdo idêntico não tem nada a mascarar: depois de um salvamento confirmado, mas não sincronizado, a nota no disco é realmente a nova, portanto, reenviá-la é realmente impossível. Rastrear uma sincronização perdida seria um estado que se cura sozinho, que é o tipo de escrituração contábil ADR-019 recusada pelo mesmo motivo.
- **O que não é reivindicado deliberadamente:** não há nova tentativa de sincronização, nenhuma garantia de que um salvamento seja durável quando a sincronização falha e nenhum `fsync` do arquivo de notas após a renomeação. O contrato é que uma nota nunca seja escrita pela metade e nunca seja revertida silenciosamente *dentro de um sistema em execução*; a janela de durabilidade está documentada em `docs/storage.md` em vez de ocultada.
- **Teste além do ponto de confirmação:** depois que `rename` for retornado, não há nada que um teste possa fazer ao sistema de arquivos que retorne à sincronização que o segue, portanto, essa falha é injetada no processo por um identificador `#[cfg(test)]` cuja sincronização de diretório sempre falha. Ele é compilado a partir de cada compilação real e orienta o `save_note_atomic` real e o `save_content` real, portanto, o que os testes verificam é o caminho de produção e não uma reimplementação dele. As falhas de pré-commit mantêm sua injeção real de `ENOTDIR`, que atinge os próprios syscalls.

## ADR-021: Quatro blocos, três formas, sem framework de blocos
- **Decisão:** o bloco de código, o texto explicativo, a citação em bloco e o comentário foram construídos como a menor coisa que poderia carregá-los, e nenhuma arquitetura de bloco compartilhado foi extraída.
  - um **bloco de código** é o `CodeBlock` do upstream com `lowlight` no topo e um método substituído;
  - um **callout** é o `Blockquote` existente com um atributo;
  - um **comentário** é um novo nó, porque nada que já esteja no esquema é um bloco de texto literal
isso não faz parte da prosa do documento.
- **Justificativa:** o roteiro permitiu uma arquitetura de blocos reutilizáveis ​​"onde a forma desses recursos justifica uma", e isso não acontece. Eles compartilham uma seção de menu e nada mais: modelos de conteúdo diferentes (`text*` versus `block+`), sintaxe Markdown diferente (uma cerca, um prefixo de citação, um comentário HTML), regras de análise diferentes, escape diferente. Uma base comum teria sido uma interface vazia com quatro implementações não relacionadas por trás dela, que é uma camada para leitura em vez de uma camada que carrega peso.
- **A chamada é um atributo, não um nó.** Essa decisão paga pela maior parte da fase. Um texto explicativo herda o modelo de conteúdo do blockquote, portanto, vários parágrafos, listas e blocos aninhados funcionam sem serem projetados; herda o prefixo `>`, então a serialização é a saída do pai com uma linha na frente; ele herda os comandos e regras de entrada. E o modo de falha é gratuito: um `[!KIND]` não reconhecido não produz nenhum atributo, que *é* uma citação simples com o marcador ainda em seu texto. Um nó `callout` separado precisaria de tudo isso escrito duas vezes e de uma regra sobre o que fazer quando o tipo for desconhecido.
- **O realce é uma decoração e apenas uma decoração.** `lowlight` pinta ProseMirror decorações sobre os mesmos personagens, para que o arquivo permaneça uma cerca simples. Dezesseis gramáticas são importadas por nome, em vez do pacote `highlight.js`, que carrega quase duzentas: a fase inteira custa cerca de 30 kB compactada.
- **Nunca adivinhe um idioma.** O upstream recorre a `highlightAuto` para um bloco sem idioma ou que não pode ser resolvido; ambos são substituídos por um `highlightAuto` que não retorna nada. Uma cerca escrita sem linguagem é propositalmente clara, e colorir uma cerca desconhecida com o que ela mais se assemelha diz ao leitor algo que a nota não diz.
- **O identificador do idioma nunca é reescrito.** Não normalizado, não padronizado, não descartado. Um alias permanece um alias e um idioma desconhecido mantém sua ortografia, porque a nota é o arquivo e o arquivo disse o que disse. Os aliases são resolvidos apenas para realce e para o rótulo do menu, e a tabela de alias é lida a partir das próprias gramáticas, em vez de escrita à mão.
- **Comentários se transformaram em conteúdo.** O sanitizador costumava descartar todos os comentários, exceto os metadados de tarefas do próprio Note-it, portanto, uma nota contendo um deles os perdia no primeiro salvamento. Um comentário é um dado inerte, nunca uma marcação executável, e agora é mantido. Dois testes afirmaram o comportamento antigo e foram substituídos. Um `<!--` interminado é escapado em vez de engolir o restante do arquivo, que é a mesma regra que o restante do sanitizador segue: degradar para texto, nunca excluir.
- **Um comentário é visível, mas não tem conteúdo.** Em um editor WYSIWYG, um comentário oculto é um comentário que ninguém pode editar ou remover, e um arquivo contendo algo que a janela nunca mostra perde coisas silenciosamente. Em vez disso, ele é desenhado como um pequeno bloco rotulado, separado da prosa e serializado como `<!-- ... -->`. Um `-->` dentro é escrito com escape, porque a sequência literal fecharia o comentário mais cedo e espalharia a nota dele.
- **Nenhuma nova superfície para HTML arbitrário.** Cada rótulo é uma constante no elemento e cada tipo vem de uma lista de permissões de cinco valores, portanto, nenhum conteúdo de nota atinge um atributo, uma classe ou um estilo. O conteúdo do bloco de código é texto em um nó que se declara como código; o conteúdo do comentário é texto em um nó que não recebe marcas.
- **Um menu, uma seção.** Os quatro ficam em **Blocos** no popover que já existe, construído a partir do mesmo painel e auxiliares de linha de todas as outras seções, e as linhas refletem onde o cursor está em vez de oferecer uma lista fixa. Nenhum atalho foi adicionado: os acordes úteis são usados ​​e digitar Markdown ainda funciona.

## ADR-022: Uma gravação atômica e um ponto de commit para cada arquivo armazenado pelo Note-it

A Fase 3.4R.2 estabeleceu que a renomeação é o ponto de confirmação para uma nota: um salvamento relata falha para qualquer coisa até ela, inclusive, e sucesso a partir dela, porque depois de renomear o arquivo no disco *é* o novo conteúdo e nenhum chamador pode acreditar no contrário. `state.json` e `config.toml` nunca entenderam essa regra e se afastaram dela em direções opostas.

`state.json` foi escrito atomicamente, mas propagou uma falha de sincronização de diretório pós-renomeação como uma falha ao salvar. Cada chamador trata isso como "nada foi escrito": fechar uma nota reverteu seu estado na memória e deixou a janela aberta, e ocultar recusou-se a fechar as janelas - enquanto o arquivo já continha o novo estado. Memória e disco descreveram então diferentes aplicações.

`config.toml` não foi escrito atomicamente. Ele passou direto pelo arquivo real com uma abertura truncada, então uma gravação interrompida deixou uma configuração escrita pela metade; o carregamento volta aos padrões sem uma palavra, o que transforma uma gravação parcial em uma redefinição silenciosa do tema e de todas as outras preferências.

Três cópias de uma regra sutil foi como ela evoluiu, então agora existe uma: `atomic_file::write_atomic` contém a regra e sua explicação, e notas, estado da janela e configuração passam por ela. A criação do diretório pai é deixada para quem chama - os diretórios do store são criados uma vez na inicialização, e um diretório de notas que desapareceu desde então é uma falha a ser relatada, em vez de ocultada.

## ADR-023: A página é o widget de foco da janela

Uma nota é uma janela `gtk4-layer-shell` contendo um WebView. Essa janela é mapeada sem nenhum widget de foco: a janela pode estar ativa, com o compositor enviando chaves para ela, enquanto GDK não tem onde entregá-las e as descarta antes do WebKit. Cada atalho dentro de uma nota estava, portanto, morto até que um clique focasse o WebView como um efeito colateral.

O foco não é algo para agarrar uma vez na inicialização. A janela perde e recupera o foco do teclado ao longo de sua vida útil - um clique, uma mudança de camada, uma invocação - então o WebView é focado sempre que a janela *se torna* ativa. Isso cobre o primeiro mapa e cada remapeamento com uma regra, e não captura nada enquanto a nota não for a superfície com a qual o compositor está falando.

O que isso não faz, e não pode, é fornecer chaves para uma superfície para a qual o compositor não as está enviando. Uma nota na camada `bottom` está atrás de cada janela e recebe foco somente quando é clicada; se estiver coberto, não há nada para clicar. O `Ctrl+Shift+Space` autoritativo, portanto, pertence a Niri e chama a GAction `toggle-layer` do aplicativo em execução. O acorde WebView continua sendo um substituto local. Consulte `docs/niri.md`.

As medições em Niri 26.04 e no protocolo Layer-Shell versão 4 também corrigiram uma suposição separada: a configuração `Bottom`/`Overlay` não remapeia inerentemente a superfície. O antigo caminho rápido `present()`/visibilidade era o comportamento do aplicativo, não um requisito de protocolo. Note-it agora remapeia deliberadamente apenas uma promoção Desktop-to-Overlay obstruída para forçar o commit pendente Wayland, mapeia-o com a interatividade do teclado desativada para manter o foco da janela normal e restaura o clique para focar depois que o compositor observou o mapa. A direção reversa usa a mudança de protocolo ao vivo sem apresentação.

## ADR-024: Um resultado calculado é uma decoração e o analisador não tem avaliador

Duas decisões levam à Fase 3.6, e nenhuma delas trata de aritmética.

**Um resultado nunca é satisfeito.** Cada valor que o mecanismo produz é uma decoração de widget ProseMirror — o mesmo mecanismo que pinta o realce de sintaxe sobre uma cerca de código. Escrever os resultados no documento teria sido mais simples de construir e errado de cinco maneiras distintas ao mesmo tempo: o `.md` ganharia números que ninguém digitou; `updated_at` mudaria porque algo foi *recalculado* em vez de editado, desfazendo tudo o que a Fase 3.4R estabeleceu; abrir uma nota seria uma edição; um resultado obsoleto seria salvo em uma nota editada em outro lugar; e o arquivo deixaria de ser portátil Markdown. Como decoração a nota no disco é a nota que foi escrita, reabrindo recomputações do texto, e não há nada que fique obsoleto. Isso também significa que desfazer e refazer não precisava de nenhum trabalho: os resultados não são etapas, portanto, uma edição é um desfazer e os resultados seguem o que quer que o documento se torne.

**O analisador não tem nenhum avaliador por trás dele.** Um lexer que conhece dez formas de token, um analisador descendente recursivo que produz seis tipos de nós e uma caminhada sobre essa árvore. Sem `eval`, sem `Function`, sem acesso de propriedade, sem sintaxe de chamada, sem objeto host. `= window.location` não é uma entrada filtrada — não pode ser escrita e para em `.`. As variáveis ​​residem em um `Map` em vez de em um objeto, que é uma propriedade de segurança e não uma escolha de estilo: um objeto responderia `constructor`, `__proto__` e `toString` com valores reais de JavaScript. Nada foi adicionado a `package.json`; uma biblioteca de expressões gerais teria sido maior que a gramática e traria recursos para os quais este formato de nota não tem utilidade. O mecanismo custa cerca de 2,5 kB compactado.

**Sintaxe explícita, porque a alternativa é uma máquina de adivinhar.** Um cálculo começa com `=` e uma declaração usa `:=`. Sem um marcador, o mecanismo passaria a vida decidindo quais números em uma nota são aritméticos – uma data, uma versão, “2 + 2 = 4” escrito em uma frase – e estaria errado de forma visível e frequente. O mesmo raciocínio fixa o limite de agregação: `sum` lê o bloco de linhas `=` diretamente acima dele e nunca um número simples em prosa.

**Previsibilidade sobre inteligência, nos dois pontos onde eles entram em conflito.** `200 + 10%` é lido como um aumento porque é isso que todos querem dizer com isso, mas a regra está anexada a um `%` escrito na linha e não a um valor que veio de um, então `taxa := 10%` seguido por `= 200 + taxa` adiciona `0,1`. E um número com dois separadores é recusado em vez de lido como um agrupamento: `1.234.567` é um número agrupado por mil numa convenção e sem sentido na outra, e o resultado da adivinhação é uma resposta errada que parece certa. Os resultados são impressos sem um separador de milhares pelo mesmo motivo - um resultado que esse mesmo mecanismo não pudesse ler seria uma armadilha.

**De cima para baixo, portanto não há gráfico.** Uma variável existe a partir de sua declaração para baixo. Isso torna `= preco * 2` acima de `preco := 100` uma variável desconhecida em vez de um quebra-cabeça, e torna os ciclos impossíveis sem um resolvedor para evitá-los: `a := b + 1` sobre `b := a + 1` falha na primeira linha porque `b` ainda não existe. Um gráfico de dependência teria sido um resolvedor, um detector de ciclo e uma ordem de avaliação para errar, em troca de um comportamento que ninguém pediu.

**Recalcule tudo e meça antes de otimizar.** Cada alteração no documento reavalia toda a nota. É uma varredura e um pequeno analisador do texto de uma janela; em uma nota com 100 parágrafos, 20 variáveis, 50 expressões e três agregadores é uma fração de milissegundo, o que é menos do que a contabilidade necessária para uma versão incremental. A reatividade então é gratuita, em vez de ser um recurso: não há cache para invalidar.

**Apenas parágrafos simples.** O cálculo não é lido dentro de blocos de código, código embutido, comentários, títulos, listas, tarefas, citações ou textos explicativos. Apoiá-los pela metade produziria uma nota onde a mesma linha calcula em um lugar e não em outro por razões que o leitor não pode ver. O limite é uma regra, declarada na documentação e testada; ampliá-lo posteriormente é uma mudança para uma função.

## ADR-025: A conversão é um sufixo de linha e a tabela de unidades é composta por dados

A Fase 3.7 teve que adicionar conversões sem construir um segundo mecanismo de cálculo ao lado do primeiro e sem permitir que uma tabela de unidades se transformasse em uma biblioteca de física. Quatro decisões fizeram isso.

**`em` fica no nível da linha, não dentro da gramática da expressão.** Uma linha é `expression unitRef 'em' unitRef`, e o analisador de expressão é executado primeiro e para por conta própria na unidade de origem — um identificador após uma expressão completa não é algo que qualquer regra possa continuar. Esse posicionamento único é o motivo pelo qual a conversão não custa nada à gramática de expressão: `10`, `distancia`, `(10 + 5)` e `x * 2` analisam exatamente como fizeram em 3.6, e tudo o que deixam para trás é de onde as unidades são lidas. Colocar `em` dentro da gramática, como `de` é, significaria que uma unidade se tornaria uma espécie de operando e, com ela, uma decisão sobre o que `2 * 3 km` significa antes que houvesse qualquer necessidade de ter um.

O custo é uma regra declarada: **a unidade se aplica a toda a expressão do lado esquerdo**, então `= 10 + 5 km em m` equivale a quinze quilômetros. Não existe álgebra unitária para dar um significado à outra leitura, e uma regra que um leitor pode ter em mente é duas entre as quais ele terá que adivinhar.

**Uma unidade é uma linha em uma tabela, não uma ramificação.** Cada biblioteca de conversão eventualmente aprende que `if km then m, if m then cm` são O(n²) regras para escrever e O(n²) regras para errar. Cada linha carrega uma dimensão e uma escala, a conversão é `value × from.scale ÷ to.scale` e adicionar uma unidade é adicionar uma linha. A temperatura é a exceção que a forma deve permitir — suas escalas têm zeros diferentes e nenhuma multiplicação leva de 0 a 32 e de 100 a 212 ao mesmo tempo — portanto, essas linhas carregam `toBase`/`fromBase`, e nada fora de `convert.ts` precisa saber de que tipo é uma linha.

Duas consequências que merecem ser mencionadas. **Área é sua própria dimensão**: `m²` é uma linha com fator 1 e `cm²` uma linha com fator 0,0001, não `m` com um expoente, então `1 m²` é `10 000 cm²` e não `100`. E **velocidade são três linhas nomeadas**, não um comprimento dividido por um tempo. Um sistema de unidades derivadas teria sido o início de uma biblioteca de física; `km/h`, `m/s` e `mph` são uma tabela com três linhas e uma regra extra no leitor de unidades, que é onde esta fase traçou o limite.

**Ortografia exata, sem conversão de maiúsculas e minúsculas (case folding) e apenas valores que não são opiniões.** A pesquisa é um `Map` digitado por cada ortografia listada na tabela, com correspondência exata. `m` é um metro e `M` não é nada, porque uma regra que os dobrasse dobraria `MB` em `mb`, que diferem por um fator de oito milhões; onde uma conveniência em letras minúsculas é segura, ela é listada como um alias, e é por isso que `ml` e `l` funcionam. O `Map` também é a propriedade de segurança, a mesma que as variáveis ​​do mecanismo matemático possuem: um objeto responderia `constructor` e `__proto__` com valores reais de JavaScript.

O que *não* está na tabela é tão importante. `cup`, `tsp`, `xícara` e `alqueire` são medidas reais com mais de um valor real, e uma conversão cuja resposta depende da definição que o leitor tinha em mente é pior do que nenhuma conversão, porque está silenciosamente errada. Os aliases portugueses são ASCII (`quilometros`, não `quilômetros`) porque nomes de variáveis ​​e nomes de unidades compartilham um lexer, e ampliá-lo para acentos mudaria o que uma palavra acentuada significa em uma expressão - uma decisão política sobre variáveis, feita acidentalmente, para comprar um alias. Três caracteres *foram* adicionados, `°`, `²` e `³`, porque aparecem em símbolos de unidade e em nada mais, e recusar um `1 m² em cm²` colado teria sido o tipo errado de estrito.

**Uma variável contém um número, não uma quantidade.** `distancia := 10 km` é uma expressão inválida e o formato suportado é `distancia := 10` com a unidade na linha que a utiliza. Transportar unidades através de variáveis ​​significa que o tipo de valor do mecanismo deixa de ser `number`, e porcentagens, agregação, `isLiteral` e todas as regras já estabelecidas devem ser redefinidas em torno de um tipo de quantidade. Essa é uma característica real e coerente, mas não é algo que possa ser colocado ao lado de uma tabela de unidades; o roteiro pedia uma escolha consciente em vez de um híbrido, e é isso, documentado onde o leitor o encontra.

Pela mesma razão **uma linha convertida encerra um bloco de agregação**. `sum`, `avg` e `count` somam números simples e não sabem nada sobre unidades. Deixar uma quantidade convertida em um bloco totalizaria dez mil de uma coisa contra cinco de outra e apresentaria a resposta como um fato. Agregar unidades é um recurso; agregar silenciosamente entre eles é um bug.

**As moedas estão ausentes, e a ausência é a entrega.** Tudo no registro é uma constante: um quilômetro são mil metros em uma máquina que nunca teve uma interface de rede, e isso acontecerá em dez anos. Essa propriedade é o que torna seguro calcular uma conversão silenciosamente, como uma decoração, sem cache e sem obsolescência para raciocinar. Uma moeda não tem nada disso - não há resposta sem uma taxa, uma taxa diferente a cada minuto, e uma taxa codificada aqui estaria errada antes que o commit que a adiciona terminasse de ser enviado.

Portanto, o limite é a borda do módulo, e honrá-lo agora não custa nada: `Dimension` lista apenas quantidades que são constantes e `convertValue` é síncrono e total. Uma conversão baseada em taxa não é nenhuma das duas coisas, portanto não pode ser adicionada a esta função sem que a mudança seja óbvia - ela pertence a um provedor assíncrono com sua própria desatualização, seu próprio estado de falha e sua própria maneira de informar ao leitor a idade do número. Nenhuma interface de provedor foi escrita, porque uma abstração vazia é um guia pior para o futuro do que uma declaração simples de como o futuro deverá ser. O que esta fase deve à próxima é a ausência de uma taxa codificada, e um teste afirma que nada no mecanismo pode alcançar a rede.

## ADR-026: O isolamento de teste deve cobrir o canal IPC, não apenas o sistema de arquivos

`scripts/note-it-isolated` substituiu os quatro diretórios base XDG e nada mais. Essa é a leitura óbvia de “isolar o store”, e é errada para esta aplicação de uma forma que fica invisível até custar alguma coisa.

Note-it é uma instância única `GApplication`. A instância única não é um arquivo de bloqueio ou uma verificação pid: é um nome bem conhecido no **barramento de sessão**. O segundo processo iniciado encontra o nome de propriedade, entrega sua linha de comando ao proprietário por meio de D-Bus e sai. O proprietário faz o trabalho.

Então as variáveis ​​XDG configuraram um processo que nunca abriu um store. Com um daemon já rodando no barramento real, cada comando "isolado" era encaminhado para ele, e o daemon real escrevia para o store real. Durante os testes físicos da Fase 3.7, coloque uma nota de teste no diretório de notas do próprio usuário.

**A decisão: um ambiente de teste deve isolar todos os canais pelos quais o trabalho pode sair dele e, para um aplicativo de instância única, o barramento IPC é um deles.** O harness agora inicia um `dbus-daemon` privado por sessão e aponta `DBUS_SESSION_BUS_ADDRESS` para ele. Nesse barramento, o nome conhecido não tem dono, então o processo isolado se torna a instância primária e faz seu próprio trabalho. O daemon real nunca é parado e nunca percebe, o que importa: um equipamento que exigisse a interrupção da sessão do usuário simplesmente não seria usado.

**Falha de forma segura (fail-closed), sem sucesso parcial.** Cada verificação é executada antes de Note-it ser iniciado — o barramento é iniciado, é comprovado que é um endereço diferente do endereço real, é comprovado que ele responde — e o ambiente do processo iniciado é então lido de volta a partir de `/proc` e comparado. Quatro códigos de saída nomeiam as quatro garantias (90 XDG, 91 binário, 92 barramento, 93 ambiente lançado). Deliberadamente, não existe nenhum caminho que se degrade para "pelo menos a parte XDG funcionou", porque esse é exatamente o estado em que o script antigo estava enquanto falhava.

**`XDG_RUNTIME_DIR` permanece real.** `WAYLAND_DISPLAY` é resolvido dentro dele, então substituí-lo custaria a exibição. `DBUS_SESSION_BUS_ADDRESS` decide o barramento e vence o soquete do diretório de tempo de execução, portanto, configurá-lo é suficiente e a única coisa que não quebra outra coisa. As variáveis ​​D-Bus *starter* são apagadas pela mesma razão que uma correia tem uma cinta.

**O barramento é por sessão, não por comando.** Um aplicativo de instância única só pode ser testado em vários comandos se eles compartilharem um barramento, então `--root DIR` registra o barramento sob essa raiz e cada invocação posterior que o nomeia o reutiliza. Isso é o que torna "iniciar um daemon e depois enviá-lo `new`" testável, e é a forma que todo teste físico neste projeto assume.

**O teste de regressão constrói o incidente em vez de descrevê-lo.** `scripts/test-isolation` cria uma sessão de ambiente com seu próprio barramento e sua próprio store doméstico, imprime impressões digitais em nanossegundos e - onde existe um display - coloca um daemon `note-it --background` genuíno nele, possuindo o nome real bem conhecido. Então ele executa o harness. Contra o harness fixo a nota só cai no store descartável; contra o antigo, o teste relata a nota perdida no store do daemon de ambiente, que é exatamente o incidente. Ele roda em `cargo test`, porque um teste que ninguém lembra de executar é a documentação.

A metade stub desse teste existe, então tudo é executado em CI, onde não há exibição. O que o esboço prova é para onde o harness *aponta* um processo, que é o que falhou; a metade do daemon prova a consequência.


## ADR-027: Pesquisa sem índice e duas ideias diferentes da "mesma palavra"

A Fase 3.8 teve que tornar cada nota localizável, levar o leitor ao que foi encontrado e deixá-lo alterá-lo – sem que nada disso se tornasse uma segunda fonte de verdade sobre o que as notas contêm. Quatro decisões fizeram isso.

**Sem índice, porque a varredura já é rápida o suficiente para ficar invisível.** Mil notas são listadas, lidas, dobradas com acento, combinadas e transformadas em trechos em cerca de 40 ms nesta máquina (cerca de 20 ms antes da Fase 3.8R fazer a ordenação ler o próprio `updated_at` de cada nota); uma consulta que não corresponde a nada custa o mesmo e uma que corresponde a tudo custa menos porque a lista de resultados é limitada. Isso está bem abaixo do limite em que uma pessoa percebe um atraso, e é todo o orçamento – não há cache quente e nenhuma penalidade na primeira execução, porque não há nada para aquecer.

Um índice não compraria nada mensurável aqui e custaria muito que não seja medido em milissegundos: invalidação quando um arquivo muda abaixo dele, reconstrução após uma falha, uma versão de formato para migrar, um arquivo para backup que não seja uma nota e uma segunda implementação para o CLI concordar. Cada uma delas é uma forma de pesquisa discordar das notas. A leitura das notas não pode discordar das notas. A medição reside em `searching_a_thousand_notes_is_fast_and_writes_nothing`, portanto a reivindicação é verificada novamente em vez de lembrada, e o dia em que ela falha é o dia em que essa decisão deve ser revista — com o número em mãos.

**Leituras de pesquisa; ele nunca escreve.** Nada no caminho de pesquisa libera, salva ou toca em uma nota, e abrir um resultado também não: ativar, abrir e expandir são estados de janela e `updated_at` significa "quando o texto foi alterado pela última vez". O leitor deve ser capaz de pesquisar suas anotações centenas de vezes e encontrar cada carimbo de data/hora exatamente onde o deixou. O mesmo teste que mede a varredura também afirma que os tempos de modificação das notas permanecem inalterados depois dela.

**Duas dobraduras diferentes, propositalmente.** A pesquisa global não faz distinção de acentos: `biopsia` encontra `Biópsia`, que em português é a diferença entre a pesquisa funcionar e a pesquisa ser um exercício de digitação. Localizar e substituir dentro de uma nota é sensível ao acento*, porque a substituição é destrutiva e um leitor que digita `saude` não pediu para substituir `saúde`. Ser capaz de dizer por que eles diferem vale mais do que a organização de uma regra.

Isso deixa uma costura e é fechada explicitamente: um resultado traz a grafia que realmente corresponde a *naquela nota*, então escolher `biopsia` na paleta diz à nota para procurar por `Biópsia`. Sem ele, a nota abriria com destaque para nada, o que é uma resposta pior do que não pesquisar. Recuperar essa grafia é o motivo pelo qual a dobra preserva o comprimento onde pode estar e é mapeada de volta através da origem onde não pode - os deslocamentos dobrados precisam nomear posições reais nos bytes originais.

**Substituir é uma transação, não uma operação de string.** Serializar a nota para Markdown, executar `String.replace` e recarregar levaria algumas linhas e jogaria fora marcas, seleção, posição de rolagem e histórico de desfazer, e aplicaria a substituição a destinos de link, caracteres de escape e tudo mais que o serializador escreve que o leitor nunca digitou. Em vez disso, cada ocorrência é um intervalo de documentos e `Replace All` é uma transação ProseMirror aplicando-as da última para a primeira - da última para a primeira, para que as posições anteriores permaneçam válidas, uma transação, então vinte substituições são uma `Ctrl+Z`. Marcas, estrutura de lista e títulos sobreviveram porque o documento nunca foi reconstruído.

O mesmo princípio responde ao que Find pode ver. O `4` de um cálculo e o `10000 m` de uma conversão são decorações e as decorações não estão no documento; uma pesquisa no documento, portanto, não pode encontrá-los, não sendo necessária nenhuma regra para excluí-los. `Ctrl+F` para `4` em uma nota cujo único `4` é um resultado não encontra nada, o que é exatamente correto: esse caractere não está no arquivo.

**Colar um URL em uma seleção é um comportamento com uma porta; links compactos não são nenhum.** (A Fase 3.8 chamou isso de "AutoPaste"; a Fase 3.9 o renomeou, sem alterá-lo, então a palavra é livre para o modo de captura da área de transferência que a Fase 3.11 trará - veja a Fase 3.9 no roteiro.) Colar um URL sobre o texto selecionado é aquela pasta onde a intenção do leitor é inequívoca - eles escolheram as palavras primeiro - e onde o comportamento padrão joga fora o que eles escolheram. Ele reutiliza `safeLinkUrl`, a lista de permissões que a política de autolink já tinha, portanto, há exatamente uma opinião no aplicativo sobre o que é um URL; O próprio `linkOnPaste` de Tiptap foi desligado por esse motivo, porque usa `linkifyjs` e teria aceitado esquemas que este aplicativo não permite. Nada é obtido: nenhum título, nenhum favicon, nenhuma visualização e, portanto, nenhuma rede, nenhum rastreamento e nenhuma espera.

A renderização de link compacto foi avaliada e deliberadamente não implementada. Todo o seu efeito é ocultar parte de um destino, e o leitor que mais precisa ver `https://evil.example.com/path` por completo é aquele que uma forma abreviada enganaria. Note-it já renderiza o texto de um link e mantém seu alvo no Markdown, que é a versão honesta da mesma ideia. O roteiro pedia "somente onde cabe na arquitetura"; isso não acontece, e dizer isso é o resultado.


### ADR-027.1: Fazendo com que as promessas correspondam ao comportamento (Fase 3.8R)

A Fase 3.8 enviou uma pesquisa que funcionou. Quatro coisas que ele *disse* não foram exatamente o que fez, e o 3.8R corrigiu as quatro em vez de aumentar o recurso.

**"Cada nota" agora significa cada nota.** A varredura parou em 5.000 notas. Era um limite máximo que ninguém cumpriria e uma promessa que ninguém poderia verificar: a nota 5.001 era impossível de encontrar e nada em lugar algum diria isso. Um limite nos resultados é diferente de um limite na varredura – cem linhas é o que uma pessoa pode ler, e o leitor pode ver que há uma centena delas. Uma nota que nunca é examinada não deixa vestígios de ter sido ignorada. Assim, a varredura lê todo o armazenamento e `MAX_RESULTS` ainda limita a resposta, com `a_note_past_the_old_scan_ceiling_is_still_searched` colocando uma nota na posição 5 001 e encontrando-a. A listagem de consulta vazia mantém um limite, porque mostra no máximo cem notas e a leitura além delas não responderia a nenhuma pergunta.

**Uma resposta obsoleta é qualquer resposta a uma pergunta que não está mais sendo feita.** A paleta numerou todas as solicitações e recusou qualquer resposta anterior à última que havia *aceitado*. Isso cobre uma resposta lenta que chega depois de uma rápida e perde a ordem oposta: pergunte `bio`, depois pergunte `biopsia`, e a resposta para `bio` chega enquanto `biopsia` ainda está em andamento - mais antiga que a pergunta atual, mas mais recente do que qualquer coisa aceita, assim foi mostrada. A regra é agora a mais simples que sempre deveria ter sido: apenas a resposta ao pedido actualmente pendente pode alterar a lista.

**Os limites limitam a consulta e a resposta, não a nota.** `MAX_QUERY_CHARS`, `MAX_RESULTS` e `MAX_SNIPPET_CHARS` foram descritos como fazendo com que uma *nota* patológica custasse um valor limitado. Eles não fazem isso, e não devem: a pesquisa encontra texto no final de uma nota grande, o que significa ler até o final de uma nota grande, e um corte silencioso na quantidade pesquisável de um arquivo colocaria no armazenamento um texto que nenhuma pesquisa poderia retornar. A documentação diz agora o que é verdade – estes são limites máximos para a pergunta e para a resposta, o custo de uma nota grande é medido em vez de limitado, e não é reivindicada qualquer garantia formal para um ficheiro único arbitrariamente grande. Nada foi tornado assíncrono para satisfazer uma frase; a frase foi corrigida.

**"Mais recente" significa escrito mais recentemente.** A Fase 3.4R definiu `updated_at` como a última alteração no *texto* de uma nota, e a aparência - cor, papel, intensidade do padrão, tamanho da fonte - deliberadamente não a move. Mas a aparência é armazenada no arquivo de notas, portanto, alterá-la reescreve o arquivo e a ordem lê o `mtime` do arquivo. Recolorir uma nota, portanto, tornou-a a nota "editada" mais recentemente no alternador rápido, e a nota que uma invocação trouxe de volta. A ordem agora é `updated_at`, o campo no qual o contrato já está escrito, e volta para `mtime` para uma nota que não possui nenhuma - uma escrita antes da existência do campo, uma sem front matter, aquela cujo cabeçalho não pode ser analisado. Esse substituto é a regra que todas as notas seguiam antes de haver um campo para leitura, portanto, nada muda em um store antigo.

Custa uma leitura limitada do cabeçalho de cada nota - 4 KB, o suficiente para um front matter de um punhado de linhas curtas - onde a listagem anteriormente custava apenas um `readdir`. Medido, foi isso que levou uma busca de mil notas de cerca de 20 ms a cerca de 40 ms na liberação, aproximadamente metade nas leituras e metade no YAML; o teto de varredura removido não explica nada disso, porque nenhuma store na medição atingiu 5.000. Quarenta milissegundos é um terço dos 120 ms que a paleta espera antes de perguntar, então é um preço que vale a pena pagar para que "mais recente" signifique a mesma coisa na troca rápida, na pesquisa e na invocação. Um cabeçalho ilegível custa registrar seu carimbo de data e hora, nunca a listagem: nada aqui escreve, nada aqui entra em pânico e um empate é desfeito por identificador para que a mesmo store sempre liste na mesma ordem.

O cabeçalho é lido uma vez por nota para decidir a ordem, e as notas que realmente serão mostradas são então lidas na íntegra. Isso não é deliberadamente mesclado em uma única leitura completa de cada nota: abrir a paleta em uma consulta vazia mostra cem notas, e um store que possui algumas notas enormes não deve pagar para ler todas elas para listar cem. A pesquisa lê todas as notas por completo, porque uma pesquisa não pode saber qual nota contém a palavra até que seja procurada.

## ADR-028: Excluir uma nota significa mover seu arquivo, e a movimentação é o ponto de commit

A fase 3.9 deu uma exclusão a Note-it. Até então, fechar uma nota a deixava no disco, o que era seguro e também significava que não havia como se livrar dela de dentro do aplicativo. A versão adicionada é deliberadamente a menor que não pode perder texto.

**Uma nota excluída sai do store ativo.** `notes/<uuid>.md` se torna `trash/<uuid>.md`, em um diretório irmão do mesmo diretório de dados `note-it`. A alternativa — um sinalizador `deleted: true` no front matter, com o arquivo permanecendo onde está — foi rejeitada: todo leitor do store teria então que saber sobre o sinalizador, e aquele que esquecesse listaria, pesquisaria, convocaria ou restauraria uma nota que o usuário havia excluído. Listagem, pesquisa, atualidade e inicialização leem o diretório de notas, portanto, retirar o arquivo dele é o que faz com que uma nota excluída deixe de ser uma nota em todos os lugares ao mesmo tempo, sem nenhuma regra adicionada em nenhum lugar.

**A movimentação é o ponto de commit**, a mesma regra ADR-020 estabelecida para um save. A ordem é flush, move, state, surface e cada falha antes da movimentação deixa a nota aberta, ativa e editável. Essa ordem é o recurso completo: uma nota cujo texto mais recente não pôde ser escrito nunca deve desaparecer da tela como se tivesse sido excluída, pois o leitor veria uma exclusão e cobraria uma edição por ela. Após a movimentação, a nota *está* na lixeira, portanto, nem a gravação do estado da janela nem a desmontagem da superfície podem informar o contrário - a gravação do estado é o melhor esforço, e a janela segue em qualquer direção, porque uma janela que ainda mostra uma nota cujo arquivo foi movido está mostrando algo que não está lá. `commit_trash` é uma função livre em três fechamentos, portanto cada uma dessas falhas é um teste e não uma afirmação.

**Duas direções, duas ferramentas, para dois riscos diferentes.** Mover *para* a lixeira é `rename`: um syscall, portanto não há nenhum instante em que a nota esteja ativa e excluída. Mover *voltar* é `hard_link` seguido por `remove_file`, porque `rename` substituiria silenciosamente uma nota ao vivo carregando o mesmo identificador, e verificar primeiro apenas estreita a corrida em vez de encerrá-la. `hard_link` recusa atomicamente um nome existente, portanto "a restauração nunca substitui uma nota ativa" é uma propriedade do syscall e não de uma verificação. É também a preservação mais rigorosa possível do arquivo: o nome restaurado é o mesmo inode, não uma cópia dele. A assimetria é o ponto - cada direção usa o primitivo que torna impossível sua própria falha perigosa, e o restante que uma desvinculação com falha poderia produzir (uma nota ativa e listada na lixeira) é visível e inofensiva, onde uma substituição silenciosa não seria.

**Nada é escrito na nota.** A data em que foi excluída fica em um arquivo secundário `<uuid>.json` ao lado do arquivo. Escrevê-lo no front matter significaria que o arquivo que volta não é o arquivo que entrou e faria da lixeira uma segunda opinião sobre o conteúdo da nota. Um sidecar ausente ou ilegível carrega essa data exata e nada mais: em vez disso, o próprio horário de modificação do arquivo responde e nada é escrito para corrigi-lo. A consequência que vale a pena mencionar é que uma nota Note-it que não consegue sequer analisar — ​​danificada front matter, editada à mão YAML — ainda vai para a lixeira e ainda retorna byte por byte, porque a lixeira move os arquivos e nunca os lê.

**Lixeira e restauração não são edições.** Nenhum dos dois abre, analisa ou serializa a nota, então `updated_at` não pode se mover: uma nota restaurada retorna exatamente à posição no alternador rápido que estava, em vez de fingir que acabou de ser escrita. A entrada do estado da janela é definida como fechada em vez de removida, portanto, uma nota que retorna volta ao tamanho e ao local em que estava - e uma entrada obsoleta nomeando uma nota que não está mais em `notes/` é inerte, porque o que a inicialização restaura vem dos arquivos no disco.

**Sem exclusão permanente e sem esvaziar a lixeira.** Ambos foram deliberadamente deixados de fora desta fase. A fase é de recuperação, e uma interface que oferece destruição irreversível ao lado de um botão de restauração é aquela em que o clique errado não pode ser desfeito. O lixo é, portanto, ilimitado, o que é uma limitação real e está escrito como tal; uma pessoa que deseja o espaço de volta pode excluir arquivos de `trash/` com qualquer gerenciador de arquivos, que é uma propriedade de armazenar notas como arquivos comuns.

**O painel é um painel.** A lixeira reutiliza a forma da paleta de pesquisa — um elemento na página, não uma superfície de segunda camada para colocar, focar, empilhar e desmontar. Rótulos e snippets são escritos com `textContent`, exatamente como os resultados da pesquisa, e cada ação aborda um `Uuid`: não há nenhuma mensagem na ponte que carregue um caminho, então `../../etc/passwd` não é uma solicitação que possa ser escrita.

## ADR-029: Um backup é um diretório de arquivos obtidos antes da alteração contra a qual ele protege

A segunda metade da fase 3.9 é um instantâneo local de tudo o que pode ser recuperado: `notes/`, `trash/`, `config.toml` e `state.json`, copiado em `backups/<timestamp>/`.

**Um diretório simples, não um arquivo.** Sem tar, sem zip, sem banco de dados, sem formato próprio do Note-it. O que quer que tenha dado errado, um instantâneo pode ser lido com `ls` e colocado de volta com `cp`, e recuperá-lo não requer nada que possa ser quebrado. Compactá-lo compraria espaço em um store medido em kilobytes e custaria a única propriedade que importa quando um backup for finalmente necessário.

**Nada sai da máquina.** Nenhum servidor, nenhuma nuvem, nenhum WebDAV, nenhum Git remoto, nenhum cliente HTTP — e nenhum foi adicionado, portanto não há nenhuma superfície de rede para auditar aqui. A consequência honesta está escrita e não implícita: um instantâneo local protege contra uma exclusão acidental, uma corrupção lógica, uma edição para desfazer, uma versão para a qual voltar. Ele fica no mesmo disco que as notas, portanto não protege contra **nenhuma** unidade morta, máquina perdida ou roubada, e não é criptografado. Vender backup local como recuperação de desastres seria o tipo de promessa que este projeto não faz.

**Realizado antes da primeira alteração elegível, não depois dela e não em um cronômetro.** A verificação é executada no início de uma mutação persistente — um salvamento de nota, uma movimentação para a lixeira — e em nenhum outro lugar. Um cronômetro que despertasse o processo para perguntar se um dia se passou seria um trabalho contínuo em uma aplicação cujo custo ocioso é um recurso; e um instantâneo tirado *após* uma edição é um instantâneo do estado do qual você queria fugir. Portanto, um daemon que ninguém está usando não faz absolutamente nada, e um daemon deixado aberto por uma semana tira seu instantâneo no momento em que seu proprietário começa a digitar novamente.

**O armazenamento é seu próprio registro de quando foi feito o último backup.** O manifesto do instantâneo válido mais recente responde "quando", portanto, não há arquivo de contabilidade para gravar, perder, versionar ou manter honesto - e nenhum estado que possa discordar do que está no disco. É lido uma vez por sessão e lembrado, pois a pergunta é feita antes de cada salvamento automático e deve ser liberada quando a resposta for "ainda não", o que vale para todos, exceto um salvamento por dia. Uma tentativa com falha não é repetida por quinze minutos, portanto, um armazenamento cujos backups não podem ser gravados não tenta novamente a cada pressionamento de tecla.

**Um backup com falha nunca é um salvamento com falha.** Um snapshot é uma camada extra de segurança; transformar sua falha em uma recusa de gravação custaria ao leitor a edição que o backup existe para proteger. A falha é relatada para `stderr` e o salvamento é concluído. Um backup que o leitor *pediu* é o oposto: alguém está esperando para saber se tem um ponto de segurança, então sempre diz qual era, em uma linha no rodapé da nota, em vez de um diálogo sobre ela.

**A renomeação também é o ponto de confirmação aqui.** Um instantâneo é criado em `backups/.tmp.…` e renomeado no local inteiro. Um processo eliminado no meio sai de um diretório `.tmp.…`, que nunca pode ser confundido com um snapshot: ele não possui nome de snapshot e não possui manifesto, e ambos são obrigatórios. O próximo backup o varre – e varre apenas os diretórios que carregam esse prefixo, porque confundir o arquivo de uma pessoa com detritos seria uma falha pior do que os detritos.

**A retenção é executada somente depois que um novo instantâneo é confirmado.** Sete são mantidos, em um pool, independentemente do que os tenha gerado. Excluir um backup antigo para abrir espaço para um que falharia trocaria a proteção real por nenhuma, então a ordem é criar, confirmar, remover - e um instantâneo que não pode ser removido é relatado em vez de permitir a falha de um backup que já existe. As camadas diárias/semanais/mensais não foram construídas: sete instantâneos e uma regra é algo que uma pessoa pode ter em mente, e nada ainda diz que as camadas seriam usadas.

**Um backup nunca segue um link simbólico.** Somente arquivos regulares nos diretórios conhecidos são copiados, e um nome que começa com `.` é ignorado — o que também mantém um `.tmp.…` deixado por um salvamento interrompido fora de um instantâneo. Uma única entrada criada no armazenamento não deve ser capaz de fazer a cópia de backup `/etc`, um diretório inicial ou qualquer outra coisa fora dos dois diretórios que foi solicitada a copiar. `backups/` nunca é uma fonte, portanto, um instantâneo nunca pode conter instantâneos.

**Restaurar um store inteiro deliberadamente não é um botão.** Colocar um instantâneo de volta em um store ativo é uma transação com vários arquivos, e uma versão com um clique ao lado de uma entrada de menu normal é o controle mais destrutivo que o aplicativo pode oferecer. O que esta fase deve, em vez disso, é a prova de que o instantâneo *é* restaurável, e isso é um teste: um instantâneo é copiado em uma segunda árvore XDG vazia e aberto, e as notas, identificadores, Markdown, lixo, configuração e estado da janela voltam. O procedimento manual está escrito em `docs/storage.md` e é `cp` com o aplicativo fechado.

## ADR-030: O estado canônico do timer é um instante no tempo e não faz parte da nota

**Status:** Aceito (Fase 3.10)

Duas decisões, e tudo o mais na fase decorre delas.

### Uma contagem regressiva em execução é armazenada no momento em que termina

A implementação óbvia é um número e uma marca repetida:

```js
setInterval(() => { remaining -= 1000; render(); }, 1000);
```

Está errado exatamente nas situações para as quais existe um cronômetro. `setInterval` não promete um tick por segundo; não promete mais do que um. Um WebView que o compositor não está mostrando é acelerado, uma máquina sob carga entrega com atraso, um laptop suspenso não entrega nada - e cada tick perdido é um segundo que a contagem regressiva mantém silenciosamente. Volte do almoço e um Pomodoro de 25 minutos ainda faltam onze minutos. O erro não é um artefato de arredondamento que precisa ser ajustado; é o modelo que paga tudo o que o agendador entregou.

Portanto, uma corrida armazena `deadline` — um instante de relógio de parede — e cada leitura é `deadline - now`. Nada diminui. Um redesenho que chega tarde, ou nunca, custa uma *imagem* obsoleta e não uma *resposta* errada: a próxima leitura está correta sempre que acontecer. A deriva não pode se acumular porque não há acumulador.

A pausa é a imagem espelhada, e o espelho é importante. Uma corrida pausada não tem fim instantâneo – ela tem uma dívida – então o prazo é **descartado** e o restante congelado. Deixar um prazo desatualizado em um cronômetro pausado é exatamente o que faz com que o tempo pausado seja gasto de qualquer maneira na próxima vez que algo o ler, e é por isso que `sanitize` limpa o campo pertencente ao estado em que um registro não está e é por isso que há um teste para um registro pausado com um prazo.

`Date.now()` é o relógio, deliberadamente, e não `performance.now()`. O relógio monotônico é a resposta certa para "quanto tempo demorou" e a resposta errada para "quanto resta da tarde": em Linux ele não avança na suspensão, então uma máquina fechada por dez minutos voltaria acreditando que nenhum tempo havia passado. O tempo civil é a essência de um cronômetro, e um salto no relógio do sistema é um problema mais raro e visível do que um laptop suspenso.

A coisa toda é, portanto, reconstruível a partir de um número. Fecha o aplicativo às 14h05 com cronômetro iniciado às 14h por 25 minutos, reabre às 14h10, e os quinze minutos restantes são computados, não lembrados. Reabra às 14h30 e o estado é `finished` em vez de uma contagem regressiva até zero.

**A conclusão é protegida pela transição, não por um sinalizador.** Somente uma execução `running` pode ser concluída, e a conclusão a torna `finished`. Por mais que muitos redesenhos observem um prazo no passado, exatamente um deles é o que movimenta o estado — uma escrita, uma linha no rodapé da nota, uma notificação. A verificação e a atribuição são a mesma etapa, que é a única versão que não é uma corrida.

**A restauração nunca toca.** Uma execução que terminou enquanto o aplicativo estava fechado é restaurada como concluída e silenciosamente. Um alarme sobre o passado não é um alarme, e qualquer regra para “recente o suficiente para ainda soar” seria um número arbitrário. Em vez disso, o estado finalizado está na barra, que é o que realmente informa ao leitor.

### Um temporizador está em estado operacional, em `state.json`, e nunca em Markdown

O lugar tentador para colocá-lo é a nota: ela pertence a essa nota, e a nota é um arquivo que já possui front matter. Isso seria errado pelo mesmo motivo pelo qual o zoom não está na nota. O arquivo de uma nota é o documento do leitor – aquilo que ele escreveu, aquilo que pode abrir em qualquer editor, aquilo cuja data de modificação ordena sua troca rápida. Iniciar um cronômetro não é escrever.

Colocar o cronômetro no Markdown significaria: o arquivo muda quando ninguém o editou; `updated_at` se move, então uma nota salta para o topo do switcher porque uma contagem regressiva terminou; a lixeira e o índice de pesquisa veem uma chave que ninguém digitou; e `25:00` torna-se um texto localizável em uma nota que apenas possui um Pomodoro em execução. Cada um deles é um defeito, e nenhum deles vale uma casa conveniente por sete escalares.

Portanto, ele reside na entrada da nota em `state.json`, ao lado da geometria, do estado de recolhimento e do zoom — todos os quais já são "estado da aplicação sobre esta nota" em vez de "estado da nota". As consequências são as propriedades pelas quais a fase é julgada, e elas se mantêm estruturalmente e não por cuidado: a pesquisa lê `notes/`, a lixeira move os arquivos em `notes/`, o título recolhido é projetado a partir de Markdown e nenhum desses três abre `state.json`.

**Escrito em uma alteração, nunca em um tique.** Inícios, pausas, retomadas, cancelamentos, redefinições, mudanças de fase e conclusões são gravações; os segundos que passam não o são, porque o prazo armazenado não muda durante a contagem regressiva. Portanto, um cronômetro em execução não custa nenhum tráfego de disco e nenhum IPC. Uma nota cujo cronômetro está em seu estado original não armazena nada: o campo está ausente, então uma nota que nunca teve um parece exatamente como era antes desta fase existir.

**Um por nota, por construção.** O registro fica pendurado no identificador da nota e o mecanismo é uma máquina, portanto não há nenhum arranjo de cliques que inicie duas contagens regressivas em uma nota e nenhum slot compartilhado para que o cronômetro de uma nota apareça em outra. Alterar o modo não é uma maneira de contornar isso: as guias ficam indisponíveis enquanto uma execução está ativa e o mecanismo recusa a alteração de qualquer maneira. Deliberadamente, não existe um gerenciador de cronômetro global - uma nota é o escopo e uma segunda é uma segunda nota.

### A página não pode escrever uma notificação

A mensagem de conclusão na ligação é um `TimerFinishKind`, um valor de um conjunto fechado de quatro. As palavras estão `TimerFinishKind::notification` no host. A página informa *que tipo de execução* terminou e não possui nenhum campo para fornecer texto, portanto não há nenhuma rota pela qual uma linha de uma nota, um título ou um trecho possa chegar à área de notificação da área de trabalho - nem através de um bug na página, nem através de qualquer coisa que uma nota possa conter. A notificação também é opcional no sentido funcional: uma área de trabalho sem daemon de notificação não recebe nenhum e todo o resto do recurso permanece inalterado, porque o sinal do qual o recurso realmente depende é aquele dentro da nota.

### Quanto custa a divisão

O host não executa a contagem regressiva, portanto, um cronômetro não é concluído enquanto o aplicativo está oculto – a ocultação destrói os WebViews e não resta mais nada para observar o prazo. O preenchimento é então entregue quando a nota volta, dentro do prazo, que está correto, mas atrasado. A alternativa é uma segunda máquina de estado em Rust contendo um alarme `glib` por nota, e dois proprietários de uma conclusão são exatamente a forma que produz duas notificações. Mudar para outro aplicativo **não** oculta Note-it — as notas permanecem na tela com WebViews ao vivo, e é para esse caso que o recurso se destina — portanto, o adiamento se aplica apenas a um explícito "guardar tudo". Um proprietário, uma transição, uma notificação valeu a pena.

## ADR-031: Desligado significa que não há ouvinte, e o kit de ferramentas decide o que é nosso

**Status:** Aceito (Fase 3.11)

AutoPaste monitora a área de transferência do sistema. A área de transferência contém senhas, tokens, mensagens privadas, notas médicas e tudo o mais que alguém copiar, então as decisões abaixo são sobre isso antes de qualquer outra coisa.

### "Desligado" é a ausência de um ouvinte, não de uma ramificação dentro de um

A implementação fácil mantém um manipulador conectado e retorna mais cedo quando o modo está desativado. Ele se comportaria de forma idêntica e teria o formato errado, porque então "Note-it não olha para sua área de transferência" é uma afirmação sobre uma condicional - um refatorador, um booleano invertido, um retorno antecipado movido e silenciosamente deixa de ser verdadeiro.

Portanto, o manipulador `changed` está conectado exatamente em um local, quando uma nota é armada, e desconectado em exatamente um local, quando ela é liberada. Enquanto o AutoPaste está desativado, não há nada inscrito na área de transferência. `AutoPaste` em `autopaste.rs` ainda responde `NotArmed` por uma mudança que nunca deveria ver, porque vale a pena ter uma política total; mas a garantia não depende disso.

O mesmo raciocínio permeia o resto. `AutoPaste` contém quatro campos pequenos e nenhum deles é texto: não há última área de transferência, nenhum hash de um e nenhum buffer, portanto não há nada para vazar, nada para persistir e nada que precise ser lembrado para ser limpo. Os formatos são verificados antes de qualquer leitura, portanto uma imagem é recusada sem ser transferida. E nenhum conteúdo da área de transferência chega a um registro em qualquer nível — o diagnóstico registra a *forma* de uma decisão (`read`, `queued`, `ignored-own`, `ignored-not-text`) e nunca um byte do que estava nela.

### Se o modo está ativado não está escrito em lugar nenhum

O delimitador é uma preferência e reside em `config.toml`. Se o AutoPaste está *ativado*, não é armazenado deliberadamente em nenhum lugar: nem no Markdown, nem no `state.json`, nem na configuração, nem em um arquivo secundário.

Isto não é uma limitação e persistir não seria uma conveniência. Um modo que observa a área de transferência nunca deve voltar sozinho após uma reinicialização, uma falha, um logout ou uma atualização, porque a pessoa que o ativou na terça-feira passada para uma nota não está necessariamente consentindo com isso hoje. Não ter nada para restaurar é o que garante isso - não há nenhum campo em `LoadNote` que possa ativá-lo novamente, e é por isso que o teste afirma que isso é sobre o protocolo e não sobre o código.

### Um alvo, porque a área de transferência é uma coisa

Duas notas capturadas significariam cada `Ctrl+C` arquivado duas vezes, em dois lugares, o que é surpreendente na primeira vez e perigoso na décima. Assim, armar uma nota libera tudo o que a segurava, na mesma etapa, e ambas as notas são informadas: uma nota que perdeu o alvo ainda mostra que o possui de outra forma.

Não há deliberadamente nenhum gerenciador de captura, nenhuma fila de alvos e nenhum modo por nota. Um `Option<CaptureSession>` para o aplicativo é o modelo completo.

### A proteção contra repetição é `gdk_clipboard_is_local`, não uma comparação

Copiar dentro da nota que está sendo capturada não deve anexar as próprias palavras da nota a ela mesma. A solução tentadora é `if text == last_text { ignore }`, e está errada de uma forma que só aparece em uso: copiar `ABC` duas vezes de um navegador, em duas ações deliberadas, equivale a duas capturas, e a desduplicação de conteúdo devora silenciosamente a segunda para sempre.

A pergunta certa não é "este é o mesmo texto", mas "nós * o colocamos lá", e GDK responde. Um `Ctrl+C` ou `Ctrl+X` dentro de um WebView é este aplicativo que reivindica a área de transferência, então `is_local()` é verdadeiro e a alteração é recusada antes do início de qualquer leitura. É uma propriedade do kit de ferramentas e não uma heurística, e é verificada no único momento em que pode ser verificada de forma confiável.

**Quanto isso custa, declarado claramente:** `is_local()` é verdadeiro para todo o processo, portanto, copiar da nota B enquanto a nota A está sendo capturada também é recusado. Distinguí-los significaria o WebView relatando sua própria cópia e o host competindo contra o sinal de GDK no mesmo loop principal, sem nada ordenando os dois. Uma resposta errada é uma nota comendo seu próprio texto ou uma captura perdida silenciosamente, então a resposta conservadora é a honesta: Note-it captura de outros aplicativos, e a cópia nota a nota é feita colando. Esse é um limite real e está documentado e não encoberto.

### Uma geração, verificada quando a leitura chega

A leitura da área de transferência é assíncrona e tudo pode mudar enquanto ela está no ar: o modo desligado, o alvo movido para outra nota, a nota fechada, o aplicativo escondido. Portanto, cada execução armada carrega uma geração, cada leitura carrega a sessão em que foi iniciada e a verificação quando retorna é a igualdade exata em relação ao estado como está *então*. Armar e desarmar ambos cria uma nova geração, que é o que torna cada leitura já em voo obsoleta - incluindo aquela que de outra forma chegaria em uma nota que o leitor parou de capturar há pouco.

As leituras também são serializadas, uma de cada vez. Dois em vôo podem terminar em qualquer ordem, e as capturas que chegam como A, C, B seriam um defeito que ninguém poderia explicar. Uma alteração que chega durante uma leitura é lembrada e lida depois dela; vários se transformam em um, porque a área de transferência contém um valor e os intermediários já desapareceram.

Medido em uma sessão Niri real, em vez de presumida: GDK emite exatamente um `changed` por cópia ali, portanto, três cópias produzem três leituras e três capturas, e nenhuma janela de coalescência foi necessária.

### Desarmado antes do flush, nunca depois

Fechar uma nota, ocultar, sair e mover uma nota para a lixeira termina com um WebView sendo destruído e todos eles são liberados primeiro. O AutoPaste é desligado *antes* de ser liberado em cada um desses caminhos, então uma leitura ainda no ar não pode alcançar um documento que está prestes a ser escrito e rasgado. A verificação de geração já o recusaria; fazê-lo nesta ordem significa que a questão nunca surge.

### O host lê, a página insere, o salvamento comum grava

A captura vai do callback de leitura para o WebView da nota alvo e em nenhum outro lugar. Ela não é gravada em `.md` pelo observador, porque o WebView aberto possui o documento ativo e duas autoridades sobre um arquivo é como uma nota perde uma edição. A página a anexa por meio de uma transação normal do editor, o próprio caminho de atualização do editor é eliminado e o salvamento automático existente grava a nota - e é também por isso que uma captura se comporta como a edição que é: `updated_at` se move, a pesquisa encontra o texto e um salvamento com falha falha da mesma forma que todos os outros salvamentos com falha aqui.

Ativar ou desativar o modo e alterar o delimitador não toque em nada disso. Eles são o estado do aplicativo, portanto, deixam a nota byte por byte como estava.

## ADR-032: Uma imagem é um arquivo ao lado da nota, acessado por meio de um esquema

**Status:** Aceito (Fase 3.12)

### Os bytes são um arquivo e a nota contém um caminho

O atalho tentador é um `data:` URI no Markdown. Ele funciona imediatamente e estraga a finalidade do arquivo: uma captura de tela transforma uma nota que alguém pode ler, comparar, usar grep e editar manualmente em um megabyte de base64 que eles não podem, e faz o mesmo com cada backup e cada commit em que a nota aparece.

Portanto, os bytes vão para `assets/<note-uuid>/<asset-uuid>.<ext>`, um irmão de `notes/` e `trash/`, e a nota armazena `../assets/<note>/<asset>.<ext>`.

**Relativo e relativo a `notes/` especificamente.** `notes/` e `trash/` são irmãos, então `..` sobe para o mesmo diretório de dados de qualquer um deles: uma nota movida para a lixeira e restaurada não precisa ser reescrita, e a referência é válida durante todo o caminho. Um caminho absoluto teria que ser reescrito a cada movimento, quebraria no momento em que um armazenamento fosse copiado para outra máquina e gravaria o diretório inicial do leitor em um arquivo que eles poderiam colocar no Git.

**Os identificadores são nossos.** Qualquer que seja o nome do arquivo no disco do leitor, não é como é chamado aqui. Nada que um nome de arquivo possa carregar — um `..`, um separador, uma nova linha, um caractere de controle, uma dobra maiúscula que signifique algo diferente em outro local — sobrevive em um caminho, porque nada disso é usado para construir um. O formato é decidido da mesma forma: pelos primeiros bytes, nunca pela extensão. Um PNG chamado `.txt` é um PNG e um SVG chamado `.png` ainda é um SVG e ainda é recusado.

**SVG é recusado, e por construção e não por uma regra sobre ele.** É um formato de documento que pode conter scripts e referências externas. Admitir isso significaria auditar toda aquela superfície em prol de uma imagem. Não possui assinatura binária, então o mesmo sniffing que aceita os outros quatro o rejeita sem um caso especial.

### A página pede uma foto; nunca nomeia um arquivo

Um `<img src="file:///home/…">` teria funcionado. Ele também teria colocado um caminho absoluto do sistema de arquivos ao alcance da página, em um aplicativo cujo contrato de front-end é que ele nunca soletre um - a pesquisa leva um `Uuid`, o lixo leva um `Uuid`, e não há nenhuma mensagem na ponte carregando um caminho precisamente para que não haja nada para percorrer.

Então o host registra `note-it-asset:` e o veicula. A página carrega `note-it-asset:/<note>/<asset>.<ext>` e o manipulador analisa ambas as metades como `Uuid`s antes que qualquer coisa toque o disco. Um `..`, um caminho absoluto, um segmento extra, um separador codificado por porcentagem: nenhum deles resolve para um arquivo, porque nenhum deles *analisa*. A travessia não é bloqueada por uma verificação que possa ser contornada; é irrepresentável.

A Política de Segurança de Conteúdo da página foi ampliada exatamente por esse esquema — `img-src 'self' note-it-asset:` — e por nada mais. Não `http:`, não `https:`, não `data:`, não `file:`. Uma nota não pode buscar nada, o que também é a resposta para imagens remotas: alguém digitado à mão percorre o texto que é e é desenhado sem nenhuma fonte. A abertura de uma nota chega à rede de graça e não pode ser usada para informar a ninguém que ela foi aberta.

Medido antes de ser construído: um ativo sintético servido desta forma carrega no WebKitGTK real sob a política real, reportando suas verdadeiras dimensões. Os ícones na Fase 3.9UX falharam silenciosamente sob esta mesma política, razão pela qual isto foi medido primeiro em vez de assumido.

### Duas formas armazenadas e uma regra para escolher entre elas

A sintaxe da imagem de Markdown não tem onde definir largura ou alinhamento, e essas são duas das quatro coisas que esta fase existe para oferecer. HTML possui, e esta base de código já armazena o que Markdown não pode como tags inline canônicas - `<span data-note-it-color>`, `<mark data-note-it-highlight>`.

Portanto: simples `![alt](src)` enquanto não há nada a dizer além de onde a imagem está, e um canônico `<img>` quando uma largura ou um alinhamento não padrão for escolhido. A regra é determinística em ambas as direções — o alinhamento padrão normaliza *de volta* para a forma simples — então uma imagem é sempre um conjunto de bytes e um salvamento que não mudou nada não muda nada no disco.

A tag é canonizada pela mesma função que canoniza um `<span>`, sob a mesma disciplina: quatro atributos, sempre em uma ordem, cada um validado em vez de copiado, e a fonte deve ser um dos ativos gerenciados pela próprio store. Um `onerror`, um `style`, um `srcset` ou um caminho saindo do diretório de ativos não é escapado e nem mantido - a tag simplesmente não é uma das nossas e é descartada.

**O texto alternativo é armazenado e nunca projetado.** Cada imagem que este aplicativo insere carrega `alt=""`. Isso é o que mantém uma nota contendo uma imagem e nenhuma palavra ainda sem nome, e o que mantém o identificador de um ativo fora da pesquisa - e isso significa que o formato simples e o formato da tag concordam sobre o que uma nota diz, o que não aconteceria se um alt derivado do nome do arquivo fosse projetado de um e retirado do outro. Um `![alt](url)` escrito à mão mantém o comportamento que sempre teve.

### O host armazena; a página edita; o salvamento comum grava

O host nunca grava o `.md`. Ele pega bytes, decide o que são, armazena-os e envia de volta um caminho relativo; a página coloca isso no documento por meio de uma transação normal do editor e o salvamento automático existente o carrega para o disco. Uma autoridade sobre o documento, que é a mesma regra que uma captura da área de transferência segue e o motivo pelo qual uma imagem se comporta como a edição: `updated_at` se move, a pesquisa encontra as palavras ao seu redor e um salvamento com falha falha da mesma forma que todos os outros salvamentos com falha aqui.

As três maneiras diferem apenas na origem dos bytes. O seletor de arquivos é a caixa de diálogo do próprio host, portanto o caminho é aquele escolhido pelo *leitor* em vez de aquele nomeado pela página. Colar e soltar dá à página um `File`, e a página envia seus bytes - base64 para o comprimento de uma mensagem, nunca nada que chegue a uma nota. Em nenhum dos três a página consegue apontar o host para um arquivo.

### Um instantâneo também contém as fotos (3.12R)

Isso não aconteceu quando o 3.12 foi lançado. Os bytes foram para `assets/` e o backup continuou copiando `notes/`, `trash/`, `config.toml` e `state.json`, portanto, um instantâneo obtido entre restaura o Markdown de uma nota e não o arquivo para o qual seu `![](../assets/…)` aponta. Um backup cuja promessa é "tudo recuperável" que contém silenciosamente meia nota é pior do que aquele que nunca a reivindicou, e o 3.12 não foi aceito até que este fosse fechado.

`assets/` é uma árvore em vez de um diretório simples, portanto, ele obtém uma cópia própria, em vez de uma simples, para notas sendo liberadas em uma recursão geral - uma rotina que desce onde quer que encontre um diretório é como um backup acaba seguindo algo da árvore que foi solicitado a copiar, e colocaria em risco as próprias garantias das notas para servir uma forma diferente.

É estrito onde a cópia das notas perdoa, e a assimetria é o ponto. `notes/` contém arquivos que uma pessoa pode razoavelmente ter colocado lá, portanto, uma estranheza é ignorada com um aviso. `assets/` foi escrito por Note-it e por nada mais, então qualquer coisa que não seja `<note-uuid>/<asset-uuid>.<ext>` significa que o armazenamento não está no estado que acredita estar - e a única coisa que um backup nunca pode fazer é omitir o conteúdo gerenciado enquanto relata o sucesso. Nenhum link simbólico é seguido em nenhum dos níveis; cada nome é validado pelo mesmo `parse_asset_request` que o esquema URI usa, portanto, um instantâneo contém exatamente os arquivos que o aplicativo pode servir.

Uma imagem para a qual nenhuma nota aponta é copiada como o resto. Decidir que é dispensável seria a coleta de lixo que esta fase deliberadamente não faz, alcançada por omissão e não intencionalmente.

A transação é aquela que já estava lá: a cópia acontece dentro do diretório scratch, antes da renomeação que é confirmada, portanto, uma falha na cópia de uma imagem não deixa nenhum snapshot, nenhum manifesto e nenhum antecessor removido. O manifesto passa para a versão 2 e registra a contagem; a versão 1 continua analisando, porque o campo é padrão e nada se ramifica no número - todos os instantâneos no disco hoje foram gravados pela versão 1 e nenhum deles pode se tornar ilegível.

### Remover uma imagem deixa o arquivo

Tirar uma imagem de uma nota tira-a da nota. Os bytes permanecem.

Não há mais coleta automática de ativos nem notas, e isso é deliberado e não inacabado. Decidir que um arquivo não é utilizado significa ter certeza sobre cada nota, incluindo aquelas que estão na lixeira, aquelas que estão sendo editadas em um WebView que ainda não foi salvo e aquelas que um backup irá restaurar posteriormente - e estar errado destrói algo que o leitor não pode recuperar. Manter um arquivo que ninguém faz referência custa espaço em disco; excluir algo que alguém faz custa a imagem.

O arranjo deixa possível uma varredura futura: os ativos são agrupados por identificador de nota, de modo que o conjunto de referências ativas é `notes/` mais `trash/` analisado para `../assets/…`, e qualquer coisa abaixo de `assets/<id>/` não nomeada é um candidato. Se isso for construído, deve ser algo que o leitor peça e possa ver o resultado primeiro, e não algo que funcione por conta própria.

## ADR-033: Os metadados semânticos residem no Markdown, mas o YAML fica sob responsabilidade do Core

**Decisão.** Tags e propriedades textuais V1 são valores de nível superior no front matter ao lado do mapeamento `note_it` reservado. `noteit-core` possui sua validação, identidade, ordenação, persistência e catálogos derivados. Os adaptadores recebem estruturas de domínio; nem o WebView nem um futuro CLI analisam novamente YAML.

**Identidade.** Reutilização de tags e chaves de propriedade `search::fold`: letras minúsculas Unicode mais a tabela de acentos latinos documentada. A primeira grafia da tag é apresentação; identidade dobrada é comparação. Um hash FNV-1a fixo dessa identidade seleciona um dos sete slots de cores UI revisados, de modo que a cor é estável e nunca armazenada.

**Limites.** Uma nota tem no máximo 32 tags de 64 caracteres e 32 propriedades com chaves de 64 caracteres e valores de linha única de 512 caracteres. A rejeição é explícita e o truncamento nunca ocorre. Os valores V1 são strings: adicionar tipos posteriores pode estender a representação do domínio sem alterar as strings existentes, mas objetos aninhados, esquemas, relações e fórmulas estão deliberadamente ausentes agora.

**Preservação.** O wrapper front matter digitado nivela o nível superior desconhecido YAML em um mapa privado e grava esses valores de volta. Isto é preservação semântica, não uma árvore de sintaxe concreta: serde_yaml não retém comentários, aliases/âncoras ou espaços em branco originais. Eles podem normalizar em um salvamento real, que está documentado; uma abertura/fechamento intocada nunca serializa e permanece idêntica em bytes.

**Transações e datas.** Uma solicitação de metadados transporta o Markdown ativo. O host valida o rascunho, clona seu `NoteDocument` ativo, dobra qualquer texto pendente no mesmo candidato, chama o caminho `StorageManager::save_note_atomic` e adota/reconhece somente após renomear commits. Uma gravação com falha deixa o disco e a memória antigos e o mesmo rascunho pode ser repetido novamente. A mudança apenas semântica não afeta nenhum carimbo de data/hora; movimentos de texto pendentes `updated_at` porque o texto foi alterado.

**Catálogos e leituras limitadas.** Os catálogos examinam notas ativas sob demanda e, portanto, não podem ficar obsoletos; o lixo é excluído pela associação ao diretório. Não existe índice ou banco de dados. As leituras apenas do front matter param no delimitador real e no limite funcionam em 256 KiB, confortavelmente além de cada campo V1 em seu limite. Isso substitui a suposição de atualidade de 4.096 bytes sem a leitura do corpo das notas.

**Alternativas rejeitadas.** Sidecars separam uma nota de seus metadados portáteis e criam uma segunda transação. Um índice de tags persistente introduz invalidação e recuperação antes que a medição solicite isso. Enviar YAML por meio de IPC torna WebView outra autoridade de formato. Colocar metadados em ProseMirror faz com que a pesquisa, os títulos e o estudo interpretem a contabilidade como prosa. Todos os quatro são rejeitados.

## ADR-034: Separação entre a CLI headless (`noteit`) e o adaptador de desktop (`note-it`)

**Decisão.** Crie um binário headless `noteit` separado em um membro do espaço de trabalho `noteit-cli` dedicado em vez de incorporar a funcionalidade CLI no binário GUI de desktop existente (`note-it`). Ambos os executáveis ​​consomem `noteit-core` como domínio compartilhado e autoridade de persistência.

**Justificativa.**
1. **Zero sobrecarga GUI.** `noteit` deve operar em ambientes headless (SSH, contêineres, scripts, agentes) sem exigir um servidor de exibição X11 ou Wayland, inicialização GTK, tempo de execução WebKitGTK ou registro de barramento de sessão `GApplication`.
2. **Preservando o ciclo de vida da área de trabalho.** `note-it` continua sendo um adaptador de área de trabalho especializado e um gerenciador de ciclo de vida de instância única para janelas de notas adesivas. Modificar seu despachante de comando para tarefas CLI avançadas combinaria o ciclo de vida do desktop com a semântica CLI não interativa.
3. **Isolamento estrito de dependência.** `noteit-cli` depende apenas de `noteit-core` e de bibliotecas leves headless (`clap`). O script de limite `scripts/check-cli-boundary` e CI garantem que nenhuma dependência de desktop entre em `noteit-cli`.
4. **Resolução de caminho puro.** `noteit status` deve ser estritamente somente leitura e nunca criar diretórios ausentes no disco. A resolução do caminho foi extraída em `StorePaths::resolve()` puro em `noteit-core`, reutilizada por `StorageManager` somente ao inicializar ou abrir armazenamentos.
5. **Bilíngue UX e apresentação de erro humano.** A apresentação humana é em português (`ajuda`, `versao`, `status`), com aliases internacionais padrão (`help`, `version`, `status`, `--help`, `-h`, `--version`, `-V`). Os erros de uso do Clap são mapeados para mensagens em português fáceis de usar no stderr usando o tipo `ErrorKind` e o contexto de erro sem ignorar o Clap como autoridade de análise.
6. **Autoridade de versão do workspace.** A versão do projeto é centralizada em `[workspace.package]`, com `version.workspace = true` em todos os crates (`note-it`, `noteit-core`, `noteit-cli`), evitando divergências de versão.

### ADR-035: Arquitetura da API de leitura headless e limites de segurança

**Decisão.** Implemente uma inspeção headless estritamente somente leitura API em `noteit-core` e `noteit-cli`, expondo listagem de notas, recuperação de notas individuais, pesquisa, catálogos de tags/propriedades, extração de tarefas e inspeção de lixo.

**Justificativa.**
1. **Projeções de leitura centradas no núcleo.** Toda a lógica de leitura do domínio (filtragem, derivação de título canônico via `search::label_for`, análise de tarefas e correspondência de metadados) reside diretamente em `noteit-core`. `noteit-cli` continua sendo um adaptador focado exclusivamente na análise de argumentos CLI e na apresentação do terminal.
2. **Modo aberto estritamente somente leitura.** `NoteItCore::open_read_only()` e `StorageManager::open_read_only()` inspecionam caminhos sem chamar `ensure_directories()`. Os stores ausentes retornam resultados limpos e vazios com código de saída 0 em vez de criar diretórios vazios ou arquivos de estado.
3. **Resolução segura do seletor de notas.** Os seletores de notas (UUID completo ou >= 8 caracteres hexadecimais) são validados em relação à passagem de caminho (`..`, `/`, `\`) e ​​caracteres não hexadecimais antes da correspondência de prefixo com IDs de notas ativas. Prefixos ambíguos, IDs inexistentes e links simbólicos falham de modo seguro (fail-closed) com o código de saída 1.
4. **Segurança e higienização de terminais.** A saída renderizada para terminais é higienizada (`output::sanitize_for_terminal`) para neutralizar códigos de escape ANSI (sequestro de área de transferência CSI, OSC, OSC 52), BEL, backspaces e caracteres de controle, evitando que conteúdo malicioso de notas manipule estados de terminal.
5. **Análise de tarefas e integridade de carimbo de data e hora.** As caixas de seleção de tarefas (`- [ ]`, `- [x]`, `- [X]`) e ​​o aninhamento de profundidade são extraídos exclusivamente do texto Markdown fora das cercas de código (``` e ~~~) e front matter. Os carimbos de data e hora `completed_at` são extraídos apenas de marcadores de comentários ISO 8601 válidos, sem nunca inventar carimbos de data e hora para datas ausentes ou não analisáveis.
6. **Zero mutações de armazenamento.** Nenhum arquivo de estado, backup, arquivo temporário ou estrutura de diretório é tocado durante as operações de leitura. A integridade do armazenamento byte por byte é comprovada por portas de teste.

## ADR-036: Proteção do contrato da API de leitura, datas locais e avisos tipados

**Decisão.** Padronize a apresentação humana de data e hora em `noteit-cli` para usar o fuso horário local da máquina correspondente ao contrato GUI, expanda a limpeza de entrada do terminal para todas as strings não confiáveis ​​renderizadas, desacople avisos de leitura não fatais em `ReadWarning` / `ReadBatch<T>` digitados em `noteit-core` com zero instruções de impressão e alinhe comentários de metadados de tarefas que correspondam estritamente à especificação TypeScript.

**Justificativa.**
1. **Consistência de fuso horário local (`dd/MM/yyyy HH:mm`).** Os usuários humanos esperam que os carimbos de data/hora exibidos pelo CLI correspondam ao fuso horário da máquina local visto na interface do desktop. A formatação de data e hora é centralizada em `output::format_datetime_local` em `noteit-cli`, enquanto os modelos `noteit-core` permanecem estritamente digitados em UTC (`DateTime<Utc>`).
2. ** Sanitização abrangente de entradas não confiáveis. ** Todas as entradas variáveis ​​ou externas renderizadas para stdout ou stderr são higienizadas via `output::sanitize_for_terminal` antes do estilo ou da saída. Isso inclui consultas de pesquisa em cabeçalhos, seletores de notas em mensagens de erro, contextos de argumentos Clap em erros de uso e caminhos XDG personalizados em `noteit status`.
3. **Modelo de aviso Core puro e desacoplado.** `noteit-core` não deve imprimir diretamente em stdout ou stderr com `println!` ou `eprintln!`. Os métodos de leitura retornam `ReadBatch<T>` contendo itens analisados ​​​​e estruturas `ReadWarning` digitadas (`note_id`, `kind`, `message`). O adaptador CLI formata esses avisos para stderr em português, enquanto futuros adaptadores JSON ou MCP podem projetá-los em cargas de erro estruturadas.
4. **Análise fiel de comentários da tarefa.** Os comentários de conclusão da tarefa `<!-- note-it:completed_at=... -->` são correspondidos em qualquer lugar na linha da tarefa sem exigir que sejam o primeiro comentário HTML. Apenas o comentário de metadados Note-it é removido de `TaskEntry.text`, preservando os comentários HTML de autoria do usuário. As tarefas não verificadas eliminam quaisquer carimbos de data/hora de conclusão.

## ADR-037: Pureza do pipeline de leitura, unificação de avisos de pesquisa e separação de consultas de domínio

**Decisão.** Unifique o pipeline de pesquisa Core para garantir políticas idênticas de aviso e carregamento em pesquisas filtradas e não filtradas em todo o universo elegível, remova todas as impressões stderr diretas dos caminhos de leitura de Core, separe as consultas de pesquisa de domínio da limpeza de apresentação e aplique a correspondência estrita de token nos comentários de metadados de tarefas.

**Justificativa.**
1. **Pipeline de pesquisa unificado.** `noteit buscar X` e `noteit buscar X --tag Y` usam o mesmo pipeline de coleta `load_note` e `ReadWarning` em `NoteItCore::search_notes_filtered`. A pesquisa não filtrada não ignora mais a geração de avisos, e as notas corrompidas emitem consistentemente avisos estruturados em ambos os modos, sem abortar a verificação.
2. **Verificando todo o universo qualificado.** As consultas de pesquisa verificam todas as notas qualificadas antes de aplicar o limite de resultados especificado pelo usuário (`--limite`), garantindo que as correspondências em notas mais antigas ou de menor atualidade não sejam perdidas.
3. **Erradicação de impressões diretas em caminhos de leitura.** Removido o `eprintln!` restante em `StorageManager::read_bodies`. Todos os métodos de leitura Core retornam dados puros, erros ou avisos digitados.
4. **Separação de consulta de domínio.** A consulta bruta do usuário é fornecida diretamente para `noteit-core` sem alteração, garantindo que a lógica de pesquisa opere no termo de pesquisa pretendido. A higienização do terminal é aplicada estritamente durante a renderização da apresentação.
5. **Correspondência estrita de Regex de comentário de tarefa.** A extração de metadados de tarefa valida que `<!-- note-it:completed_at=... -->` contém exatamente um token de carimbo de data/hora sem espaço em branco. Comentários com lixo à direita (por exemplo, `<!-- note-it:completed_at=2026-08-27T11:32:00Z lixo -->`) não correspondem ao regex de metadados Note-it e não são modificados no texto da nota.

## ADR-038: Um gravador do Note-it por store e a barreira que garante isso

**Decisão.** Exatamente um processo Note-it pode gravar um store por vez, restrição imposta por um `flock` consultivo em um arquivo de bloqueio no diretório de runtime. A instância de desktop adquire esse lease na inicialização e o mantém até o processo terminar; a CLI o adquire durante um comando quando está livre e, quando não está, envia a alteração para quem o detém por um soquete de domínio Unix privado, em vez de gravar diretamente o arquivo. Uma nota aberta em uma janela é alterada somente após seu editor ter sido congelado e seu texto ativo coletado, e tudo o que a página envia posteriormente carrega uma geração de runtime que o host verifica.

**Justificativa.**

1. **Uma gravação atômica não é suficiente.** `write_atomic` mantém um *arquivo* inteiro; não diz nada sobre dois processos em que cada um lê uma nota, cada um altera sua própria cópia e cada um a escreve de volta. Ambas as gravações foram bem-sucedidas, os dois arquivos estão intactos e a edição de uma pessoa desapareceu - e nada na camada de armazenamento pode ver isso acontecer, porque de onde está, ambas as gravações estavam corretas. A exclusão, portanto, deve estar acima do arquivo e deve ser o mesmo mecanismo em ambos os adaptadores ou não será exclusão de forma alguma.

2. **Um bloqueio, não um arquivo.** O lease é `flock` em um arquivo de bloqueio, nunca a existência desse arquivo. Um processo que trava o libera imediatamente, porque o kernel fecha seus descritores; um arquivo de bloqueio deixado por um processo inativo não bloqueia ninguém. Nenhum PID é confiável, nenhum carimbo de data e hora é comparado e nenhuma desatualização é adivinhada – todos os três são maneiras de estar errado sobre se alguém está lá. A biblioteca padrão do Rust fornece isso desde 1.89, portanto não custa dependência.

3. **Codificado por store, não por máquina.** Uma store de teste isolada e o store real são dois stores diferentes com dois gravadores legítimos ao mesmo tempo. O diretório de coordenação recebe o nome de um resumo determinístico do diretório de notas, portanto, eles nunca competem — e um teste nunca pode travar contra o aplicativo que seu autor está usando.

4. **Tempo de execução, não armazenamento.** Um bloqueio e um soquete descrevem esta inicialização. Eles não têm sentido após uma reinicialização, nunca devem ser copiados e não devem ser colocados ao lado das notas. `$XDG_RUNTIME_DIR` é o diretório que a especificação define exatamente para isso. Ambos os diretórios são criados `0700` e recusados ​​se forem um link simbólico ou pertencerem a outro usuário; o soquete está `0600` dentro deles.

5. **A instância do desktop é a autoridade porque só ela pode ser.** Uma nota aberta em uma janela pode conter um parágrafo que o arquivo ainda não possui. O único processo que pode escrever essa nota com segurança é aquele que pode solicitá-la à janela, portanto, enquanto Note-it está em execução, tudo passa por ele. O lease é mantida durante toda a sessão e liberada somente quando o processo termina, pois é nesse momento que ele deixa de poder salvar.

6. **Fluxo não é barreira.** Pedir o texto da página e depois escrever tem uma lacuna: o leitor continua digitando, a resposta já está desatualizada quando chega e o caractere digitado no meio é sobrescrito. Assim, a página deixa de ser editável *primeiro* e depois lê seu próprio texto. O congelamento está na transação, não apenas na editabilidade – a editabilidade interrompe o leitor e a própria página altera os documentos por meio de comandos que não se importam com isso.

7. **Uma geração, então nada em andamento pode desfazer um commit.** Cada gravação externa confirmada move um contador de tempo de execução. Cada mensagem da página que contém conteúdo cita a geração contra a qual foi composta, e o host recusa qualquer coisa mais antiga. Sem ele, um salvamento automático que saísse da página antes do commit iria parar depois dela e colocaria o corpo anterior de volta.

8. **Recusado e confirmado nunca são confundidos.** Uma gravação que falhou antes do ponto de confirmação não mudou nada e pode ser repetida. Uma gravação que foi confirmada, mas não conseguiu atualizar a janela, *não* é uma falha, e relatá-la como alguém anexaria o mesmo parágrafo duas vezes. Uma conexão que caiu depois que a solicitação foi encerrada não é nenhuma das duas coisas e é relatada como desconhecida - porque adivinhar de qualquer maneira é como uma nota termina com texto duplicado.

9. **Referências de tarefas são instantâneos otimistas, não identidade.** A Fase 4.0D deliberadamente não deu às tarefas nenhum identificador persistente e isso não contrabandeia nenhum: nenhum arquivo secundário, nenhum banco de dados, nada escrito no Markdown. Uma referência é recalculada a partir da nota no momento da escrita e recusada se não nomear mais exatamente uma tarefa. Ser instruído a listar as tarefas novamente é um resultado muito melhor do que marcar silenciosamente uma tarefa diferente.

10. **Privado e permanecendo assim.** O protocolo de controle é um soquete Unix local que carrega
com prefixo de comprimento JSON. Não há TCP, nem HTTP, nem porta e nem servidor localhost, e uma solicitação
não pode carregar um caminho do sistema de arquivos porque não há campo para colocá-lo. É uma implementação
detalhe da transferência - não a interface legível por máquina para a qual a Fase 4.0F está reservada - e
nada fora deste repositório pode depender disso.

## ADR-039: A instância de desktop é proprietária do store ou não inicia, e a página declara a adoção

**Decisão.** Uma instância de desktop Note-it que não pode aceitar o lease de escrita *e* abrir seu soquete de controle se recusa a iniciar, em vez de executar sem ser a autoridade do store. Considera-se que um documento confirmado chegou à janela somente quando a própria página envia `ExternalWriteApplied`; avaliar o script que o carregou não é tratado como prova. E depois que a página entrega seu instantâneo, ela nunca libera o documento em um prazo próprio — apenas `ApplyExternalDocument` ou `AbortExternalWrite` o descongela.

**Justificativa.**

1. **A invariante do ADR-038 não foi realmente aplicada.** A primeira implementação manteve a autoridade como `Option` e continuou quando era `None`: uma instância que não conseguiu aceitar o lease ainda abria janelas, ainda salvava automaticamente, ainda escrevia notas. Esse é um segundo gravador, produzido pelo código destinado a evitá-lo. "Exatamente um gravador por store" é uma propriedade do sistema ou é um comentário, e um campo opcional o torna um comentário.

Agora é um tipo. `AppContext` contém `WriteAuthority` por valor, a única maneira de obter um é um `claim` completo e a reivindicação acontece antes de existir qualquer janela, documento ou salvamento automático. Um Note-it editável e em execução que não possui seu armazenamento não é um estado que este programa pode descrever.

2. **Um lease sem soquete também não é autoridade.** Se o soquete de controle não puder ser aberto, `noteit` encontra o armazenamento retido e seu detentor inacessível - portanto, ele recusa corretamente todas as gravações e a instância do desktop bloqueou todos os outros fora de um armazenamento que só ele pode alterar. A inicialização falha e o lease é liberado na saída, o que é estritamente melhor do que um processo se tornar silenciosamente o único gravador que funciona.

3. **Não existe um modo somente leitura, deliberadamente.** Seria um terceiro estado para raciocinar, e a resposta honesta para "outra coisa possui suas anotações" é uma frase, não um aplicativo degradado.

4. **`evaluate_javascript` retornando `Ok` prova que o script foi executado.** Isso não prova que a mensagem foi roteada para um ouvinte, que o ouvinte correspondeu à solicitação ou que o documento foi adotado — e a página detecta seus próprios erros de ouvinte, portanto, uma falha dentro de um ainda relata uma avaliação bem-sucedida. Tratar a entrega como adoção significava que uma janela mostrando o texto pré-comprometido poderia ser relatada como sincronizada. A própria página agora o diz, nomeando a nota, a solicitação e a geração que ocorreu, e somente depois de adotar o documento e retomar a edição. A falha na entrega ainda é usada, mas apenas para falhar rapidamente: um script que não pôde ser avaliado certamente não atualizou nada.

5. **Uma adoção rejeitada é respondida, não deixada em silêncio.** `ExternalWriteApplyFailed` carrega a nota e a solicitação e nada mais — sem motivo, sem pilha, sem conteúdo da nota, porque o host age de acordo com o se e nunca sobre o porquê. Custa uma mensagem e evita que o host espere um tempo limite para saber algo que a página já sabia.

6. **Uma página que não pôde adotar mantém a geração antiga.** Ela está mostrando um texto que o arquivo não possui mais, portanto não deve ser possível salvá-lo sobre a alteração que acabou de ser submetida. Permanecer na geração superada é o que faz com que o host a recuse. *(Alterado por ADR-040: tal página também nunca é lançada. Manter a geração antiga impede que o texto obsoleto chegue ao arquivo, mas por si só deixa o leitor digitando em um editor que descarta tudo silenciosamente.)*

7. **Após o instantâneo, não há tempo seguro para adivinhar.** O antigo tempo limite do lado do cliente liberou o documento quinze segundos após `ExternalWriteReady`, momento em que o host pode estar no meio da gravação de um arquivo temporário, sincronizando-o ou renomeando-o. O leitor estaria então digitando um documento prestes a ser substituído – exatamente a corrida que a barreira existe para remover, reintroduzida pela própria rede de segurança da barreira. Um commit lento pode ser lento; a resposta honesta é dizer isso. O indicador agora aumenta para "Sincronização demorando…" e nada mais acontece.

8. **Não há órfão para resgatar.** O WebView pertence ao mesmo processo que o host. Se o host morrer, a página morrerá com ele, portanto, uma liberação automática nunca poderia salvar uma página de um host desaparecido – só poderia tirar a integridade de um que ainda estivesse funcionando.

9. **Um commit permanece confirmado.** A confirmação é executada inteiramente após o ponto de commit, portanto, ele só pode decidir se a resposta carrega `ui_sync_warning`. Ausente, recusada ou não entregável, a gravação ocorreu, o comando foi bem-sucedido e nada convida a uma nova tentativa que seria anexada duas vezes.

**Resolução na Fase 4.0R.R1.** O limite conhecido foi eliminado: a chave do store agora é calculada a partir do caminho físico canônico resolvido (`canonicalize_store_directory`), unificando links simbólicos, segmentos `.` e `..` e separadores redundantes. Todos os caminhos que apontam para o mesmo armazenamento físico convergem para a mesma autoridade e o mesmo lease de exclusão (consulte ADR-044).

## ADR-040: Uma janela que não conseguiu adotar o documento comitado permanece travada

**Decisão.** Quando ocorre um commit e a página não adota o documento confirmado, o documento **não** é liberado: o editor permanece congelado, as ações do documento na fila permanecem na fila, a geração permanece onde estava, nenhuma confirmação positiva é enviada e a nota diz que está fora de sintonia até ser reaberto. A gravação em si permanece confirmada e ainda é relatada como confirmada com um `ui_sync_warning`.

**Justificativa.**

1. **ADR-039 errou neste, por uma razão plausível.** Ele liberou o editor após uma adoção fracassada, argumentando que o arquivo já estava correto e que uma nota congelada seria inutilizável e impossível de ser fechada. Ambas as metades disso são verdadeiras e a conclusão ainda não se segue. O editor lançado está em uma geração que o host já ultrapassou, então cada salvamento automático que ele envia é recusado corretamente - o leitor digita algo que parece completamente normal e perde tudo, sem nada na tela para dizer isso. Manter a geração antiga protegida do *arquivo*; não fez nada pela pessoa.

2. **Uma inconsistência visível vence uma invisível.** Uma nota que é mantida e diz "A alteração foi gravada, mas esta janela não conseguiu acompanhá-la. Reabra a nota." é uma inconsistência que alguém pode ver e agir sobre ela. Um editor que aceita todas as teclas digitadas e não armazena nenhuma é algo que eles descobrirão mais tarde, ou nunca. Entre uma nota que não aceita entrada e uma nota que a consome, apenas uma delas é recuperável.

3. **A fila segue a mesma regra.** Uma captura retida, uma imagem ou um salvamento de metadados não são descartados e também não são executados. Executá-lo aplicaria uma mutação a um documento que o store já passou, que é a mesma falha ao usar roupas diferentes. Ele permanece retido; reabrir a nota é o que encerra a situação.

4. **`release` estava fazendo mais do que o nome sugeria.** Ele limpa a solicitação ativa, cancela o indicador, descongela e *e* drena a fila. Essa combinação só é segura quando a página contém um documento que vale a pena editar. Chamá-lo incondicionalmente de leitura de contabilidade e foi de fato a decisão. Agora ele é chamado exatamente de dois caminhos: um aborto antes do commit e uma adoção bem-sucedida.

5. **Nada mais tarde pode desbloqueá-lo.** Um `ApplyExternalDocument` repetido, uma anulação ou uma mensagem para uma solicitação diferente deixam uma página com falha exatamente onde está. Não há nenhuma sequência de mensagens que convença a página a editar o texto que o store não possui mais.

6. **Uma nota bloqueada bloqueia outras gravações externas, e isso está correto.** A página não pode produzir um instantâneo confiável, então a barreira nunca responde, o host atinge o tempo limite antes de confirmar qualquer coisa e `noteit` é informado de que o armazenamento está ocupado e nada foi alterado. A recusa é a resposta certa; escrever contra um instantâneo que ninguém pode garantir, não é.

7. **Reabrir é a recuperação, e é suficiente.** O arquivo contém o conteúdo confirmado, portanto, reiniciar o aplicativo — ou um recarregamento futuro e deliberado de uma única nota — traz a janela de volta exatamente para ele, sem duplicação e sem perda de nada. Isso é verificado de ponta a ponta no ambiente isolado, e não presumido.

**Não feito aqui, de propósito.** Sem recarga automática, sem reconciliação, sem mesclagem, sem nova tentativa em segundo plano, sem hash de conteúdo na confirmação. Uma recarga segura por nota é o próximo passo óbvio e é registrada como uma recomendação, e não contrabandeada para uma correção correta.

**Emenda (4.0E.2R): o terminal teve que ser tornado verdadeiro, não apenas declarado.** Duas maneiras de sair do estado do terminal sobreviveram à correção original, ambas encontradas por auditoria e não por um teste com falha.

A primeira: o cronômetro de aviso lento foi deixado armado e seu único guarda perguntou se a solicitação ainda era a ativa – o que, após uma adoção fracassada, é deliberadamente. Então, quatro segundos depois, a página substituiu "esta janela não conseguiu acompanhar, reabra a nota" por "a sincronização está demorando", o que não era apenas cosmético: descreveu uma gravação ainda em andamento quando não havia nenhuma, e apontou para longe da única recuperação que existe. O cronômetro agora é cancelado nesse caminho, por meio de um auxiliar que faz *apenas* isso — buscar `release` para cancelar um cronômetro foi o que causou o bug 4.0E.2 original, porque ele também descongela e drena.

O cancelamento não é a garantia, no entanto. Um callback já pode ser colocado na fila quando seu cronômetro é cancelado, então a própria fase agora é a porta: a página contém um `SyncState`, cada transição pergunta em que estado ela está e um callback tardio encontra um estado no qual não pode atuar. `unsynchronised` não possui nenhuma aresta de saída (outgoing edge) - nem de um temporizador, de uma aplicação repetida, de uma anulação, de uma mensagem para outra solicitação ou de uma geração de `LoadNote`. O mesmo guarda corrige o caso simétrico que ninguém havia relatado: um aviso obsoleto chegando após uma gravação *bem-sucedida*, o que faria com que uma gravação finalizada parecesse lenta.

A segunda: a adoção de um documento suspende brevemente o bloqueio de transação do editor - é a única alteração que o bloqueio existe para permitir a passagem - e a restauração ocorreu após a chamada, e não em um `finally`. Uma adoção que foi interrompida, portanto, deixou o bloqueio desativado, que é exatamente o momento em que todo comando que a página pode executar deve ser recusado. Agora está restaurado em um `finally`.

## ADR-041: A interface da máquina é um segundo renderizador, não um segundo programa

**Decisão.** `noteit --json` emite um documento JSON versionado por execução, renderizado a partir do mesmo resultado tipado que origina as frases em português. O despachante executa uma operação e produz um `Outcome` ou um `CommandError`; `output::render` transforma isso no que uma pessoa lê e `machine::render` transforma isso no documento que um script analisa. Nenhum deles é construído a partir da saída do outro e nenhuma regra de negócio existe duas vezes.

**Justificativa.**

1. **Um consumidor de máquina que precisa ler português não é um contrato.** O objetivo da fase é que um agente pode decidir o que fazer a seguir sem um regex. Portanto, cada decisão que um chamador precisa tomar - funcionou, alguma coisa mudou, o commit aconteceu, a janela está na etapa, o que deu errado - é um campo digitado com um token estável em snake_case em inglês, e cada frase humana é explicitamente documentada como diagnóstico. `message` é para a pessoa que lê o registro.

2. **Serializar o texto renderizado teria sido um erro fácil.** `{"result": "Nota criada com sucesso"}` é JSON e não é uma interface: é o mesmo problema de analisar uma string como se fosse um tipo de conteúdo. A camada de resultado tipado existe para que o renderizador da máquina nunca veja uma frase renderizada e para que o renderizador humano não possa ser reaproveitado silenciosamente como fonte de dados.

3. **Dois renderizadores, uma operação.** `WriteOperation`, `NoteMutation`, `WriteOutcome`, `WriteError` e `authority::perform` permanecem intactos. Não há `json_append`. O armazenamento não pode se comportar de maneira diferente dependendo de qual adaptador solicitado, e os testes comprovam isso comparando o arquivo de notas resultante, byte por byte, entre os dois modos para uma gravação que não move nenhum registro de data e hora.

4. **Os canais precisavam se tornar dados antes que pudessem ser garantidos.** O antigo despachante imprimia avisos de leitura com `eprint!` no meio de um comando, que é invisível para um teste de nível de função e fatal para uma interface de máquina — um `--json listar` bem-sucedido teria escrito uma frase para o erro padrão. `run_with_args` agora retorna um `CliResponse` carregando o código de saída e ambos os canais, então "o sucesso não grava nada no stderr" é algo que um teste pode afirmar, em vez de algo que um revisor deve observar. Esse refatorador é a única alteração da fase na infraestrutura interna (plumbing) do caminho humano e sua saída permanece inalterada.

5. **`ui_sync_warning` e `Indeterminate` são a razão pela qual a fase existe.** Ambos são estados pós-confirmação em que um adaptador ingênuo se transforma em "falhou" e ambos seriam tentados novamente, e um acréscimo repetido duplicaria um parágrafo. Portanto, ambos são de primeira classe: uma gravação confirmada cuja janela não foi confirmada é `status: warning`, `commit_state: committed`, saída `0`, com um `ui_sync: {status, code}` estruturado; um resultado desconhecido é `status: indeterminate`, `commit_state: unknown` e nunca `not_committed`. Os quatro casos que um chamador deve distinguir — `ok/committed`, `warning/committed`, `error/not_committed`, `indeterminate/unknown` — são distintos sem a leitura de um único caractere de prosa.

6. **`commit_state` é a única fonte de verdade sobre repetição.** Um booleano `retry_safe` foi considerado e rejeitado: dois campos que respondem à mesma pergunta divergem, e a resposta honesta para `not_committed` é "depende de `error.code`", o que um booleano não pode dizer. A tabela documentada mapeia o status e o estado de confirmação para saber se uma repetição automática é permitida, e nada mais afirma isso.

7. **O modo de máquina sobrevive a uma falha de análise.** O Clap ao recusar uma lista de argumentos é exatamente quando um script mais precisa de uma resposta legível por máquina e também é quando o sinalizador analisado ainda não existe. Portanto, o modo é decidido a partir da opção analisada quando a análise foi bem-sucedida e a partir de uma pequena varredura exata dos argumentos brutos quando isso não aconteceu - token inteiro `--json`, nunca uma substring, nunca após o escape `--`, nunca entrada padrão. Um teste afirma que os dois concordam em cada lista de argumentos analisada, de modo que o substituto não pode se desviar da regra real.

8. **JSON são dados e o sanitizador de terminal não é aplicado a eles.** `sanitize_for_terminal` existe para impedir que o conteúdo de uma nota acione um terminal; um documento que ninguém renderiza como texto não tem esse problema, e o escape de JSON já neutraliza todos os caracteres de controle que ele poderia carregar. Destruir o corpo para proteger um terminal que não existe entregaria um texto de script que a nota não contém. O renderizador humano ainda é higienizado e ambos são testados na mesma nota.

9. **O contrato público não é o protocolo privado.** `ControlRequest`, `ControlResponse`, identificadores de solicitação, a versão do protocolo, o soquete, o lease, a geração de janela e `WritePath` derivam de `Serialize` e nenhum deles é exportado aqui. A conversa CLI para desktop e a conversa CLI para consumidor são limites diferentes que compartilham uma codificação, e um teste verifica cada documento em busca do vocabulário do primeiro.

**Consequências.** O esquema público reside em `noteit-cli/src/machine.rs` como DTOs explícitos em vez de como os próprios tipos do Core, portanto, uma renomeação em Core é um erro de compilação em vez de uma alteração silenciosa do esquema. Cada token – nomes de comandos, tipos de resultados, códigos de erro, códigos de aviso, estados de tarefas – é escrito em um `match` pelo mesmo motivo. `docs/machine-interface.md` é o contrato; são os testes da fase que a mantêm verdadeira.

**Dívida deixada em pé, deliberadamente.** A canonização da identidade do store (`/path/data` versus `/path/./data`) permanece por 4,0R. A confirmação ainda não contém hash de conteúdo. A relação entre o tempo limite de autoridade e os tempos da própria página ainda está acoplada e ainda não documentada como um único número. A recarga por nota após uma adoção fracassada ainda é a próxima etapa recomendada e ainda não foi implementada. Nenhum deles foi tocado para deixar esta fase mais organizada.

## ADR-042: A apresentação é uma camada da CLI, e as capacidades do terminal são um valor

**Decisão.** `noteit` sem argumentos passou a mostrar uma apresentação — logotipo `NOTE-IT` em blocos, versão, uma linha sobre o que o Note-it é, cinco comandos por onde começar — e continua encerrando imediatamente com código `0`. A apresentação vive em `noteit-cli/src/welcome.rs`, atrás do renderizador humano, e é uma função pura de duas coisas: o que o canal aceita e quanta largura existe. Ambas passaram a ser um valor, `OutputContext`, agora decidido **por canal** e não mais uma vez a partir da saída padrão.

**Justificativa.**

1. **A tela inicial é apresentação, não interatividade.** A fase pedia uma CLI agradável para uma pessoa, e a leitura fácil dessa frase seria uma TUI: painéis, navegação, um prompt esperando comandos. Nada disso foi feito, e o motivo é que nada disso é compatível com o que a CLI já é. `noteit` é um processo que responde e sai — é assim que scripts, pipes e agentes o usam, e é essa propriedade que a Fase 4.0F selou. Uma tela que espera entrada quebraria `noteit` dentro de um `Makefile`. A TUI foi registrada como Fase 5.0 e nenhuma dependência foi adicionada na direção dela.

2. **Capacidades como valor, não como `is_terminal()` espalhado.** A alternativa óbvia — perguntar ao sistema em cada ponto de decisão — torna a matriz inteira dependente de o teste ter um terminal físico, o que a integração contínua não tem. Com `OutputContext` carregando "aceita cor", "largura conhecida" e "desenha blocos", cada renderizador vira função pura e os oito estados da matriz são alcançáveis em teste sem terminal nenhum. Os testes que precisam mesmo de um terminal — porque a pergunta é justamente `isatty` — abrem um pseudoterminal de tamanho declarado e leem de volta o que o binário real escreveu nele.

3. **Cada canal responde por si.** Antes desta fase havia um único contexto, derivado da saída padrão, e ele estilizava também a saída de erro. `noteit comando-inexistente 2> erros.txt`, rodado de um terminal, gravava sequências ANSI dentro do arquivo — um erro que ninguém consegue filtrar depois. É um defeito anterior à fase, mas é exatamente o defeito que a política de ANSI desta fase proíbe, então foi corrigido aqui, com teste de regressão nos dois sentidos: terminal na saída padrão com erro em arquivo, e o contrário.

4. **Largura vem do terminal, e `COLUMNS` é só reserva.** `COLUMNS` é a resposta sem dependência nenhuma, e é a resposta errada: os shells não a exportam para processos filhos, então confiar nela sozinha significaria que os formatos estreitos nunca apareceriam de verdade. `TIOCGWINSZ` pergunta à janela, que é o que a pergunta realmente é. Isso custou uma dependência — `libc`, para uma chamada — e ela já estava no grafo do workspace por `dirs-sys` e `getrandom`: é uma aresta nova, não um crate novo, e nada dela alcança o `noteit-core`. Valores implausíveis (zero colunas, um `COLUMNS` de cinco dígitos) são recusados dos dois lados, e sem resposta nenhuma a suposição conservadora é 80 colunas — larga o bastante para a tela inteira.

5. **Um cano não tem largura.** Nada é medido quando a saída padrão não é um terminal: um `COLUMNS` herdado do shell que iniciou o pipeline descreve uma janela para onde a saída não está indo. O cano recebe a apresentação completa em texto puro, idêntica a cada execução — determinismo é o que um redirecionamento precisa, e o logotipo em UTF-8 atravessa um arquivo sem problema.

6. **Cor e glifo são perguntas diferentes.** `NO_COLOR` e `TERM=dumb` desligam a cor; só `TERM=dumb` também dispensa a arte em blocos, porque um terminal que se declarou sem recursos não é um lugar para mandar seis linhas de caracteres de desenho. Um cano continua recebendo o logotipo. Manter os dois eixos separados é o que permite testar cada um sozinho.

7. **Cor nunca carrega informação.** Amarelo para a marca, magenta para o acento, e nada mais — nenhuma terceira voz. Um teste percorre a tela estilizada, remove os escapes e compara com a tela pura: se alguma informação dependesse da cor, a comparação falharia. Cores ANSI básicas, sem true color exigido.

8. **A versão tem uma fonte só.** A apresentação lê `CARGO_PKG_VERSION`, a mesma que `noteit versao` e `noteit status` leem. Um teste roda os dois no binário real e compara, para que não exista uma constante que se atrase em relação ao pacote.

9. **A apresentação não é um comando.** Executar `noteit` não cria nota, janela, socket, lock ou store, não depende de haver notas e não falha se o store ainda não existe. É afirmado como fingerprint: cada caminho sob um XDG isolado, com modo, dono, inode, tamanho, bytes, `mtime` e `ctime`, antes e depois das cinco variantes da tela.

10. **O logotipo aparece uma vez.** `noteit ajuda`, os erros, cada comando e o `--json` seguem sem ele. Uma ajuda é referência, e referência não abre com anúncio.

**Consequências.** `run_with_args` passou a receber `Channels` em vez de um `OutputContext`; é a superfície interna da crate, e o binário e os testes foram acompanhados. A interface de máquina não mudou em nada: mesmo documento, mesmos canais, mesmos códigos — agora também provado sobre um terminal real e sob janelas de todos os tamanhos, justamente porque a camada de apresentação passou a existir e precisava ser provada incapaz de alcançá-la.

## ADR-043: Os gates vivem no repositório e o CI os consome

**Decisão.** `scripts/check` é a autoridade sobre o que precisa passar. O workflow do GitHub Actions não reimplementa os comandos de qualidade: cada step invoca um estágio de `scripts/check`, e o mesmo estágio é o que uma pessoa roda localmente. `scripts/doctor` diagnostica o ambiente sem alterá-lo e `scripts/build.sh` faz a build reprodutível. Nenhum código de runtime do Note-it foi tocado para isso.

**Justificativa.**

1. **Havia quatro listas e elas já divergiam.** O CI rodava sete comandos; `docs/development.md` documentava oito, incluindo `cargo check --workspace`, que o CI **não** executava; o `CONTRIBUTING.md` trazia uma quinta lista mais fraca que todas as outras — `cargo fmt --check` em vez de `cargo fmt --all -- --check`, `cargo clippy -- -D warnings` sem `--workspace --all-targets --all-features`, `cargo test` sem `--workspace`, e nenhum dos dois boundary scripts. Um colaborador que seguisse o CONTRIBUTING passaria em tudo localmente e quebraria no CI. Não é um problema de disciplina: é o resultado previsível de manter a mesma lista em quatro lugares.

2. **O consumidor certo do gate é o CI, não o contrário.** A alternativa seria gerar o script a partir do workflow, ou aceitar a duplicação e adicionar um teste que compara as duas listas. Ambas mantêm duas fontes; a segunda ainda deixa a lista fraca do CONTRIBUTING de fora. Colocar os comandos num script versionado e fazer o workflow chamá-lo resolve as duas coisas de uma vez, e tem o efeito colateral de que reproduzir uma falha do CI localmente passa a ser o mesmo comando que falhou lá.

3. **Um step por gate continua sendo o certo.** Trocar nove steps por um `scripts/check all` gigante economizaria linhas de YAML e custaria a informação mais útil de um run vermelho: qual gate quebrou. Os estágios são atômicos justamente para o workflow preservar essa granularidade enquanto chama uma implementação só.

4. **Nada foi removido, e um gate foi ganho.** `cargo check --workspace` já estava documentado como gate local e faltava no CI; agora está lá. As duas suítes headless permanecem apesar de `cargo test --workspace` repetir seus testes: elas provam outra coisa — que o Core e a CLI funcionam sem display, sem compositor e sem barramento. Um teste que passa dentro da sessão ambiente não diz nada sobre isso.

5. **`doctor` verifica e nunca conserta.** A tentação óbvia é fazer o diagnóstico instalar o que falta. Um script do projeto que chama `pacman`, `apt` ou `brew` decide por quem opera a máquina, precisa de privilégio que não deveria pedir, e no CI colide com a instalação do runner — que continua sendo do workflow. Verificar e dizer o que falta é a fronteira inteira.

6. **`doctor` não para no primeiro problema; `check` para.** São perguntas diferentes. Um diagnóstico que aborta faz a pessoa instalar uma coisa, rodar de novo e instalar outra; ele roda tudo e o resumo é o veredito. Um gate que continua depois de falhar está mentindo sobre o estado do repositório; ele para e propaga o código do estágio que quebrou.

7. **A toolchain mínima é a que o `Cargo.toml` já declara.** `doctor` lê `rust-version` do manifesto em vez de repetir o número. Uma política de versão escrita em dois lugares é a mesma classe de bug que esta fase existe para eliminar. Para `node` e `pnpm` o projeto não declara mínimo nenhum, então ausência é erro e estar atrás do que o CI usa é aviso — não se inventa aqui uma incompatibilidade que ninguém demonstrou.

8. **pnpm e só pnpm.** O `build.sh` anterior caía para `npm install` quando `pnpm` não existia, e instalava sem `--frozen-lockfile`. As duas coisas produzem uma árvore de dependências que o lockfile não descreve e que o CI nunca viu — exatamente o oposto de uma build reprodutível. Falta de pnpm passou a ser erro.

9. **O harness de isolamento não roda duas vezes.** `tests/isolation.rs` já executa `scripts/test-isolation` de dentro do `cargo test`, então `workspace-tests` o cobre. Numa sessão gráfica ele abre brevemente uma janela real do Note-it, apontada o tempo todo para um store descartável em um barramento próprio; isso é comportamento conhecido do projeto e está documentado onde alguém vai encontrá-lo.

**Consequências.** Adicionar um gate agora é editar `scripts/check` e acrescentar um step que o chama; ele passa a valer local e remotamente na mesma alteração. `CONTRIBUTING.md` e `docs/development.md` apontam para os entrypoints em vez de repetir comandos. Os três scripts resolvem a raiz do repositório a partir do próprio caminho, então funcionam de qualquer diretório. Nenhuma dependência nova, nenhum task runner, nenhum arquivo de runtime alterado: a fase inteira cabe em `scripts/`, no workflow e na documentação.

## ADR-044: Canonicidade Física da Identidade do Store e Integridade Estrita da Nota

**Decisão.**
1. **Identidade Canônica do Store na Coordenação:** A chave de coordenação de escrita (`store_key`) é derivada estritamente do caminho físico canônico do diretório de notas (`canonicalize_store_directory`). Qualquer representação alternativa (links simbólicos, segmentos `.`, traversais `..`, barras redundantes) colapsa para a mesma autoridade e compartilha o mesmo lease. Resoluções com ciclos de links simbólicos ou acessos inválidos falham estritamente (fail-closed) com recusa explícita, sem qualquer fallback para strings brutas.
2. **Ancoragem Determinística da Identidade da Nota:** O identificador canônico de uma nota persistida é ancorado ao UUID de seu nome de arquivo (`<uuid>.md`). Uma nota sem front matter YAML mantém determinismo estrito (`NoteDocument::parse_with_id`) e não gera novo UUID em tempo de leitura ou mutação.
3. **Recusa Estrita por Divergência de Identidade:** Qualquer divergência entre o UUID do nome de arquivo e o campo `id` do front matter YAML é tratada como conflito de integridade e falha de modo seguro (fail-closed), recusando leitura e escrita sem alterar o arquivo existente e sem criar novos arquivos.
4. **Defesa em Profundidade nas Camadas de Storage e Write:** As operações de persistência (`save_note_atomic_with_id`, `commit_addressed`) verificam explicitamente se o ID endereçado coincide com os metadados do documento antes de qualquer I/O atômico, garantindo que respostas de máquina e mutações em disco sejam honestas e fiéis à nota solicitada.

**Justificativa.**
1. **Eliminação do Split-Brain por Aliases (Finding R-001):** Anteriormente, o cálculo do hash FNV-1a sobre caminhos textuais brutos gerava chaves de coordenação distintas para `/store/notes` e `/store/./notes` ou links simbólicos, permitindo múltiplos gravadores concorrentes no mesmo diretório físico. A canonicidade física garante que exatamente um processo detenha autoridade de escrita por armazenamento físico.
2. **Eliminação de Arquivos Fantasmas e Perda Silenciosa de Mutação (Finding R-002/R-004):** Notas sem front matter recebiam UUIDs voláteis em `NoteDocument::parse`, e o armazenamento salvava no caminho derivado do metadata em vez do arquivo endereçado. Mutações repetidas geravam múltiplos arquivos fantasmas com UUIDs aleatórios enquanto a nota endereçada permanecia intacta.
3. **Impedimento de Redirecionamento Silencioso de Escrita:** Se um arquivo `A.md` contivesse `id: B`, uma gravação poderia sobrescrever ou criar silenciosamente `B.md`. A ancoragem e a validação bidirecional impedem qualquer corrupção ou confusão de identidade.
4. **Alinhamento com MCP:** A interface de agentes exige previsibilidade matemática em relação aos alvos de mutação e respostas de máquina (`note_id` retornado deve ser estritamente o `id` modificado no disco).

## ADR-045: O MCP é mais uma entrada tipada para o domínio, e um agente nunca grava sem a revisão que leu

**Contexto.** A Fase 4.0R fechou o que faltava para que um programa — e não uma
pessoa — pudesse ser um escritor de primeira classe do store: uma identidade
física por store (ADR-044), um único gravador por identidade (ADR-038), a
recusa de uma gravação construída sobre uma nota que já mudou (R-016) e um
protocolo privado versionado que se recusa a atender um par que discorda sobre o
significado de uma precondição (`PROTOCOL_VERSION = 2`). A Fase 4.1 é o primeiro
consumidor programático dessas garantias.

**Decisão.**

1. **Um binário separado, `noteit-mcp`, em um crate próprio.** Não um subcomando
   da CLI, não um modo do desktop.
2. **stdio, e somente stdio.** O host inicia o processo e é dono do seu tempo de
   vida. Nenhuma porta, nenhum listener, nenhum HTTP, nenhum SSE, nenhum daemon,
   nenhuma configuração persistente escrita em lugar nenhum.
3. **O SDK oficial em Rust, `rmcp`, sem features padrão.** Apenas `server`,
   `macros` e `transport-io`. Nada de JSON-RPC, framing ou negociação de versão
   escritos aqui.
4. **Somente tools.** Nem Resources, nem Prompts, nem sampling, nem elicitation,
   nem a extensão MCP Tasks.
5. **`expected_revision` é obrigatório em toda mutação de nota existente.**
   Obrigatório no schema, e `NoteRevision` — não `Option<NoteRevision>` — no
   único tipo deste crate capaz de construir uma mutação.
6. **A autoridade de escrita mudou de crate.** `authority.rs` saiu de
   `noteit-cli` e entrou em `noteit-core`; a CLI a reexporta sob o mesmo nome.
7. **Nenhuma gravação direta e nenhum subprocesso.** O crate não abre um `.md`,
   não executa `noteit` e não interpreta a saída JSON da CLI.

**Justificativa.**

1. **Por que um binário separado.** Um host MCP faz `spawn` de um processo e
   conversa por um cano. Enfiar isso na CLI significaria que `noteit` teria um
   modo em que sua saída padrão deixa de ser para pessoas, e a única coisa que
   separaria um banner de um fluxo JSON-RPC corrompido seria uma flag. Um
   binário próprio torna “stdout pertence ao protocolo” uma propriedade do
   arquivo inteiro, verificável por um gate.

2. **Por que stdio e nada além.** O store é um recurso local. Uma porta aberta
   é uma superfície que ninguém pediu, um problema de autenticação que ninguém
   tem, e um caminho para o store que não passa pelo lease. `transport-io` é a
   única feature de transporte ligada, e `scripts/check-mcp-boundary` falha se
   uma pilha HTTP, TLS, OAuth, SSE ou WebSocket aparecer na árvore.

3. **Por que o SDK oficial.** Um protocolo implementado à mão é um segundo
   conjunto de bugs e uma segunda opinião sobre negociação de versão. O que se
   ganha ao escrever JSON-RPC de novo é zero; o que se perde é a compatibilidade
   com hosts que este repositório nunca vai testar. A versão do MCP é decidida
   pelo `rmcp` e por mais ninguém — inclusive porque a revisão `2026-07-28`
   substituiu o handshake por metadados por requisição, e essa é exatamente a
   classe de detalhe que não deve ser reimplementada aqui.

4. **Por que só tools.** Um Resource é conteúdo que o host pode buscar sem uma
   decisão do modelo, e um Prompt é texto que orienta o modelo. Nenhum dos dois
   tem uma pergunta respondida nesta fase, e publicar uma superfície que não foi
   pensada é publicar uma superfície que não foi auditada. `noteit_tasks_list` é
   uma tool comum: as tarefas Markdown do Note-it não têm relação nenhuma com a
   extensão MCP Tasks, e confundir as duas seria dar ao mesmo nome dois
   significados.

5. **Por que a precondição é obrigatória aqui e opcional na CLI.** Essa é a
   decisão central da fase. `noteit editar <id>` sem `--if-revision` é *last
   writer wins*, e está certo: a pessoa que digitou o comando está olhando para a
   nota, e exigir um token dela seria cerimônia. Um agente não está olhando para
   nada — ele leu a nota em algum momento, decidiu, e vai gravar. Se a nota mudou
   nesse intervalo, uma gravação incondicional apaga a mudança e **nada falha**.

   `Option<NoteRevision>` na fronteira MCP seria exatamente essa porta: campo
   ausente → `None` → gravação incondicional, três passos e nenhum erro. Por isso
   o tipo não é opcional em lugar nenhum do caminho: o schema publicado marca o
   campo como obrigatório, a desserialização do SDK recusa a requisição antes de
   qualquer código deste repositório rodar, e `ExistingNoteMutation` — o único
   tipo do crate que produz um `WriteOperation::MutateNote` — guarda um
   `NoteRevision` já parseado, sem construtor que o omita. Uma revisão malformada
   é `invalid_input` e nunca “sem precondição”, porque um token corrompido que
   virasse `None` seria a mesma gravação incondicional por outro caminho.

   O corolário está nas descrições das tools e nas instruções do servidor: um
   `revision_conflict` exige releitura, não uma nova tentativa — nem mesmo com a
   `current_revision` que o erro devolveu, que nomeia um conteúdo que o cliente
   não olhou. Por isso a resposta de conflito não traz `revision` nem o novo
   corpo: encadear a partir dela seria a sobrescrita silenciosa com um passo
   extra.

6. **Por que a autoridade mudou de crate.** `noteit_cli::authority::perform` já
   era a regra certa e não tinha nada de CLI: só usa `control`, `coordination`,
   `storage`, `write` e o `NoteItCore`, todos do Core. Deixá-la onde estava
   daria duas opções ruins — o servidor MCP dependeria do crate da linha de
   comando e linkaria `clap`, o `ioctl` de largura de terminal e a camada de
   estilo ANSI; ou o crate MCP teria a sua própria cópia da máquina de estados.
   A segunda é impensável: duas cópias de “quem pode gravar agora” acabam sendo
   duas respostas, e o lease só funciona porque há uma. Mover é a menor abstração
   compartilhada possível, e a CLI reexporta o módulo para que nada mude do lado
   dela.

7. **Por que nenhuma gravação direta e nenhum subprocesso.** As duas alternativas
   óbvias são as duas piores. Abrir o `.md` contorna o lease, a identidade da
   nota, a precondição, o backup e a gravação atômica de uma vez só. Executar
   `noteit --json` e interpretar a saída troca uma cadeia tipada por um parser de
   texto, transforma o `SCHEMA_VERSION` público em uma dependência interna, e faz
   de todo argumento uma questão de escape de linha de comando. A cadeia continua
   tipada de ponta a ponta, e o gate mecânico recusa qualquer uma das duas.

8. **Por que não existe uma tool genérica.** `read_file`, `write_file`,
   `list_directory`, `shell`: qualquer uma delas destruiria o limite inteiro. As
   garantias deste repositório valem porque o único jeito de alterar uma nota é
   uma operação de domínio; uma tool que aceita um caminho é um caminho para o
   store que não passa por nenhuma delas. `noteit-mcp` é um servidor do domínio
   Note-it, e não um servidor de filesystem com um nome bonito.

**Consequências.** O workspace tem um terceiro binário, e `scripts/build.sh`,
`scripts/check` e o CI conferem os três. Existe um gate novo, `mcp-boundary`,
listado nos mesmos lugares que os outros dois — `scripts/check`, o workflow,
`CONTRIBUTING.md` e `docs/development.md` — para que as listas não voltem a
divergir. `docs/mcp.md` documenta o contrato do agente. `SCHEMA_VERSION` do
`noteit --json` **não** mudou: são contratos independentes, e o MCP ter passado a
existir não é um fato sobre o documento que o `--json` publica.

## ADR-046: Um gate só vale o que ele reprova, e três alegações da Fase 4.1 não eram reprováveis

**Contexto.** A auditoria independente da Fase 4.1 aceitou o comportamento do
`noteit-mcp` e encontrou outra coisa: três lugares onde a documentação e os
comentários prometiam uma garantia mecânica que o mecanismo não entregava. Não
eram bugs — o servidor fazia a coisa certa — mas eram *proteções que não
protegiam*, e uma proteção que não protege é pior que nenhuma, porque a próxima
pessoa a mexer no código vai acreditar nela.

As três foram reproduzidas antes de qualquer correção:

1. **`std::net` passava.** O boundary recusava crates de rede pelo nome —
   `reqwest`, `hyper`, `axum`, TLS, OAuth, WebSocket. Um
   `std::net::TcpListener::bind("127.0.0.1:9999")` dentro de um handler de tool
   compilou e o gate respondeu `MCP boundary OK`. A biblioteca padrão não
   aparece em `cargo tree`, e a afirmação "o gate impede que o `noteit-mcp`
   ganhe rede" era, literalmente, sobre dependências e não sobre rede.

2. **A matriz de mutações tinha duas fontes de verdade.** O `match` exaustivo de
   `tool_for` de fato não compilava sem uma decisão sobre cada variante nova,
   mas a lista iterada, `every_mutation()`, era escrita à mão. Uma variante
   acrescentada ao Core, remendada no `match` para compilar e esquecida na
   lista, teria sido *decidida e nunca exercitada* — e a asserção que guardava a
   lista continuaria lendo nove.

3. **`outcome_is_known` não era exaustivo.** Escrita com `matches!` e marcada
   `#[allow(dead_code)]`, ela prometia que "adicionar um outcome kind ao Core
   faz alguém olhar para esta fronteira". `matches!` é uma expressão que
   responde `false` para o padrão que não lista: a variante nova compilava, a
   função — que ninguém chamava — respondia `false`, e ninguém olhava para nada.

**Decisão.**

1. **"Sem rede" passa a ser verificado em quatro camadas**, porque nenhuma
   basta sozinha: o grafo de dependências (nenhum crate de rede ou de socket);
   as *features resolvidas* (`tokio` sem `net`, de modo que `tokio::net` não
   exista neste build); o código do crate (nenhum `std::net`, nenhum tipo de
   socket, de nenhuma família, Unix inclusive); e o **processo em execução**,
   por `/proc/<pid>/fd`.
2. **A matriz de `NoteMutation` passa a ser declarada uma vez**, por uma macro
   declarativa que gera a lista iterada e o `match` a partir das mesmas linhas.
3. **`outcome_is_known` sai do crate** e é substituída por um `match` exaustivo
   sem braço curinga em `noteit-mcp/tests/mcp_contract_decisions.rs`.

**Justificativa.**

1. **Por que quatro camadas e não uma regra melhor.** Não existe uma. Cada
   camada fecha exatamente o que as outras não alcançam. O grafo não vê a
   `std`. As features fecham a rota assíncrona de vez — `tokio::net` deixa de
   *existir*, então usá-la é erro de compilação e não algo que uma regra
   precisa reconhecer pelo nome —, mas unificação de features é do grafo
   inteiro e uma dependência qualquer pode ligá-la sem este manifesto pedir,
   por isso ela é *asserida* e não presumida. A regra textual pega `std::net`,
   e é textual porque não há alternativa: Rust estável não tem lint
   personalizado, e um `#![forbid]` não alcança um caminho da biblioteca
   padrão.

   A quarta camada existe porque as três primeiras compartilham um ponto cego:
   **descrevem o programa que foi escrito, não o que roda.** Perguntar ao
   núcleo o que o processo tem aberto é a única das quatro que não pode ser
   enganada por uma grafia. Um servidor que fala em entrada e saída padrão e
   chama o `noteit-core` tem três descritores e nenhum socket; foi isso que se
   mediu, e é isso que o teste exige.

   Unix sockets são recusados no código deste crate junto com os de rede, e
   isso não é excesso de zelo: o socket de controle privado existe, é
   necessário, e é do `noteit-core`. Um segundo lugar que fala com o store por
   socket seria uma segunda implementação do handover — a mesma razão pela qual
   a autoridade mudou de crate no ADR-045.

2. **Por que uma macro, tendo dito que macros complexas devem ser evitadas.**
   Porque o problema era literalmente ter escrito a mesma informação duas
   vezes, e a única correção que fecha isso é derivá-la de uma declaração só. A
   macro tem uma regra e produz três itens; não há recursão, não há
   `tt`-munching e não há geração condicional. O que ela compra é preciso: uma
   variante nova continua sendo erro de compilação no `match`, e agora *a linha
   que resolve o erro de compilação é a mesma linha que produz o valor
   iterado*. Não há mais um jeito de satisfazer o compilador e ainda assim
   pular a variante.

   Cada linha também confere, em tempo de execução, que o valor que carrega é
   mesmo da variante que nomeia. Um copiar-e-colar que testasse uma variante
   duas vezes e outra nenhuma satisfaria tanto o `match` quanto a contagem, e
   passaria despercebido — que é exatamente a classe de erro que esta ADR
   existe para fechar, aplicada a si mesma.

   A asserção de contagem foi mantida, com a descrição corrigida: ela **não** é
   a prova de exaustividade, é a verificação do sentido contrário — que nenhuma
   variante ganhou duas linhas, e que o número ainda é o nove que a
   documentação descreve.

3. **Por que a guarda de `WriteOutcomeKind` virou um teste.** Um `match`
   exaustivo é a construção certa: ele não compila com uma variante faltando,
   diga-se isso de um `matches!` ou não. O que mudou junto foi o lugar. A
   função vivia no crate sem ninguém chamá-la, existindo só para ser compilada
   — e código que existe só para ser compilado é código que a próxima pessoa
   apaga por parecer morto. Num teste ela é compilada por `cargo test`, que é
   estágio do `scripts/check` e step do CI, então o erro de compilação chega no
   mesmo instante em que chegaria; e ganha um nome que diz o que está sendo
   protegido: a decisão de **não** publicar `kind` na saída MCP, porque o
   agente sabe qual tool chamou.

   Essa decisão também passou a ser verificada no fio, e não só no tipo: o
   teste lê uma resposta real e o `outputSchema` publicado, e exige que `kind`
   não esteja em nenhum dos dois. Uma decisão presa que o código tivesse
   deixado de honrar em silêncio seria pior que nenhuma.

**Consequências.** O `noteit-mcp` não mudou de comportamento: nenhuma tool foi
acrescentada, removida ou alterada, nenhum schema publicado mudou, e a garantia
central de `expected_revision` está exatamente onde estava. O que mudou é o que
o repositório consegue *reprovar*. Ficam duas suítes novas —
`mcp_no_network.rs` e `mcp_contract_decisions.rs` —, três regras novas no
boundary, e uma correção de redação em `contract.rs`, que dizia que uma
renomeação no Core seria erro de compilação "neste arquivo" quando aquele
arquivo não importa nada do Core; o erro aparece em `domain.rs`, que é onde a
tradução mora.

`mcp_no_network.rs` depende de `/proc` e portanto de Linux. Isso é aceitável e
está declarado: o Note-it é uma aplicação Wayland com dependência de
layer-shell, o CI roda em Arch Linux, e procfs é como esta pergunta se responde
na plataforma que o projeto tem. Num sistema sem `/proc` a suíte falha dizendo
que não pôde olhar, em vez de passar por não ter olhado — uma verificação que
passa em silêncio quando não conseguiu verificar é a mesma classe de erro que
esta ADR fecha.

## ADR-047: A fronteira de rede é a família do endereço, e a prova dinâmica diz o tamanho que tem

**Contexto.** A ADR-046 fechou a lacuna de que `std::net` passava pelo boundary,
e criou uma suíte que observa o processo em execução. Uma auditoria
independente da 4.1R1 encontrou o resíduo: aquela suíte tirava uma fotografia
dos descritores **antes** e **depois** de cada chamada MCP, e o comentário dizia
que nenhum socket de Internet existia "em nenhum momento de uma gravação". Um
socket aberto e fechado *dentro* do handler é invisível às duas fotografias.

Reproduzido antes de escrever a correção: um `TcpListener` vinculado por 250 ms
dentro de um handler de tool, e os três testes passaram.

**Decisão.**

1. **A observação dinâmica passa a ser contínua.** Uma thread monitora amostra
   `/proc/<pid>/fd` durante toda a operação, e classifica cada socket que
   encontra no instante em que o vê.
2. **O que ela prova está escrito com precisão, separando o sólido do melhor
   esforço**, e a garantia de família é transferida para as camadas estáticas.
3. **O boundary passa a cobrir o `noteit-core`**, com uma regra que distingue
   AF_INET/AF_INET6 (proibidos) de AF_UNIX (permitido), e que exige que o
   mecanismo Unix continue existindo.
4. **A sensibilidade do instrumento é provada por um controle positivo**, e não
   afirmada.

**Justificativa.**

1. **Por que contínua em vez de duas fotografias.** Porque a diferença entre as
   duas é exatamente o achado. Medido: 14 µs de intervalo médio amostrando só
   os descritores, 76 µs na execução em que um socket aparece e precisa ser
   classificado. Com o `TcpListener` de 250 ms reinjetado, a prova nova o
   detecta e o identifica positivamente como Internet; a antiga passava.

2. **Por que a documentação diz "amostragem" em vez de "sempre".** Duas coisas
   foram *medidas* durante esta fase e mudaram o desenho:

   - um socket criado com `socket(AF_INET, …)` e nunca vinculado **não** aparece
     em `/proc/net/tcp`. A tabela do núcleo não é o conjunto de todos os sockets
     de Internet; é o conjunto dos que têm endereço;
   - um socket que fecha entre a leitura do descritor e a leitura da tabela já
     saiu dela. O laço de retry do caminho fail-closed produz dezenas desses,
     todos legítimos e todos AF_UNIX, e a primeira versão do classificador os
     reprovou como "não classificados".

   A primeira medição derruba a completude do detector; a segunda derruba a
   ideia de classificar depois do fato. O classificador resultante **não tem
   falsos positivos** e pode ter falsos negativos — o que é exatamente o
   contrário do que se precisa de uma *garantia*, e exatamente o que se quer de
   um *detector adicional*. Então ele é descrito como detector, e a garantia de
   família fica com quem consegue sustentá-la: a regra estática.

   Inventar uma prova mais forte aqui exigiria interceptação de syscalls,
   `pidfd_getfd` com `getsockopt(SO_DOMAIN)`, eBPF ou ptrace — máquinas caras,
   privilegiadas ou frágeis, todas desproporcionais, e todas produzindo mais
   confiança do que evidência.

3. **Por que a regra do Core é sobre família e não sobre a palavra "socket".** O
   `noteit-mcp` é uma casca fina: quase tudo o que ele faz acontece dentro do
   `noteit-core`. Uma regra que parasse no `noteit-mcp/src` deixaria "este
   servidor não tem rede" apoiado em um crate de um caminho de dois.

   Mas o Core **usa** socket de propósito. É assim que uma gravação chega à
   instância que segura o store, é AF_UNIX, é local, e é a ADR-045 inteira. Um
   grep que proibisse "socket" no Core exigiria apagar a arquitetura para passar
   no teste — o pior tipo de gate, o que se satisfaz destruindo o que deveria
   proteger.

   Por isso a regra nomeia `std::net`, `TcpListener`, `TcpStream`, `UdpSocket`,
   `ToSocketAddrs`, `SocketAddrV4/V6` e os tipos de endereço IP, e deixa
   `std::os::unix::net` em paz. E vem acompanhada de uma **asserção positiva**
   de que `noteit-core/src/authority.rs` ainda conecta por `UnixStream`: sem
   ela, as duas regras de proibição seriam satisfeitas perfeitamente por um Core
   que tivesse perdido o handover.

4. **Por que um controle positivo.** Um observador que não vê nada é
   indistinguível de um observador que não está olhando, e um teste que só
   afirma ausências é um teste que passa quando quebra. A gravação pela
   autoridade resolve isso de graça: o Core abre um socket, entrega a mudança e
   o fecha dentro da mesma chamada — que é precisamente a forma que a prova
   anterior não enxergava. Exigir que o monitor o veja transforma "não vimos
   socket de Internet" de ausência de evidência em medição por um instrumento
   demonstravelmente funcionando.

   Pelo mesmo motivo o guard de densidade é sobre o **intervalo médio entre
   amostras**, e não sobre a contagem: uma operação curta produz legitimamente
   poucas amostras, e um limiar de contagem ou seria instável em máquinas
   rápidas ou inútil em máquinas lentas. O limite de 1 ms deixa mais de dez
   vezes de margem sobre o pior valor medido.

**Consequências.** Nenhuma tool, nenhum schema, nenhuma dependência e nenhum
comportamento mudaram; o `Cargo.lock` é byte-idêntico. O que mudou é que a
frase escrita e o mecanismo que a sustenta passaram a ser a mesma coisa, e que
a fronteira "sem rede" passou a cobrir os dois crates do caminho em vez de um.

Fica registrado o resíduo honesto: a camada dinâmica é amostragem, e não prova
a ausência de um socket aberto e fechado entre duas leituras consecutivas de um
diretório. Quem fecha isso é a regra estática, que recusa as APIs nos dois
crates — e é por isso que as duas camadas são descritas como complementares em
vez de uma ser apresentada como fazendo o trabalho da outra.

## ADR-048: A IA fica fora do Note-it; o Note-it entrega contexto local rastreável

**Contexto.** A Fase 4.2 é a primeira do "Segundo Cérebro". O nome sugere uma
coisa que o produto não vai fazer: hospedar um modelo. A pergunta real é onde
colocar a inteligência, e a resposta decide todo o resto — o que é a fonte da
verdade, quem pode escrever, o que sai da máquina e o que o produto pode
prometer com honestidade.

**Decisão.**

1. **O raciocínio fica fora.** A IA interpreta, raciocina e sintetiza; o Note-it
   armazena, identifica, busca, recupera e controla escrita. O Note-it não fica
   mais inteligente, fica mais consultável.
2. **Markdown continua sendo a fonte da verdade**, sem exceção. Qualquer
   derivado é reconstruível, descartável e não autoritativo.
3. **O Context Engine vive no `noteit-core`**, somente leitura, usando apenas as
   leituras que o Core já tem.
4. **Nada é persistido na 4.2.** Contexto é calculado sob demanda.
5. **Uma única tool nova**, `noteit_context`, somente leitura. Sem Resources,
   sem Prompts.
6. **Um candidato de contexto nunca carrega uma `revision`.** *(Corrigido pela
   ADR-048.1: a decisão de não publicar `revision` continua de pé; o que estava
   errado era descrever `updated_at` como aquilo que ela substitui.)*
7. **Conteúdo de nota é dado não confiável**, inclusive para o próprio servidor.
8. **A fronteira de privacidade é declarada em voz alta**: o Note-it não abre
   rede; um host de nuvem pode encaminhar o que a tool devolveu.

**Justificativa.**

1. **Por que a inteligência fica fora.** Um modelo dentro do Note-it traria
   credenciais, rede, download de pesos, um daemon de inferência e uma segunda
   coisa capaz de afirmar o que a pessoa "sabe" — cada um contrariando um
   princípio já publicado em `docs/vision.md`. E seria a arquitetura errada mesmo
   sem isso: a Fase 4.1 já construiu a superfície pela qual um modelo externo
   fala com o store, com identidade, precondição e autoridade. O Segundo Cérebro
   é uma camada de *recuperação* sobre aquilo, não um segundo cérebro literal.

2. **Por que Markdown continua no comando.** É o único princípio que garante que
   apagar tudo o que a fase 4.2 vier a criar não custe uma nota. Um índice que
   fosse fonte da verdade transformaria corrupção de cache em perda de dados —
   e a série 4.0R foi inteira sobre não deixar isso acontecer.

3. **Por que o Context Engine é do Core.** O conhecimento é do domínio. Colocá-lo
   no `noteit-mcp` o tornaria inalcançável para a GUI e para a CLI, e criaria um
   segundo lugar que sabe o que é uma nota — exatamente o que a Fase 4.1 evitou
   ao mover a autoridade de escrita para o Core. Ele usa as leituras existentes
   e não abre arquivo: performance não pode comprar integridade.

4. **Por que nada é persistido agora.** Medido, com store sintético e cache
   quente: uma busca custa 10 ms com 100 notas, 48 ms com 1 000 e 435 ms com
   10 000. Para o tamanho real de um store de notas adesivas, sob demanda é
   confortável. Um índice v1 traria staleness, invalidação, corrupção, backup,
   restauração, migração e um artefato capaz de discordar das notas — para um
   problema que ainda não existe. A política de um cache futuro já está escrita
   (`XDG_CACHE_HOME`, invalidação por `revision`, nunca autoriza escrita), então
   adiar não é deixar em aberto.

5. **Por que uma tool só.** A pergunta de recuperação é uma pergunta; "leia esta
   nota inteira" já é `noteit_read`. Mais tools aumentariam a superfície
   auditada sem resolver nada. Resources foram recusados porque um Resource é
   conteúdo que o host busca sem decisão do modelo — literalmente o despejo de
   contexto que a minimização existe para evitar, e cujo custo de privacidade
   recai sobre a pessoa quando o host é remoto. Prompts foram recusados porque
   são texto do servidor que orienta o modelo, e misturá-los com conteúdo de
   nota seria construir instrução a partir de dado não confiável.

6. **Por que nenhuma `revision` no candidato — a decisão mais contra-intuitiva.**
   A orientação inicial da fase preferia `{note_id, revision}` em cada
   candidato. Foi recusado, e o motivo é a regra central da Fase 4.1: ninguém
   grava sobre uma nota que não leu.

   Com uma `revision` no candidato, um agente pode ver um trecho de 240
   caracteres e mandar uma mutação com aquela revisão. Se a revisão ainda for a
   atual, **a gravação passa** — sobre uma nota que ele nunca leu inteira. O
   conflito não salva, porque não há conflito. Seria a sobrescrita cega de
   sempre, com um passo a mais e uma aparência de rigor.

   O candidato publica `updated_at`, que é sinal de recência e **não**
   substituto de versão *(corrigido pela ADR-048.1: a redação original dizia que
   ele "dá o mesmo sinal", e isso é falso)*. E a proteção é **mecânica**, não
   documental: um carimbo RFC 3339 não passa em `NoteRevision::parse`, que exige
   sessenta e quatro caracteres hexadecimais minúsculos. Um agente que tentar
   usá-lo como precondição recebe `invalid_input` e não grava nada. A revisão
   nasce onde sempre nasceu: em `noteit_read` *(precisado pela ADR-048.2: a
   primeira escrita sobre uma nota vinda do contexto exige `noteit_read`; uma
   mutação bem-sucedida pode encadear a próxima pela `revision` que o próprio
   `WriteResult` publica)*.

7. **Por que conteúdo é dado.** Uma nota pode dizer "ignore as instruções
   anteriores" ou "chame `noteit_edit`". O servidor não tem avaliador, não tem
   shell, não origina tool calls, e — a regra que separa as duas coisas —
   descrições de tool, `instructions` e schemas são constantes de código, nunca
   construídas com texto de nota. O que o Note-it não pode garantir é que o
   modelo do outro lado não obedeça ao que leu; isso é do host. O que está ao
   seu alcance é entregar conteúdo rotulado, com proveniência, em quantidade
   mínima, e nunca dar ao conteúdo um caminho para virar ação.

8. **Por que a fronteira de privacidade é dita em voz alta.** "O Note-it não
   envia notas para a Internet" é verdade e está provado em cinco camadas. "Uma
   nota nunca sairá desta máquina" é **falso** se a pessoa conectar um host de
   nuvem, que encaminhará o resultado da tool ao provedor como faria com
   qualquer contexto. Esconder essa distinção seria a desonestidade mais séria
   possível num produto cujo primeiro princípio publicado é privacidade. Ela é
   também a razão de projeto para a minimização: quanto menos a tool devolve,
   menos sai da máquina quando o host é remoto.

**Consequências.** A GUI não muda — `docs/vision.md` continua descrevendo um
aplicativo minimalista de notas adesivas, e o Segundo Cérebro é um contrato para
programas, headless, através do MCP. A Fase 4.2 fica dividida em 4.2A
(arquitetura), 4.2B (Context Engine), 4.2C (superfície MCP), 4.2D (contrato do
agente), 4.2E (validação) e 4.2R (auditoria ofensiva).

Fica registrado um requisito de entrada da 4.2B descoberto nesta análise: o
`noteit-mcp` usa runtime *current-thread* com handlers síncronos e sem
`spawn_blocking`, de modo que um handler longo para o servidor inteiro — medido,
um `ping` enviado aos 0,05 s respondido aos 3,002 s. Benigno para as tools
atuais, inaceitável para uma consulta que varre o store. E o comentário em
`noteit-mcp/src/main.rs` que afirma o contrário precisa ser corrigido.

Embeddings, recuperação semântica e um eventual índice persistente ficam
registrados como Fase 4.3, e não entram disfarçados na 4.2.

### ADR-048.1: Recência não é versão, e um candidato não mistura versões (Fase 4.2A.R1)

A ADR-048 acertou a decisão e errou a explicação dela, e um contrato explicado
errado vira código errado. A auditoria da 4.2A não achou rota nova de perda de
dados; achou uma inconsistência entre o que o contrato dizia e o que o próprio
`noteit-core/src/revision.rs` afirma há duas fases. Corrigida aqui, antes de
existir implementação para acomodá-la.

**O que estava errado.** O contrato descrevia `updated_at` como a resposta à
pergunta "quando a nota mudou?" e o tratava como o sinal de staleness que
substitui a `revision` ausente no candidato. `revision.rs` diz o contrário, em
comentário normativo e em teste: a `revision` é o SHA-256 dos bytes exatos com
que a nota seria persistida e cobre *tudo* o que uma gravação poderia
sobrescrever, enquanto `updated_at` marca a última alteração do **texto** e fica
deliberadamente parado quando muda uma tag, uma propriedade, uma cor, um papel,
uma intensidade ou um tamanho de fonte. Trocar uma tag de `medicina` para
`cardiologia` move a `revision` e não move `updated_at` —
`tags_and_properties_never_move_a_timestamp` e
`every_persisted_field_moves_the_revision` provam os dois lados. Um carimbo que
não enxerga metade das mudanças persistidas não pode ser autoridade de staleness
da nota inteira.

**O que continua igual, e é o ponto importante.** A decisão de não publicar
`revision` no candidato **não** é revertida. Ela nunca dependeu de `updated_at`
ser um bom detector de staleness; depende da regra da Fase 4.1 — ninguém grava
sobre uma nota que não leu. Uma `revision` válida ao lado de um trecho de 240
caracteres deixaria um agente gravar a partir do trecho, e o conflito não o
salvaria, porque não haveria conflito. Tirar `updated_at` do papel de versão não
devolve esse papel a ninguém: o candidato simplesmente não publica token
autoritativo de versão.

**O contrato, agora dito nos termos certos.**

```text
noteit_context   candidatos: note_id, label, snippet, reason[], updated_at
                 updated_at = recência textual, informativa
                 nenhum token autoritativo de versão
noteit_read      conteúdo completo + revision   ← autoriza a primeira escrita
mutação          expected_revision, a única precondição autoritativa
```

Staleness autoritativa para escrita é resolvida por `revision`, e o contexto
nunca publica uma *(precisado pela ADR-048.2: dizer que a revisão "só nasce em
`noteit_read`" era amplo demais — o contrato também publica a revisão pós-operação
em `WriteResult`)*. A proteção continua mecânica:
`NoteRevision::parse` exige sessenta e quatro caracteres hexadecimais
minúsculos, e um RFC 3339 é recusado como `invalid_input` — um agente não
consegue promover o carimbo a precondição nem por engano.

**Um requisito novo para a 4.2B: coerência do candidato.** Os sinais de
recuperação — texto, tag, propriedade, tarefa, recência — vêm de leituras
diferentes do Core, e o store pode mudar entre elas. Um candidato montado com o
snippet de uma versão e as tags de outra não corrompe nada, mas mente sobre
proveniência, e a proveniência é o produto inteiro desta fase. Fica decidido
agora, para não ser decidido durante o código: **cada candidato deve vir de uma
projeção internamente coerente da nota**. A direção de implementação é carregar
uma projeção coerente por nota candidata, com todos os sinais daquele candidato
derivados dela *(a ADR-048.2 removeu a alternativa de "declarar a incoerência":
ela contradizia o próprio requisito)*. Inventar um lease de leitura ou dar
escrita ao Context Engine não são opções. A coerência é por nota: não há
transação sobre o store, e nunca foi prometida uma.

**Determinismo, dito sem prometer o que não existe.** "A mesma pergunta sobre o
mesmo store dá a mesma resposta" era uma promessa sobre a regra escrita como se
fosse uma promessa sobre o mundo. A formulação correta: para um mesmo estado
estável do store e a mesma entrada, a saída é a mesma — mesma seleção, mesma
ordem, mesmos motivos, sem dependência de relógio, iteração de hash, endereço,
locale ou ordem de diretório. Não há snapshot transacional, o Core não oferece
um e a 4.2 não vai construir um. Sob mutação concorrente o comportamento
continua seguro e explicável, e é assim que será descrito.

**`readOnlyHint` é annotation, não enforcement.** A ADR-048 delegou o modo
somente leitura ao host porque o servidor já publica `readOnlyHint` fiel nas 15
tools. Isso continua certo, com a distinção explícita: a annotation *descreve*
que a tool não grava e um host pode ignorá-la; o que garante a propriedade é o
servidor não ter código de escrita, não criar arquivo, não mover nota, não
chamar a autoridade de escrita, não aceitar `expected_revision`, não aceitar
caminho, não abrir shell e não abrir rede. Nenhuma garantia depende de um host
respeitar annotations.

**Os findings herdados continuam abertos.** O bloqueio do runtime do
`noteit-mcp` (current-thread, handlers síncronos, sem `spawn_blocking`, com o
comentário de `main.rs` afirmando o contrário) continua sendo **requisito de
entrada da 4.2B**, e não foi corrigido aqui de propósito: corrigi-lo é o
primeiro bloco executável da fase seguinte, não um efeito colateral de uma
correção documental. `noteit_read` sem teto de tamanho continua registrado, com
análise na 4.2B e ataque na 4.2R; ele não autoriza despejar corpos grandes no
`noteit_context`, que devolve snippet limitado sempre.

Nada de código mudou nesta correção — nenhum `.rs`, nenhum manifesto, nenhum
schema, `SCHEMA_VERSION` em 1 e o catálogo MCP em 15 tools. O que mudou foi o
contrato dizer a verdade sobre o mecanismo que já existia.

### ADR-048.2: Duas revisions autorizam escrita, e a coerência do candidato é propriedade e não preferência (Fase 4.2A.R1.1)

A ADR-048.1 fechou a confusão entre recência e versão. Ao fazê-lo, deixou duas
portas encostadas: uma frase larga demais sobre de onde vem uma revisão
autorizadora, e um requisito de coerência que trazia junto a permissão de
descumpri-lo. Ambas fechadas aqui, sem tocar em código.

**A frase larga demais.** "A revisão só nasce em `noteit_read`" descreve
corretamente o caminho que importa para a D-13 — uma nota descoberta pelo
contexto e ainda não lida — e descreve incorretamente o contrato MCP inteiro. O
`WriteResult` publica uma `revision` depois de uma operação bem-sucedida, e o
comentário que a acompanha em `noteit-mcp/src/contract.rs` é normativo: "the
note's revision after this operation, so the next conditional write needs no
extra read". Isso é deliberado e é da Fase 4.1, não uma concessão desta.

**A regra correta é mais estreita e mais exata.** Não é "toda revisão vem de uma
leitura"; é:

> Nenhuma revisão autoriza uma escrita sobre um estado que o agente não conhece.

Há duas formas legítimas de conhecer o estado, e uma ilegítima:

| Origem | Conhece o estado? | Autoriza escrita? | Precisa reler? |
| --- | :---: | :---: | :---: |
| `NoteView.revision` (`noteit_read`) | sim, acabou de lê-lo | **sim** | não |
| `WriteResult.revision` após sucesso | sim, acabou de produzi-lo e o servidor confirmou | **sim**, para encadear | não |
| `WriteResult.current_revision` (conflito) | **não**, é o hash de conteúdo que ele não viu | **não** | **sim** | *(removida do fio na ADR-051: publicá-la tornava a regra opcional)* |

O encadeamento — `read → R1 → mutação(R1) → R2 → mutação(R2)` — não é
sobrescrita cega: é uma sequência cuja base o agente conhece inteira, porque
cada passo foi ele que pediu e o servidor confirmou. A `current_revision` de um
conflito é o oposto: prova que a nota deixou de ser R1 e nada mais. O servidor já
trata as duas de forma diferente, e o comentário em `domain.rs` diz por quê —
num conflito o campo `revision` é deixado deliberadamente vazio, "or 'read
again' becomes 'retry with the token the error handed you'".

**A D-13 não muda.** O candidato de contexto continua sem `revision`, sem
`base_revision`, sem `etag`, sem qualquer token equivalente, e `updated_at`
continua sendo só recência. Uma nota descoberta pelo contexto exige
`noteit_read` antes da **primeira** mutação. O encadeamento só existe depois que
essa primeira autorização existiu, e é por isso que reconhecê-lo não abre a porta
que a D-13 fecha.

**D-27, em sua forma final.** A ADR-048.1 exigiu coerência do candidato e, na
mesma frase, ofereceu como alternativa aceitável publicar um candidato incoerente
desde que avisado. Um requisito que admite ser descumprido com aviso não é um
requisito. A alternativa está removida:

> **D-27 — Coerência interna do candidato.** Cada candidato devolvido pelo
> Context Engine representa uma projeção internamente coerente de uma única
> nota. Texto, metadados, tarefas, recência e proveniência daquele candidato não
> podem ser combinados de estados diferentes da mesma nota. A garantia é
> per-note e não implica snapshot transacional do store.

**DECIDIDA. OBRIGATÓRIA.** Não é preferência, recomendação nem melhor esforço.
`noteit_read` não a substitui: ele autoriza a primeira escrita, e isso é outra
propriedade — um candidato que mentiu sobre a própria proveniência não fica
verdadeiro porque alguém depois leu a nota.

O escopo continua sendo a nota e não o store. Candidatos de notas diferentes
podem vir de instantes diferentes; o que não pode é um único candidato misturar
versões da mesma nota. É isso que mantém a arquitetura sem snapshot global, sem
lease de leitura e sem camada de coordenação nova.

Consequência para a fase seguinte: o bloco 4.2B.6 deixa de escolher entre
coerência e incoerência declarada e passa a implementar e provar a propriedade
decidida aqui. Se a implementação encontrar prova concreta de que a coerência
per-note é inviável com as garantias atuais, a 4.2B **para e volta à decisão
arquitetural** — não existe degradação silenciosa, porque uma dificuldade de
implementação não deve reescrever a arquitetura sem auditoria.

**Uma tensão registrada, não resolvida aqui.** As `INSTRUCTIONS` do servidor
(`noteit-mcp/src/server.rs`) e o comentário de `expected_revision` em
`contract.rs` dizem ao agente que a precondição "must be the revision you read
the note at" — regra mais estreita do que o encadeamento que
`WriteResult.revision` explicitamente oferece. A regra estreita é segura, e
nenhuma escrita fica desprotegida por causa dela: o Core continua exigindo que a
precondição case com o estado atual. Mas as duas frases não dizem a mesma coisa,
e reconciliá-las é trabalho da **4.2D**, que é onde o contrato do agente é
escrito. Nenhum `.rs` foi tocado aqui para acomodar esta ADR.

Nada de código mudou: nenhum `.rs`, nenhum manifesto, nenhum schema,
`SCHEMA_VERSION` em 1, catálogo MCP em 15 tools.

## ADR-049: O protocolo respira enquanto o disco trabalha, e um candidato vem de uma leitura autoritativa

**Contexto.** A Fase 4.2B é a primeira implementação do Segundo Cérebro, e
tinha duas coisas para fazer numa ordem que não era negociável: tirar o I/O do
Core da thread do protocolo MCP, e só então construir o motor de contexto. A
segunda dependia da primeira — uma consulta que varre dez mil notas custa
centenas de milissegundos, e num servidor que faz isso no reactor esse é o
tempo em que ele não responde absolutamente nada.

**Decisão.**

1. **Toda chamada ao Core sai do reactor**, leitura tanto quanto escrita, por
   `tokio::task::spawn_blocking`.
2. **O runtime continua `current_thread`.** Ele nunca precisou de mais threads;
   precisava parar de fazer o trabalho do disco na única que tem.
3. **A regra é um tipo, não uma convenção.** Um `OffThread` é exigido por toda
   função que abre o store.
4. **O Context Engine vive em `noteit-core/src/context.rs`**, tipado, sem
   nenhum tipo de MCP, somente leitura.
5. **Um candidato vem de uma leitura autoritativa do `NoteDocument`** — D-27
   por construção. A varredura que enumera as notas não compõe o candidato.
6. **A ordenação é total**, com `note_id` como último degrau.
7. **Os limites são do motor**, não da futura tool.

**Justificativa.**

1. **Por que o tipo e não o cuidado.** A correção óbvia seria embrulhar as
   quinze tools em `spawn_blocking` e confiar que a décima sexta lembre. A
   Fase 4.1 já tinha resolvido o mesmo formato de problema de outro jeito:
   `ExistingNoteMutation` não pode ser construída sem revisão, então nenhuma
   tool pode gravar sem precondição *por não conseguir ser escrita*. Aqui vale
   o mesmo. `OffThread` tem campo privado ao módulo e um único construtor,
   dentro do fecho que o `spawn_blocking` executa; `Store::reader` e `perform`
   — as duas portas para o filesystem — exigem um. Chamar o Core no reactor
   deixou de ser um engano possível e passou a ser um erro de compilação.

2. **Por que um teste e não um grep.** Havia dois comentários afirmando que o
   offload já existia, e ambos eram falsos: o `main.rs` dizia que o I/O ia para
   uma blocking thread, o `Cargo.toml` dizia `spawn_blocking`, e não havia
   `spawn_blocking` no crate. Documentação não prova comportamento, e um grep
   pelo nome da função provaria exatamente tanto quanto os comentários
   provavam. As duas provas são sobre *ordem*, e nenhuma dorme: no caminho de
   escrita, uma autoridade falsa abre um portão no instante em que tem a
   operação — o servidor está provadamente dentro da chamada bloqueante — e só
   responde quando o teste abre o segundo; o `ping` vai entre os dois e tem de
   voltar primeiro. No caminho de leitura, que não tem autoridade para segurar,
   a pergunta é qual resposta chega antes: um reactor bloqueado não reordena
   nada, então uma busca longa responderia necessariamente antes do `ping`
   atrás dela. Ambas reprovavam contra o commit anterior.

3. **Por que uma leitura autoritativa por candidato.** D-27 dizia que um candidato não pode
   combinar sinais de versões diferentes da mesma nota. A forma óbvia de montar
   um candidato seria a errada: `list_summaries` para os metadados, `search`
   para o trecho, `list_tasks` para as tarefas, unidos por `note_id`. Cada uma
   dessas leituras é correta sozinha, e a união pode descrever uma nota que
   nunca existiu — trecho de antes de uma edição, tags de depois. O store não
   corrompe; a proveniência é que vira mentira, e a proveniência é o produto
   inteiro desta fase. `retrieve` lê a nota uma vez, constrói uma `Projection`
   daquele documento e a descarta antes da próxima; as funções de sinal recebem
   `&Projection` e nenhuma tem caminho até o store. A varredura que enumera as
   notas roda antes e pode observar o que a ordenação exigir — o que ela viu não
   entra no candidato, e é por isso que a afirmação correta é "uma leitura
   autoritativa por candidato" e não "uma leitura por nota". Misturar versões exigiria
   reescrever a função que orquestra, não esquecer um detalhe. O teste que
   prova isso alterna a nota entre duas versões que discordam de corpo, tag,
   propriedade e tarefa enquanto a consulta roda, e foi verificado contra um
   defeito injetado de propósito — uma segunda leitura para os metadados —, que
   ele reprovou de imediato.

4. **Por que a coerência é por nota e não do store.** Um snapshot transacional
   exigiria lease de leitura ou uma camada de coordenação nova, e o Core não
   oferece nem uma coisa nem outra. Também não é necessário: candidatos de
   notas diferentes virem de instantes diferentes não mente sobre nada, porque
   nenhum candidato afirma algo sobre outro. O que mentiria é um único
   candidato misturar duas versões da mesma nota, e é exatamente isso que está
   fechado.

5. **Por que `note_id` como último degrau.** "Mais motivos, depois recência"
   não é uma ordem total: duas notas escritas no mesmo segundo, ou duas sem
   `updated_at`, empatam nos dois primeiros critérios e cairiam na ordem que o
   filesystem devolveu. A mesma pergunta responderia diferente no mesmo store,
   o que é precisamente o que "determinística" promete que não acontece. O
   terceiro degrau é estabilidade, não um score escondido — e uma nota sem
   carimbo fica depois de toda nota que tem um, em vez de flutuar.

6. **Por que os limites vivem no Core.** Se o teto fosse da tool MCP, a GUI e a
   CLI poderiam pedir contexto sem teto nenhum, e a 4.2C teria que reinventar
   números que a arquitetura já decidiu. `limit` é aplicado com
   `clamp(1, MAX_CANDIDATES)`: nenhum pedido consegue passar dos cinquenta. A
   consulta longa demais é recusada e não truncada, que é a regra que
   `search::prepare_query` já seguia — responder a uma pergunta que ninguém fez
   é pior do que não responder.

7. **Por que tags e propriedades são sinais e não filtro.** `NoteFilter::matches`
   é um `AND` obrigatório, e reutilizá-lo como porta de entrada teria feito
   todo candidato carregar sempre os mesmos motivos — a contagem que ordena a
   lista não distinguiria nada. Aqui uma tag pedida é um sinal: quem a tem vira
   candidato e diz isso. A comparação continua sendo a `semantic_identity` do
   resto do produto, que é o que estava em jogo em reutilizar o Core.

**Consequências.** Medido em release, com store sintético: 6,5 ms com 100
notas, 66 ms com 1 000, 704 ms com 10 000, e 8 MiB de pico com dez mil. Linear,
cerca de 1,6× a busca da 3.8R, porque lê e analisa o `NoteDocument` inteiro de
cada nota em vez de só o corpo — é o preço da coerência do candidato, e é o
preço certo. Setecentos milissegundos em dez mil notas é perceptível, e fica
dito: para o tamanho real de um store de notas adesivas a consulta é
interativa, e desde esta fase uma consulta lenta já não congela o protocolo.
Nenhum índice foi criado para melhorar esse número — continua sendo 4.3, e
criá-lo aqui em silêncio teria sido trocar a decisão D-04 por um benchmark.

O catálogo MCP continua com 15 tools e `SCHEMA_VERSION` em 1: o motor existe e
não tem superfície. `noteit_context` é a 4.2C. Os dois findings herdados
continuam abertos — `noteit_read` sem teto de tamanho, e a tensão entre as
`INSTRUCTIONS` do servidor e o encadeamento que `WriteResult.revision` oferece,
que é da 4.2D.

### ADR-049.1: Um teto por coleção, e uma segunda porta que não abre (Fase 4.2B.R1)

A ADR-049 limitou a resposta pelo número de candidatos e pelo tamanho do
snippet. Isso limita o que a resposta **lista**, não o que cada item **carrega**,
e a diferença é a fase inteira: um teto que não cobre todos os campos variáveis
não é um teto, é uma média.

**O que crescia sem limite.** Quatro coisas, duas conhecidas e duas encontradas
ao auditar os tipos públicos por `Vec` e `String`:

| Campo | O que o fazia crescer |
| --- | --- |
| `tasks[]` | uma nota com mil checkboxes que casam publica mil |
| `warnings[]` | um store danificado publica um warning por nota ilegível |
| `matched_text` | a dobra **descarta** marcas combinantes, então `a` + cinquenta mil acentos + `b` dobra para `ab`, casa com uma consulta de dois caracteres, e o trecho publicado é o da fonte: cinquenta mil caracteres. Medido, não deduzido |
| mensagem de warning | a do Core nomeia o arquivo, e caminho absoluto não pode sair por esta superfície |

O terceiro é o interessante, porque contraria a intuição de que `matched_text`
está limitado pela consulta. Está limitado pela consulta *dobrada*, e a fonte
pode ter um número arbitrário de caracteres que dobram para nada.

O quarto não é sequer sobre tamanho. `docs/second-brain.md` §19 diz que a IA
nunca recebe caminho, e as mensagens do Core dizem "Leitura recusada: o arquivo
`/home/.../notes/<uuid>.md` é um link simbólico". Uma frase escrita para quem
depura um store, correta lá e errada aqui.

**Decisão.** Tetos explícitos no Core, e um warning sem texto livre:

```text
tasks por candidato        3
texto de uma task        121 caracteres
matched_text             241 caracteres
warnings                  20
mensagem de warning        não existe: note_id + kind
```

Três consequências que valem escrever:

1. **`task_ref` não é truncado.** Oito caracteres hexadecimais por construção,
   e um identificador encurtado para economizar espaço não nomearia tarefa
   nenhuma. Um teto que estraga o que limita não é um teto.
2. **O warning perde a mensagem em vez de ser saneado.** Tentar remover o
   caminho de uma frase livre é uma regra que alguém quebra depois; publicar só
   `note_id` e `kind` torna o vazamento impossível e o tamanho fixo de uma vez
   só. `ReadWarningKind` já distingue as quatro coisas que um chamador precisa
   saber, e a mensagem completa continua disponível em toda leitura do Core que
   não seja esta superfície.
3. **O corte é contado.** `tasks_truncated`/`omitted_task_count` por candidato,
   `warnings_truncated`/`omitted_warning_count` na resposta. Um store danificado
   continua dizendo o quanto está danificado. E a contagem de tarefas omitidas
   sai do conjunto já derivado da projeção, nunca de uma segunda leitura —
   contar não pode custar a coerência que o candidato existe para ter.

**A segunda porta.** `OffThread` prova que as funções de `domain.rs` não rodam
no reactor. O que ele não prova é que alguém não abra outro caminho: uma 16ª
tool chamando `noteit_core` direto do seu handler satisfaz todos os tipos deste
crate e trava o protocolo exatamente como antes. O testemunho é sobre *como* se
chama, e faltava uma regra sobre *onde* se pode chamar.

`scripts/check-mcp-boundary` passa a recusar acesso ao store nomeado fora de
`domain.rs`. Não é uma proibição de `noteit_core` — `server.rs` legitimamente
constrói `NoteMutation` e passa `Uuid`, e nada disso toca arquivo; a regra mira
os handles e as chamadas que alcançam o store. E, como a regra 5 já fazia com o
socket Unix, ela vem acompanhada da exigência de que o mecanismo permitido
continue existindo: `spawn_blocking` presente em `domain.rs`, `reader` e
`perform` exigindo o testemunho, e exatamente uma fábrica de `OffThread`. Sem
isso, o gate seria satisfeito por um `domain.rs` que tivesse deixado de fazer
offload — passando um teste de offload por deletar o offload.

Cinco violações foram injetadas e as cinco reprovaram, entre elas a que
importa para a fase seguinte: um `noteit_context` chamando
`noteit_core::context::retrieve` direto do handler.

**Uma correção de redação.** "Uma leitura por nota" era literal demais: a
varredura que enumera as notas lê o cabeçalho de cada uma para ordenar por
recência. A afirmação exata é uma leitura **autoritativa** do `NoteDocument` por
candidato, e nada do que a varredura observou compõe o candidato. D-27 não muda:
o que é publicado continua vindo inteiro de uma `Projection`.

Nenhuma dependência nova, `Cargo.lock` byte-idêntico, catálogo em 15 tools,
`SCHEMA_VERSION` em 1.

### ADR-049.2: A recusa também é publicação (Fase 4.2B.R1.1)

Duas rodadas de hardening cobriram o que a resposta de sucesso carrega e o que
os warnings carregam. Faltava a terceira coisa que sai por esta superfície: a
recusa.

`ContextError::StoreUnavailable(String)` guardava a mensagem do storage, e essa
mensagem nomeia o diretório — "The notes path `/home/.../notes` is not a
directory". Medido antes de corrigido: uma sonda contra um store cujo caminho de
notas era um arquivo imprimiu o caminho absoluto direto do `Display`.

É o mesmo defeito da mensagem de warning, um nível abaixo, e leva a mesma
correção pela mesma razão: **a variante deixou de ter payload**. Um tipo que não
tem onde guardar um caminho não vaza um, e ninguém precisa confiar que um
saneador continue funcionando depois de a próxima mensagem do sistema mudar de
formato. `Display` é uma frase fixa.

`QueryTooLong` mantém `limit` e `actual` porque os dois são inteiros e nenhum
ecoa a consulta de volta. As duas recusas continuam distinguíveis, e colapsá-las
em um `Failed` genérico jogaria fora uma distinção sobre a qual um chamador age:
uma vale corrigir o pedido, a outra não.

O que se perde é um diagnóstico, e só aqui: todo outro caminho de leitura do
Core continua devolvendo a mensagem inteira.

Agora a afirmação cobre a superfície toda: **tudo o que o Context Engine
publica — em sucesso, em warning ou em recusa — é tipado, de tamanho limitado ou
fixo, e não carrega mensagem livre nem caminho.**

## ADR-050: A décima sexta tool traduz e não decide

**Contexto.** O Context Engine existia no Core desde a 4.2B, limitado e provado,
e não tinha superfície. A 4.2C publica uma tool — `noteit_context` — e a
pergunta de projeto não era o que ela deveria fazer, que já estava decidido, mas
o quanto do adapter tem direito a pensar.

**Decisão.** Nada. O adapter traduz.

1. **Tipos MCP declarados no `contract.rs`**, não derivados dos do Core.
2. **`domain.rs` copia campo a campo**, e não recalcula nada.
3. **`tags` e `properties` publicadas como sinais**, com redação própria.
4. **Tarefas dentro do candidato**, não numa lista global.
5. **Nenhuma `message` em lugar nenhum** da resposta.
6. **O caminho é o mesmo das outras quinze**: handler, offload, domain, Core.

**Justificativa.**

1. **Por que tipos próprios.** É a regra que o `contract.rs` já seguia, e a
   razão continua valendo: uma mudança num tipo de domínio não pode virar em
   silêncio uma mudança no protocolo. `Candidate` no Core e
   `ContextCandidateView` no fio são duas coisas que hoje têm os mesmos campos e
   que precisam poder deixar de ter sem que ninguém descubra pelo host.

2. **Por que nada é recalculado.** A tentação óbvia é o adapter olhar
   `candidates.len()` e decidir sozinho se houve truncamento. Seria errado por
   construção: depois do corte, o que foi descartado não está mais lá, e
   qualquer número derivado dali é um palpite. Todo contador — `omitted_count`,
   `omitted_task_count`, `omitted_warning_count` — vem do Core, que os produziu
   quando ainda sabia a resposta. Pela mesma razão o adapter não ordena, não
   constrói snippet, não parseia tarefa e não corta texto: cada uma dessas
   coisas seria uma segunda implementação de uma ideia que já tem uma, e duas
   implementações de uma ideia acabam discordando.

3. **Por que `tags` não reusa o `FilterInput`.** O tipo existia, tinha os campos
   certos e teria economizado trinta linhas. Só que a descrição dele diz "every
   tag a note **must** carry to appear", e aqui uma tag é um sinal: quem a tem
   entra e diz por quê, quem não a tem ainda pode entrar por outro motivo. Um
   agente constrói sobre o que o schema diz, e um schema que descreve um filtro
   sobre um comportamento de sinal não é uma imprecisão de redação — é a tool
   mentindo sobre si mesma. O tipo separado custa pouco e diz a verdade.

4. **Por que as tarefas ficaram dentro do candidato.** A forma conceitual da
   4.2A previa uma lista global de tarefas ao lado dos candidatos. Cinco coisas
   puxam na direção oposta e nenhuma na dela: o Core já modela tarefas por
   candidato, o truncamento é por candidato, `omitted_task_count` é por
   candidato, aninhá-las torna a tradução 1:1 sem transformação inventada, e o
   leitor vê de qual nota cada conjunto nasceu sem cruzar identificadores. A
   forma conceitual era conceitual; esta é a fixada.

5. **Por que nenhuma `message`.** O macro `read_result!` que as outras leituras
   usam acrescenta um `message: Option<String>` diagnóstico. Usá-lo aqui
   reintroduziria, no mesmo commit, exatamente o campo que as duas ADRs
   anteriores gastaram uma fase cada removendo. A resposta é escrita à mão e
   tudo em que um chamador ramifica é `status` e `code`.

6. **Por que a barreira valeu a pena.** A R1 criou a regra de que o store só é
   nomeado em `domain.rs` e a provou com uma violação simulada da futura décima
   sexta tool. Aqui essa tool chegou de verdade, e a regra foi exercitada duas
   vezes: a chamada direta reprovou como esperado, e a mesma chamada escondida
   atrás de `use noteit_core::context as engine` **passou**. A regra conhecia o
   nome da função e não o do módulo. Ampliada para nomear os dois, as duas
   reprovam. Uma barreira que nunca é atacada é uma barreira sobre a qual nada
   se sabe.

**Consequências.** O catálogo tem 16 tools e essa é a única adição.
`SCHEMA_VERSION` continua em 1: ele versiona o documento da interface de máquina
da CLI, não o catálogo MCP, e isso foi verificado antes de ser deixado em paz.
Nenhuma dependência nova, `Cargo.lock` byte-idêntico, nenhum Resource, nenhum
Prompt, nenhuma extensão de Tasks do protocolo, nenhuma rede.

A propriedade que a fase entrega cabe numa frase: `noteit_context` pode
**descobrir** uma nota, e não pode entregar o corpo dela, nem um caminho, nem
uma revisão, nem uma escrita, nem uma mensagem do sistema de arquivos. Para
conhecer a nota inteira o agente usa `noteit_read`; para gravar sobre uma nota
que o contexto encontrou, precisa primeiro tê-la lido.

Continuam abertos, e nenhum foi tocado aqui: `noteit_read` sem teto de tamanho,
e a tensão entre as `INSTRUCTIONS` do servidor e o encadeamento que
`WriteResult.revision` oferece, que é da 4.2D.

## ADR-051: Um agente só grava a partir de uma revisão de um estado que ele conhece

**Contexto.** A série 4.1 construiu a precondição de escrita e a 4.2 construiu a
descoberta. Faltava dizer, de uma vez, como as duas se combinam — e, ao dizer,
descobriu-se que uma delas não estava sendo cumprida por mecanismo nenhum.

**O defeito, reproduzido antes de qualquer alteração.** Um `revision_conflict`
publicava `current_revision`: a revisão que a nota tem *agora*. As
`INSTRUCTIONS` mandavam não reutilizá-la, e essa frase era toda a proteção. O
token tem exatamente o formato de `expected_revision`, então reenviá-lo
funcionava. Medido, num store temporário:

```text
o agente lê                        R1 = f4ed09c3…  conteúdo "ESTADO ORIGINAL"
outra pessoa acrescenta um parágrafo   R2 = 042708e2…
o agente grava com R1              → revision_conflict
o conflito devolve                 current_revision = 042708e2…  (= R2)
o agente reenvia R2, sem ter lido  → status ok
corpo final                        "O AGENTE SOBRESCREVE"
o parágrafo da pessoa              sumiu
```

O agente gravou sobre um conteúdo que nunca viu, e nada no protocolo o impediu.
"Não reutilize" não é uma proteção: é um pedido.

**Decisão.**

1. **O MCP deixa de publicar `current_revision`.** O campo sai do
   `WriteResult`; o adapter lê o valor do erro do Core e o descarta.
2. **O Core continua conhecendo-o.** `WriteError::RevisionConflict` não muda:
   é tipo de domínio compartilhado, e deformá-lo para resolver um problema de
   uma superfície seria consertar no lugar errado.
3. **Nenhum substituto.** Publicar a mesma capacidade como `latest_revision`,
   `actual_revision`, `new_revision` ou `etag` não mudaria nada — o problema
   nunca foi o nome.
4. **Duas origens de revisão autorizam escrita**, e apenas elas: o `revision`
   que `noteit_read` devolveu, e o `revision` que uma escrita **bem-sucedida**
   do próprio agente devolveu.
5. **Um conflito exige releitura.** Ele não diz onde a nota está agora, e a
   leitura que diz também traz o conteúdo sobre o qual decidir.
6. **Um `indeterminate` exige verificação**, nunca repetição.
7. **Conteúdo de nota é dado.** Uma nota pode conter uma ordem, um nome de
   tool ou uma string de 64 hex apresentada como revisão. Nada disso ganha
   autoridade.

**Justificativa.**

1. **Por que a ausência do campo, e não uma instrução melhor.** É a terceira vez
   nesta série que a resposta certa é tirar a capacidade em vez de pedir que
   ninguém a use — a mensagem do warning nomeava o arquivo, a recusa do store
   nomeava o diretório, e agora o conflito nomeava a versão. Uma regra que
   depende de o outro lado cooperar protege exatamente quem já ia cooperar.

2. **Por que `WriteResult.revision` fica.** Ela parece o mesmo tipo de token e
   não é. Depois de uma escrita bem-sucedida o agente conhece o estado
   resultante: conhecia a base, escolheu a transformação, e o servidor
   confirmou. Exigir uma releitura entre duas escritas de uma mesma sequência
   custaria uma leitura por parágrafo sem fechar buraco nenhum. A distinção que
   importa não é "de onde veio o hash" mas "o agente sabe o que aquele estado
   contém".

3. **Por que `expected_revision` continua ecoado no conflito.** É a precondição
   que o próprio cliente mandou. Não revela estado desconhecido, já está stale
   naquele ponto, e serve para saber qual de várias escritas foi recusada.

4. **Por que as `INSTRUCTIONS` mudaram.** Elas diziam que
   `expected_revision` deve ser "the revision you read the note at", o que
   ignorava o encadeamento que o próprio `WriteResult` oferece — a contradição
   registrada como 4.2D-F001 na 4.2A.R1.1. Agora nomeiam as duas origens
   legítimas e recusam as ilegítimas por nome, inclusive a revisão encontrada
   dentro de uma nota.

5. **Por que não existe `must_reread`.** `ErrorCode::RevisionConflict` já diz
   isso. Um campo extra afirmando o que o código do erro significa é contrato a
   mais para manter e uma segunda fonte da mesma verdade.

**Consequências.** É uma quebra deliberada do contrato MCP, não uma
compatibilidade preservada com um atalho inseguro. Nenhuma tool nova, catálogo
em 16, `mutation_input!` em 8, `SCHEMA_VERSION` da CLI intocado, nenhuma
dependência nova.

E o limite da promessa fica escrito, porque exagerá-la seria o mesmo tipo de
erro: isto **não** prova que nenhum cliente jamais grava um estado que não leu.
Um cliente que descubra um hash por fora é outro modelo de ameaça, e o servidor
não tem como provar a origem de uma string de 64 hex. O que é verdade é mais
estreito e verificável: **o servidor não entrega mais, por conflito nem por
contexto, um token novo capaz de gravar sobre conteúdo que o agente não leu.**

Fica também corrigido o `4.2C-DOC-001`: as descrições diziam "no máximo 240
caracteres" onde 240 é o **conteúdo selecionado** e o truncador acrescenta uma
reticência onde cortou — um snippet cortado nas duas pontas chega a 242. A
documentação passou a distinguir orçamento de conteúdo e string publicada, em
vez de o código ser mexido para a documentação parecer certa.

## ADR-052: O que a validação ponta a ponta provou, e o que ela não provou

**Contexto.** Cinco fases construíram o Segundo Cérebro v1 em pedaços, cada um
com a sua própria prova. Peças provadas isoladamente é o estado em que um
sistema costuma parecer pronto e não estar: as propriedades que se perdem são as
das *transições*, e nenhum teste unitário olha para uma transição.

**Isto não é uma decisão nova.** É o registro do que foi verificado, e existe
porque a diferença entre "os testes passam" e "o fluxo funciona" precisa estar
escrita em algum lugar que sobreviva à memória de quem executou.

**O que foi provado**, com o binário real, por pipes reais, sobre stores
descartáveis:

1. **A cadeia inteira.** Pergunta → candidatos sem revisão → leitura →
   conteúdo + revisão → escrita condicional → nova revisão → próxima escrita.
   E os três desfechos de uma escrita: sucesso que encadeia, conflito que exige
   releitura, indeterminado que exige verificação.
2. **Os dois caminhos de escrita são a mesma coisa por fora.** Append, no-op e
   conflito comparados campo a campo entre direto e autoridade. Se divergissem,
   as regras de um agente passariam a depender de haver uma janela aberta.
3. **Um no-op nomeia um estado encadeável**, nos dois caminhos — o que fecha o
   `4.2D-TEST-001` e removeu do teste anterior a tolerância a dois
   comportamentos, que era a porta pela qual os caminhos divergiriam calados.
4. **Duas escritas sobre a mesma base não vencem as duas.** Exatamente um
   commit, exatamente um conflito.
5. **Um conflito não custa o trabalho de ninguém.** Releitura mostra o que
   mudou, a decisão é refeita, e o resultado contém as duas alterações.
6. **`indeterminate` é ambíguo de propósito.** As duas metades — comitou e caiu,
   caiu antes de comitar — são idênticas de fora. É a prova de que repetir
   automaticamente estaria errado metade das vezes.
7. **Texto que alguém está digitando continua intocável.** Uma escrita sobre a
   revisão do arquivo é recusada quando a janela segura texto não salvo, e a
   recusa não vaza esse texto.
8. **O que a fase 4.2 prometeu sobre limites, caminhos e conteúdo** continua
   verdadeiro no fio: contexto limitado sob store adversarial, warnings sem
   caminho, recusa de store sem caminho, sessão de leitura byte-idêntica,
   conteúdo hostil sem autoridade, protocolo respondendo durante uma escrita
   presa.

**O que não foi provado, e é importante dizer.**

- **`noteit_read` continua sem teto de resposta** (`4.2A-002`). Os testes usam
  nota grande e mostram que o *contexto* permanece limitado, mas uma leitura
  integral de uma nota enorme continua produzindo uma resposta enorme. Aberto,
  e é da 4.2R. *(Fechado na 4.2R: reproduzido, medido e limitado — ADR-053.)*
- **O Context Engine descreve o store persistido**, não a janela: texto não
  salvo não aparece num candidato. É a arquitetura como foi construída — o
  motor é somente leitura e não participa do protocolo de controle — e agora
  está sob teste para que uma mudança nisso seja uma decisão e não uma
  descoberta.
- **Um agente ainda pode se comportar mal.** O que o servidor garante é
  mecânico e estreito: ele não entrega token de escrita por descoberta nem por
  conflito. Que o agente releia depois de um conflito, não repita um
  indeterminado e trate nota como dado continua sendo contrato normativo,
  publicado nas `instructions` — e um cliente que o ignore ainda é recusado
  pela precondição, mas não por ela ser impossível de tentar.
- **Nada aqui é auditoria ofensiva completa.** Isto é o túnel de vento com as
  cargas previstas. A 4.2R é quem tenta arrancar a asa.

**Consequências.** Nenhuma alteração de produção foi necessária: a fase mexeu em
testes e documentação. Benchmarks reexecutados e alinhados ao histórico
(≈7 ms com 100 notas, ≈77 ms com 1 000, ≈710 ms com 10 000, sob carga de
máquina alta), sem regressão material. Catálogo em 16 tools, `mutation_input!`
em 8, zero dependência nova, `Cargo.lock` byte-idêntico.

---

## ADR-053: Uma leitura entrega o estado inteiro ou não entrega revisão nenhuma

**Contexto.** A validação da 4.2E deixou um achado aberto e nomeado: `4.2A-002`,
`noteit_read` sem teto de resposta. A auditoria ofensiva da 4.2R reproduziu-o
contra a baseline `c5fe1bb`, em store isolado, antes de tocar em produção:

| corpo da nota | JSON-RPC no fio | latência | RSS de pico |
| ---: | ---: | ---: | ---: |
| 64 KiB | 134 558 B | 33 ms | — |
| 256 KiB | 535 640 B | 119 ms | — |
| 1 MiB | 2 139 962 B | 442 ms | — |
| 4 MiB | 8 557 247 B | 1 914 ms | — |
| 8 MiB | 17 113 627 B | 3 925 ms | — |
| 16 MiB | 34 226 387 B | 7 811 ms | 153 MB |

Crescimento linear, sem teto, com um multiplicador de **2,04×** o corpo. E o
mesmo corpo em texto denso de aspas, contrabarras, tabulações e emoji: **2,88×**.

**O multiplicador é a primeira metade da decisão.** Ele não vem do escape apenas.
Um `CallToolResult` com conteúdo estruturado publica o payload **duas vezes** —
uma como `structuredContent`, outra como um bloco de texto com o mesmo JSON
dentro de uma string, para um host anterior ao conteúdo estruturado. A segunda
cópia é a primeira escapada de novo. Logo:

```text
resposta = payload + escape(payload) + 76 bytes de envelope
```

Exato, verificado contra o fio, e a razão pela qual `content.len() <= N` estaria
errado por mais do que o dobro. `noteit-mcp/src/budget.rs` mede serializando o
payload através de um escritor que conta e descarta: uma passada, sem alocar a
resposta que vai ser recusada.

**A segunda metade é o que não se pode fazer.** A saída óbvia — devolver o
começo da nota e a revisão — é proibida:

```text
agente vê parte da nota
        ↓
recebe a revision do estado inteiro
        ↓
grava sobre um conteúdo que nunca leu
```

Isso é exatamente a falha que a ADR-051 fechou no caminho do conflito, chegando
pelo caminho da leitura. Então:

> Se `noteit_read` não pode entregar integralmente o estado que a revisão nomeia,
> ele não pode entregar aquela revisão.

**Decisão.** `MAX_READ_RESPONSE_BYTES = 4 MiB`, sobre o `CallToolResult`
serializado. Acima disso, `response_too_large`: sem corpo, sem revisão, sem
rótulo, sem carimbos, sem caminho, sem pedaço de conteúdo. Não há `offset`,
`range`, `cursor` nem paginação — isso seria protocolo novo, e a 4.2R resolve
limite, não recurso.

**Por que quatro megabytes.** Não por gosto. O único teto que já existia sobre
este mesmo dado é `control::MAX_FRAME_BYTES`, 1 MiB: quando uma janela do
Note-it segura o store, toda escrita viaja até ela como um quadro, então um
megabyte é o maior corpo inteiro que o caminho de escrita consegue carregar.
Uma leitura que recusasse abaixo disso publicaria notas que a própria aplicação
não consegue percorrer de volta. Com a expansão medida coberta, quatro
megabytes preservam a propriedade:

> uma nota cujo corpo inteiro a escrita consegue carregar é uma nota que a
> leitura consegue publicar.

Verificado nos dois sentidos: 1 MiB de ASCII responde em 2 140 072 B, e 1 MiB de
texto escapado responde em 3 227 203 B — ambos publicados. A fronteira medida no
fio: o maior corpo publicado é de 2 055 570 bytes, cujo `CallToolResult` pesa
**exatamente** 4 194 304 bytes; um byte a mais de nota e a resposta é uma recusa
de 533 bytes.

**Para escala.** Uma leitura comum responde em cerca de 800 bytes. A maior nota
que qualquer suíte deste repositório constrói — 400 000 caracteres — responde em
836 560. O store da máquina onde isto foi escrito tem 154 notas, e a maior
ocupa 1 595 bytes.

**O teto é do adaptador, não do Core.** O Core continua capaz de ler uma nota de
qualquer tamanho: a GUI, o CLI e a pesquisa não ganharam limite nenhum. O MCP é
quem decide se consegue publicá-la inteira. Com a medição feita antes de
construir a resposta, recusar 16 MiB custa uma passada sobre 16 MiB em vez dos
34 MB de resposta e 153 MB de processo que a reprodução mediu.

**Consequência honesta.** Uma nota deliberadamente feita de caracteres de
controle expande até treze vezes e é recusada mais cedo. É a consequência de
limitar o fio em vez do arquivo, e é a correta: o número que importa para quem
recebe a resposta é o número de bytes que ele recebe.

---

## ADR-054: Uma mensagem pública é uma frase que o servidor escreveu

**Contexto.** A auditoria ofensiva da 4.2R foi atrás dos `ErrorCode` públicos com
canários e mediu o que cada recusa e cada warning carregava. Quatro achados
materiais, todos da mesma raiz:

```text
4.2R-001  noteit_list, noteit_search e noteit_tasks_list publicavam o caminho
          absoluto do arquivo:
          "Leitura recusada: o arquivo `/home/…/note-it/notes/….md` é um link
          simbólico."
          e noteit_read publicava
          "Failed to read note /home/…/….md: Permission denied (os error 13)"

4.2R-002  as mesmas três publicavam um warning por arquivo danificado, sem teto:
          2 000 symlinks devolviam 2 000 warnings e 920 KB para um `limit: 1`;
          20 000 devolveriam 9,2 MB

4.2R-003  noteit_trash_list não tinha teto nenhum e nem `limit` para pedir um:
          20 000 notas descartadas responderam em 9 595 659 bytes

4.2R-004  mensagens públicas repetiam a entrada e o front matter da nota no
          tamanho em que chegaram: um seletor de 300 000 bytes voltava em
          300 098; um escalar de front matter de 300 000 bytes virava um
          warning de 300 111
```

**A raiz é uma só.** As frases eram o `Display` do Core, e o Core as escreve para
quem está depurando um store. Elas nomeiam o arquivo, citam o parser, repetem o
argumento. O `code` era tipado e a frase ao lado dele não era.

Essa decisão já tinha sido tomada uma vez, para o Context Engine: a 4.2C criou
`ContextWarningView` com `code` e `note_id` e sem `message`, e o comentário no
código dizia por quê — "a mensagem do Core nomeia o arquivo". A conclusão foi
aplicada a uma superfície e não às outras quatro.

**Decisão.**

1. **`message` é `&'static str`.** Não `String`. Uma recusa recebe uma constante
   escolhida pelo `code` (`contract::message_for`) ou uma das três que uma tool
   passa à mão. Uma frase montada em tempo de execução não tem como chegar ali —
   é tipo, não disciplina, e `error.to_string()` deixou de ser chamado.
2. **Um warning é `code` e `note_id`.** Em todas as leituras, não só no contexto.
3. **Warnings têm teto de 20**, com `warnings_truncated` e
   `omitted_warning_count`, como o contexto já tinha.
4. **A lixeira tem teto de 100**, com `truncated` e `omitted_count`, igual às
   outras listagens.

**O que se perde.** Uma frase que ninguém podia usar para decidir nada — o
contrato sempre disse "diagnostic only, never branch on it". O `code` diz o que
aconteceu, o `note_id` diz onde olhar, e quem precisa reparar o arquivo tem o
arquivo. Uma assertiva de teste que exigia a frase antiga foi substituída pela
propriedade mais forte: a de que não existe frase.

**O que fica medido e não corrigido.** `noteit_list` sobre um store construído
para isso — 100 notas, cada uma com 32 tags de 64 caracteres e 32 propriedades
de valor 512 — responde em 4 377 153 bytes. É finito, é o produto de tetos que o
Core já impõe, e é o maior envelope desta superfície. Não foi reduzido de
propósito: a listagem é recuperável pelo `limit` que o chamador controla, e
recusá-la não teria a alternativa que uma leitura recusada tem. Está registrado
como característica medida, não como dívida escondida.

---

## ADR-055: Um erro de desserialização não é lugar para o servidor repetir o que recebeu

**Contexto.** A 4.2R fechou `4.2R-004` — mensagens públicas repetindo a entrada —
tornando todo `message` do MCP um `&'static str` escolhido pelo `code`. A
propriedade era verdadeira e a área em que ela valia era menor do que a auditoria
supôs: ela vale para o que o **domínio** diz, e o domínio só fala depois que os
argumentos foram desserializados no tipo de entrada da tool.

Antes disso há uma fronteira que a 4.2R não olhou. O `Parameters<T>` do SDK
desserializa `arguments` e, quando falha, monta a recusa assim:

```rust
ErrorData::invalid_params(format!("failed to deserialize parameters: {error}"), None)
```

`error` é do `serde_json`, que escreve suas mensagens para quem está depurando um
payload: `invalid type: string "…", expected u32` carrega a string inteira, e
`unknown variant \`…\`` carrega a variante inteira. Reproduzido no fio, contra o
binário real, por pipes reais, com store sintético:

```text
tool               campo          enviado                  respondido   canário
noteit_list        limit          string de 300 KiB        307 361 B    sim
noteit_search      limit          string de 300 KiB        307 361 B    sim
noteit_tasks_list  state          variante de 300 KiB      307 387 B    sim
noteit_context     include_tasks  string de 300 KiB        307 367 B    sim
noteit_edit        clear          string de 300 KiB        307 367 B    sim
noteit_list        tags           string de 300 KiB        307 368 B    sim
noteit_create      properties     item de 300 KiB          307 374 B    sim
(JSON-RPC)         method         nome de 300 KiB          307 261 B    sim
```

A última linha é a mesma classe uma camada acima: o `on_custom_request` padrão do
`ServerHandler` responde uma requisição que não roteia com o nome do método que o
cliente escolheu.

**Por que a auditoria anterior não viu.** Havia um teste enviando exatamente
esses valores — o `r16` da suíte ofensiva, que percorre `limit` adversariais —
e ele dizia `let Ok(result) = … else { continue };`. Um valor recusado pelo
schema era pulado sem ser examinado. O teste que tinha a entrada certa na mão
tinha decidido que uma recusa não precisava ser olhada.

**Decisão.**

1. **Uma fronteira própria de parâmetros.** `noteit-mcp/src/params.rs` declara
   `SafeParameters<T>`, e `server.rs` a importa como `Parameters` — que é o nome
   que a macro `#[tool]` procura para derivar o `inputSchema`. Uma tool escrita
   do jeito comum recebe a segura sem saber que ela existe; voltar à insegura
   exige nomeá-la, e o gate recusa o nome.
2. **O erro é descartado sem ser lido.** O `Err(_)` não liga o erro a nome
   nenhum: não há o que formatar, registrar ou anexar como `data`. A recusa é
   uma constante — `INVALID_ARGUMENTS` — igual para todo campo, todo tipo e todo
   tamanho.
3. **A recusa é erro de protocolo `-32602`.** É a classificação do próprio MCP:
   argumento inválido é forma errada de requisição, e uma requisição malformada
   nunca foi uma chamada. O SDK a roteia para o canal de *tool result*, mas só
   farejando o prefixo literal `failed to deserialize parameters:` na própria
   mensagem — um detalhe privado dele, que este servidor deliberadamente não
   produz. O canal novo também é o contrato mais coerente: um `CallToolResult`
   deste servidor sempre carrega `structuredContent`, e o antigo não carregava.
4. **O método não é ecoado.** `on_custom_request` é sobrescrito e responde
   `-32601` com uma constante, sem nomear o método nem ler os `params`.
5. **Nenhuma frase deste crate é montada em tempo de execução.** O último
   `format!` fora do `main.rs` — a falha de serialização da própria resposta —
   virou constante, e o gate reprova qualquer `format!` novo em
   `noteit-mcp/src` fora do `main.rs`, que escreve para a saída de erro e nunca
   para o protocolo.

**A propriedade, e por que é uma só e não cinco.** Corrigir os cinco campos que
a reprodução encontrou deixaria o sexto para quem o escrevesse. A propriedade é
de classe: *nenhum texto derivado dos argumentos do cliente chega ao fio*, porque
existe um único extractor, ele é o único lugar onde um erro de desserialização
nasce, e ele não olha para esse erro.

**Medido depois.** As mesmas chamadas respondem em 112 e 113 bytes; o método
desconhecido, em 103. E a forma forte: um argumento de 1 KiB, 64 KiB, 300 KiB e
1 MiB no mesmo campo recebe **o mesmo número de bytes** — um teto seria
satisfeito por uma recusa que ecoasse os primeiros quinhentos bytes, e uma
igualdade não é.

**O que se perde, dito por inteiro.** A recusa não nomeia mais o campo. Nomear um
campo seria seguro em si — é uma constante do schema —, mas produzi-lo não seria:
o `serde_json` reporta a falha como frase e não como caminho, então o nome teria
que ser recuperado analisando a frase que cita a entrada, e um parser sobre essa
frase é exatamente o mecanismo que esta ADR remove. Os campos obrigatórios estão
publicados no `inputSchema` de `tools/list`, e as `INSTRUCTIONS` dizem que toda
mutação exige `expected_revision`. Quatro testes que afirmavam a frase antiga
foram substituídos pela propriedade mais forte, num auxiliar único do harness:
código `-32602`, a frase constante, e o arquivo intocado.

**O que não mudou.** Os 16 tools, os schemas publicados — comparados documento a
documento contra os do tipo embrulhado, para cada uma das 15 entradas —, quais
requisições são aceitas, a exigência de `expected_revision` no schema e no tipo,
e o comportamento de `revision_conflict`. Nenhuma dependência nova; `Cargo.lock`
byte-idêntico.

**O que fica fora do alcance, medido e registrado.** Uma linha sintaticamente
inválida na entrada padrão não recebe resposta nenhuma — é o transporte do SDK, e
não vaza nada. `tools/list` responde em 123 977 bytes, que é o catálogo deste
servidor e não a entrada de ninguém.

---

## ADR-056: A recuperação melhora primeiro por termos, e o embedding é de quem o usuário escolher

**Contexto.** A Fase 4.3 foi reservada para embeddings locais, índice vetorial e
ranking por similaridade. A 4.3A não implementou nada disso: mediu primeiro, e a
medição mudou a ordem da fase.

**Problema.** O Context Engine casa **a consulta inteira como substring** do
texto dobrado. Não há casamento por termo e não há pontuação. Medido contra o
binário real, por stdio, sobre um store sintético de 30 notas e 32 consultas com
ground truth explícito — corpus versionado em `docs/retrieval-corpus.json`:

```text
19 das 30 consultas com resposta voltam vazias
R@1 0,333   R@3 0,367   R@5 0,367   MRR 0,350
```

"hipertensão arterial" não acha a nota sobre pressão alta. "problemas para
dormir depois do plantão" não acha a nota sobre insônia após trabalho noturno.
Nenhuma das duas falha por falta de semântica.

**Opções consideradas e o que cada uma mediu.**

```text
motor                                   R@1     R@3     R@5     MRR    custo
lexical de hoje (baseline)             0,333   0,367   0,367   0,350   —
lexical por termos                     0,667   0,767   0,833   0,732   nenhum
BM25                                   0,667   0,767   0,833   0,728   nenhum
semântico e5-small fp32                0,767   0,867   0,933   0,830   470 MB + ONNX
semântico e5-small int8                0,667   0,900   0,933   0,783   118 MB + ONNX
semântico paraphrase-MiniLM fp32       0,700   0,867   0,933   0,793   470 MB + ONNX
semântico potion (estático)            0,700   0,933   0,967   0,812   512 MB
semântico static-mrl (estático)        0,767   0,900   0,933   0,845   434 MB
semântico e5-small + chunks            0,833   0,867   0,967   0,870
semântico potion + chunks              0,767   0,967   0,967   0,861
RRF BM25 + e5-small chunks             0,867   0,967   1,000   0,919
RRF BM25 + potion chunks               0,767   1,000   1,000   0,867
BM25 → potion (encadeado)              0,767   0,900   0,967   0,845
```

**A leitura que decide a fase.** O passo lexical entrega **+0,40 de R@3 sem
dependência, sem modelo, sem cache e sem superfície de privacidade nova**. O
passo semântico entrega mais **+0,13** e custa um artefato de 100 a 512 MB e uma
dependência. Os dois se justificam. A ordem não estava decidida e agora está: a
4.3 começa pelo lexical.

**Decisão.**

1. **Ampliar o Context Engine, não criar motor paralelo.** `context::retrieve`
   tem um único consumidor hoje (`noteit-mcp/src/domain.rs`), e um segundo motor
   duplicaria as regras que a 4.2 levou seis subfases para acertar — leitura
   autoritativa por candidato, ausência de `revision`, tetos, warnings sem
   caminho.
2. **Casamento por termo com BM25**, sobre a dobra que o `search::fold` já faz.
   Nenhuma normalização nova: acento já é resolvido, e duas normalizações seriam
   duas verdades sobre a mesma palavra.
3. **Embeddings estáticos de token** (classe model2vec / static-embedding), não
   transformer. 1 250–1 400 notas/s contra 23–29, qualidade dentro do ruído do
   corpus, e — o que decide — **nenhum runtime de inferência**: um modelo
   estático é uma tabela e uma média. Sem ONNX Runtime, sem C++, sem binário
   baixado em tempo de build, sem risco à fronteira de rede fechada na 4.1R1.1.
4. **Classes de precedência, não fusão.** O resultado é a concatenação de quatro
   classes, cada uma ordenada internamente, sem reordenação entre elas:

   ```text
   classe 1  SINAIS DECLARADOS  TextMatch · SharedTag · PropertyMatch · TaskMatch
   classe 2  TERMOS             TermMatch                              (4.3B)
   classe 3  SEMÂNTICA          SemanticMatch                          (4.3C)
   classe 4  RECÊNCIA           Recent — exclusiva, só existe sozinha
   ```

   **Os quatro sinais de hoje ficam na mesma classe, e isso foi medido**
   (4.3A.R1.2). Hoje `TextMatch` não tem precedência sobre `SharedTag` nem
   `PropertyMatch`: a ordem é por contagem de motivos, e uma nota com
   `shared_tag` + `property_match` fica acima de uma com `text_match` sozinho —
   verificado contra o binário real. Pôr `TextMatch` numa camada acima das outras
   três mudaria esse comportamento em silêncio, sem que nenhuma auditoria
   tivesse pedido. Então a classe 1 é o conjunto de admissão que o motor já tem,
   com a regra de ordenação que ele já usa, e as classes 2 e 3 são
   **estritamente aditivas**: acrescentam candidatos abaixo de tudo o que já
   existia e não movem nada.

   Daí a propriedade na forma exata em que ela é verdadeira: um candidato
   admitido por `TextMatch` nunca é rebaixado **por `TermMatch` nem por
   `SemanticMatch`**, porque estes vivem em classes inferiores. Ele continua
   podendo ficar atrás de um `SharedTag` com mais motivos, como hoje — isso é
   preservação, não regressão.

   Um candidato aparece uma vez, na classe mais alta que o admitiu, e carrega
   todos os motivos aplicáveis; assim que entra na classe 1, BM25 e similaridade
   deixam de influenciar sua posição. Sem consulta não há classe 2 nem 3 — não há
   termo a pontuar e embutir uma consulta vazia não significa nada —, então
   filtro sozinho continua sendo classe 1, e requisição vazia continua sendo
   classe 4 e só. Nenhum candidato fica sem classe, e cada classe termina em
   `note_id`. A RRF pontua um pouco melhor em R@3 e **rebaixou um acerto
   exato** numa consulta; a concatenação não pode rebaixar. A 4.3A.R1 estendeu a
   propriedade ao **BM25** e não só ao semântico — a 4.3B é quem introduz o BM25,
   e um ranking BM25 sobre todos os candidatos poria um casamento por termo à
   frente de um casamento de frase exata. Desempates dentro de cada camada são
   deterministas e terminam sempre em `note_id`, como a ordenação de hoje já
   termina.
5. **Chunk por parágrafo**, teto de 800 caracteres com corte em sentença, sem
   sobreposição. Identidade `note_id` + `revision` + ordinal: a revisão canônica
   já é o detector de staleness que existe, e não é preciso inventar outro.
6. **Índice em memória, força bruta, sem persistência em v1.**
7. **Sem score publicado em v1**, e um `Reason::SemanticMatch` no lugar.

**Benchmark de escala** (protótipo Python, modelo estático):

```text
escala    indexar    matriz    consulta p50   p95
   100    0,07 s     0,10 MB      0,012 ms    0,024 ms
 1 000    0,79 s     1,02 MB      0,072 ms    0,091 ms
 5 000    4,02 s     5,12 MB      5,25 ms     7,47 ms
10 000    7,13 s    10,24 MB      3,51 ms     6,91 ms
```

**Persistência: dispensada, com gatilho.** O store real desta máquina tem 41
notas — cerca de 30 ms para embutir. Mil custam 0,8 s; dez mil, 7 s. O que custa
não é o índice, é o artefato do modelo. Persistir 10 MB de vetores para poupar 7
segundos enquanto se carregam 100 MB de pesos é otimizar a metade errada. Um
cache entra quando a indexação a frio passar de **2 s num store real**, e então
em `$XDG_CACHE_HOME/note-it/`, nunca em `notes/`, com cabeçalho de validade e
renomeação como ponto de commit.

**ANN: dispensado, com gatilho.** Consulta por força bruta custa 3,5 ms com
10 000 vetores. ANN entra se passar de 50 ms, o que fica em centenas de milhares
de vetores.

**A medição que restringe a arquitetura.** Nenhum limiar de similaridade separa
"tem resposta" de "não tem resposta":

```text
e5-small     menor topo-1 com resposta 0,8248   maior sem resposta 0,8494
potion       menor topo-1 com resposta 0,1760   maior sem resposta 0,3469
static-mrl   menor topo-1 com resposta 0,0995   maior sem resposta 0,1486
```

Hoje o motor devolve vazio quando nada casa, e isso é informação verdadeira. Um
motor semântico sempre tem vizinho mais próximo e devolveria dez. Por isso
candidatos puramente semânticos são rotulados e limitados, em vez de cortados por
um número que não separa nada.

**Privacidade.** Conteúdo de nota é dado privado. Nenhuma API remota, nenhuma
telemetria, nenhum upload, nenhuma inferência em servidor. O artefato do modelo é
obtido em desenvolvimento ou empacotado; conteúdo do usuário nunca sai da
máquina. A escolha de modelo estático elimina o vetor de risco mais concreto: o
`ort` traz `ureq` como dependência opcional para baixar binários do ONNX Runtime
em tempo de build, e a fronteira de rede do MCP existe justamente para que isso
não entre.

**Fallback.** Falta de modelo, artefato corrompido, dimensão errada, valor não
finito, cache ilegível ou memória insuficiente degradam para lexical e dizem
`semantic_unavailable`. Nada disso afeta ler, escrever, listar, buscar, a CLI, o
MCP ou as notas. A etapa lexical não depende de nada da semântica — é por isso
que é implementada primeiro e sozinha.

**Impacto na arquitetura existente.** Nenhum, nesta fase: nenhum `.rs` foi
tocado, o catálogo continua com 16 tools, `SCHEMA_VERSION` não se move,
`Cargo.lock` é byte-idêntico e nenhuma dependência foi adicionada. Quando a
implementação vier: `Reason` ganha variantes, o Context Engine ganha etapas, e
nada em `revision`, `expected_revision`, `WriteResult` ou no protocolo muda.

**Alternativas rejeitadas.**

* **Banco vetorial ou índice ANN** — 3,5 ms de força bruta com 10 000 vetores
  não justifica estrutura, parâmetros, não-determinismo e dependência.
* **Índice persistente em v1** — otimiza a metade barata do custo.
* **Transformer local (`e5-small`, `MiniLM`)** — 50× mais lento a indexar,
  qualidade dentro do ruído, e traz um runtime de inferência C++ para o centro
  do Segundo Cérebro.
* **RRF** — melhor em R@3 e sem a garantia estrutural de não rebaixar.
* **Corte por limiar de similaridade** — medido, não separa.
* **API remota de embeddings** — incompatível com a proposta local; nunca esteve
  em consideração e fica registrado que não esteve.
* **Substituir o motor lexical pelo semântico** — o semântico sozinho perde
  casos exatos que hoje funcionam.

**Provider abstraction.** Existe uma interface, `EmbeddingProvider`, e nenhuma
lógica de fornecedor espalhada pelo chunker, pelo índice, pelo ranking ou pelo
Context Engine. Ela expõe `embed_document` e `embed_query` **separadamente**, e
isso não é simetria estética: o `e5` exige os prefixos `passage: ` e `query: `, a
Voyage prepende instruções diferentes conforme `input_type`, e o
`gemini-embedding-001` tem `RETRIEVAL_DOCUMENT` e `RETRIEVAL_QUERY`. Uma função
única obrigaria cada chamador a saber disso, que é exatamente o acoplamento que a
interface existe para conter. Não existe `AnthropicProvider`: em 2026-09-04 a
documentação oficial diz *"Anthropic does not offer its own embedding model"* e
aponta a Voyage. Registrar um provider inexistente seria inventar API.

**Local vs remote.** Dois modos oficiais e intercambiáveis. O local não envia
nada, funciona offline, não tem chave nem cobrança, e roda em processo — um
modelo estático é uma tabela e uma média, não há runtime a isolar. O remoto é
**opt-in explícito**: nada sai da máquina porque uma atualização habilitou
recuperação semântica. A distinção é dita e não deduzida — "o conteúdo não sai
desta máquina" contra "trechos das suas notas são enviados para <provider>".
Índice local não é privacidade: se o provider é remoto, o texto saiu para ser
embedado, mesmo que o vetor volte. E o LLM é independente do provider: o host MCP
não escolhe o fornecedor de embeddings, a configuração do Note-it escolhe.
Nenhuma regra do tipo "ChatGPT → OpenAI" ou "Claude → Voyage".

**Network boundary.** A fronteira de rede do MCP e do Core **não é afrouxada**.
Ela é o motivo do desenho:

```text
noteit-mcp ──► noteit-core ──► EmbeddingProvider (trait)
  sem rede        sem rede         ├── LocalProvider   em processo, sem rede
                                   └── RemoteProvider  cliente do worker
                                            │ AF_UNIX, já permitido pelo gate
                                            ▼
                                     noteit-embed  ← processo separado, o único
                                            │        com cliente HTTP e o único
                                            ▼        que vê a credencial
                                     api do provider
```

O worker separado não está aí por elegância — a 4.1R1.1 e a 4.2B ensinaram a
desconfiar disso. Ele é a única forma de ter provider remoto sem (1) pôr
`reqwest`/`hyper` no grafo do `noteit-mcp`, o que reprova o gate; (2) pôr a
credencial no processo que fala com o agente; (3) dar ao MCP a capacidade
genérica de fazer HTTP. O canal é o mesmo padrão AF_UNIX da autoridade de
escrita, que o `check-mcp-boundary` já permite **por nome**, distinguindo família
de endereço de "socket" (ADR-047). O worker só existe quando um provider remoto
está configurado: no modo local não é iniciado. A subfase que o implementar
**estende** o gate — `noteit-embed` é o único lugar onde uma crate HTTP é
permitida, e ele não ganha acesso ao store.

**Credential boundary.** Uma chave nunca está em nota, front matter, índice,
embedding, log, resposta MCP, stdout, stderr sem redação, Git ou documentação
gerada. Configuração não secreta (provider, modelo, dimensão, modo, fallback) vai
onde a configuração já vai; credencial não. Ordem proposta: variável de ambiente
do processo `noteit-embed`, depois Secret Service/keyring, e arquivo com
permissão restrita como último recurso e dito como tal. Nenhum armazenamento
inseguro "só para o protótipo". O cliente MCP não precisa saber qual credencial o
Note-it usa, e não há tool que a devolva. Erros de provider são tipados
(`Unavailable`, `Authentication`, `RateLimited`, `InvalidResponse`,
`ModelUnavailable`, `DimensionMismatch`) com mensagem pública escolhida pelo
Note-it — nunca `format!("{external_error}")` no fio. É a lição da 4.2R.R1
aplicada antes do defeito existir: **o fornecedor não escreve a mensagem pública
do Note-it**.

**EmbeddingSpaceId.** Responde uma pergunta só: estes dois vetores podem ser
comparados? `{provider, model, artifact_identity, dimension, embedding_recipe,
normalization}`, e só entram na mesma busca se forem **iguais** — não
"compatíveis o suficiente".

**O papel da entrada não faz parte do espaço** (corrigido na 4.3A.R1). A primeira
redação punha `task = document/query` dentro do `EmbeddingSpaceId` e ao mesmo
tempo exigia igualdade exata para comparar, o que se contradizia: uma busca
compara o vetor de uma consulta com vetores de documentos, e sob aquela regra
nenhuma busca seria válida. São três conceitos e não dois — o **espaço** onde os
vetores são comparáveis, o **papel** (`EmbeddingRole::Document | Query`), e a
**receita** de preparação de cada papel, versionada **em par**, porque mudar só a
receita de consulta invalida a comparação tanto quanto mudar a de documento. Um
provider declara o mesmo `EmbeddingSpaceId` em `embed_document` e `embed_query`
sempre que seus vetores forem comparáveis; um que não possa não é provider deste
sistema.

**Identidade do artefato e da receita.** O nome do modelo não basta: a classe de
defeito medida — números válidos, ranking inválido, nenhum erro estrutural —
reaparece inteira se pesos, tokenizer, normalização ou receita mudarem mantendo
nome e dimensão. No provider local a identidade é verificável e obrigatória: o `sha256` de um
**manifesto** — `ArtifactManifestV1 {weights_sha256, tokenizer_sha256,
embedding_recipe_version, normalization_version}`, codificado como JSON canônico
sob o separador de domínio `noteit.artifact.v1` —, calculado na carga sobre os
bytes efetivamente carregados e gravado no cabeçalho do cache. O separador dá
separação **semântica**: a mesma cadeia de bytes não pode ser lida como
identidade de outro domínio ou de outra versão do formato. Ele não promete
ausência de colisão, que continua sendo propriedade do SHA-256 e não de um
prefixo. O manifesto, e não
uma concatenação (endurecido na 4.3A.R1.1): concatenar componentes de comprimento
variável é ambíguo, e duas decomposições diferentes podem produzir a mesma cadeia
de bytes e portanto a mesma identidade para artefatos distintos — exatamente a
classe de defeito que esta ADR existe para fechar. Campos nomeados, comprimentos
fixos, ordem fixa e separador versionado resolvem por construção. No remoto, quando
o provider publica versão ou snapshot imutável, o identificador entra no espaço e
a garantia é forte; quando só oferece alias mutável, **não há como detectar** que
o modelo mudou do outro lado, e a resposta honesta é marcar o espaço como **não
verificável**, expor isso ao usuário, manter reconstrução manual sempre
disponível e preferir o identificador versionado na configuração padrão. Nenhuma
heurística de detecção é proposta: comparar amostras contra vetores guardados
seria inventar um teste estatístico cujo falso negativo é o caso perigoso.

Dimensão igual **não** é compatibilidade, e isso foi medido em vez de suposto.
Truncando os vetores de um modelo para a dimensão de outro, o que produz números
perfeitamente calculáveis:

```text
mesmo espaço      R@1 0,700   R@3 0,933   R@5 0,967   MRR 0,812
espaços cruzados  R@1 0,033   R@3 0,133   R@5 0,133   MRR 0,094
```

O ranking colapsa e nada no cálculo avisa. Mesmo provider com modelo novo é
espaço novo até prova explícita em contrário.

**Note-vector provenance.** `EmbeddingRecord {note_id, source_revision, chunk_id,
chunker_version, space, vector}`. `source_revision` é a **revisão canônica que o
Core já calcula** — não se inventa um segundo detector de estado, e a 4.2A.R1 já
registrou o custo de ter dois. `chunk_id` é `note_id + revision + ordinal + hash
do texto do chunk`, para que notas diferentes de texto igual não colidam.

E a regra que sustenta a Fase 4.2: **`source_revision` é chave de cache e mais
nada**. Nunca é publicada num candidato, nunca chega ao agente, nunca autoriza
escrita. O atalho `embedding → revision → write` é proibido; a cadeia continua
sendo descobrir → `noteit_read` → revisão → decidir → `expected_revision`.

**Staleness.** Medido. Uma nota indexada com o texto A é editada para B; o índice
ainda tem o vetor de A. Consultando o assunto de A: sem validação de proveniência
o candidato obsoleto vem em primeiro (`sim=0,5954`); comparando `source_revision`
com a revisão atual ele desaparece.

**A validação exige a leitura da nota, e a 4.3A errou ao dizer o contrário**
(corrigido na 4.3A.R1, depois de conferir a implementação em vez de supor). As
únicas formas de obter uma `NoteRevision` no crate são
`NoteRevision::for_document(&NoteDocument)`, que faz `sha256` do documento
canônico serializado inteiro, e `NoteRevision::parse`, que não calcula nada;
`write::revision_of` é invólucro da primeira. A varredura
`list_notes_by_recency_with_warnings` lê o front matter só para o horário de
edição e cai no `mtime`, e `NoteSummary` **não tem campo `revision`**. Não existe
caminho autoritativo para a revisão atual sem carregar o `NoteDocument`, e esta
ADR recusa-se a criar um: uma segunda definição de estado é o defeito que a
4.2A.R1 registrou. `updated_at` tampouco serve — ele move com o texto e fica
parado quando muda uma tag, uma propriedade ou uma cor, enquanto a revisão move.

O custo real, porém, **é zero em I/O**, e a afirmação corrigida é melhor que a
original: o Context Engine já faz exatamente uma leitura autoritativa por
candidato, porque é disso que a coerência depende (D-27), e a validação pega
carona nela.

A ordem oficial é uma só, e é esta:

```text
índice → candidato preliminar (note_id + canal)
       → UMA leitura autoritativa do NoteDocument
       → NoteRevision::for_document(&documento)
       → source_revision == revisão atual ?
            não → descartar e marcar para reindexação
            sim → snippet, motivos e tarefas DESSA MESMA leitura
```

Ler primeiro, validar depois: não há o que comparar antes de carregar o
documento. **É a leitura que produz o snippet publicado, nunca o cache**. Um registro obsoleto é descartado da
resposta e agendado para reindexação. Daí também a decisão de o índice guardar
apenas vetor e metadados, e nenhum texto: guardar texto pouparia uma leitura que
o motor já faz por candidato (D-27) e compraria um segundo lugar onde conteúdo de
nota vive em disco e uma segunda maneira de publicar texto velho.

**Provider/model switching e multi-index.** Trocar de provider não pode comparar
vetores de espaços diferentes — a medição acima diz o que acontece se comparar.
Recomendado **um índice ativo por vez em v1**, com o diretório nomeado pelo
`EmbeddingSpaceId`, de modo que guardar mais de um seja mudança de política de
limpeza e não de formato. No modo local, rebuild custa 7 s para 10 000 notas e
não justifica guardar espaços mortos; no modo remoto custa dinheiro, e é ali que
guardar o espaço anterior compensa — decisão da subfase que implementar o remoto,
com número na mão.

**Reindexing.** Incremental por nota quando a revisão de uma nota muda; global
quando muda o chunker, o provider, o modelo, a dimensão, a task ou a versão do
formato. Uma edição numa nota não pode recalcular dez mil. Indexar é **leitura**:
não move conteúdo, front matter, `updated_at`, `created_at`, `revision` nem
`mtime` — a Fase 3.4R levou uma fase inteira para que abrir uma nota não movesse
`updated_at`, e isto não desfaz aquilo.

**Persistência, e por que a resposta difere por modo.** No local, o custo de
reindexar é CPU ociosa e persistir não se justifica em v1. No remoto, cada
reindexação custa tokens pagos e latência, então persistir vale desde a primeira
nota — **o usuário não pode pagar para embedar tudo a cada busca**. A mesma
pergunta tem respostas diferentes, e a especificação não finge que tem uma só.
Quando houver cache: em `$XDG_CACHE_HOME/note-it/`, nunca em `notes/`, com
cabeçalho que se autoidentifica (formato, `EmbeddingSpaceId` inteiro, versão do
chunker), permissões restritas, validação na carga, e ponto de commit por
renomeação atômica — construir em temporário, validar, publicar. Queda antes
deixa o índice anterior válido; queda depois deixa o novo reconhecível; nunca
meia-indexação que pareça completa. Incompatibilidade → **reconstruir**, jamais
reinterpretar.

**Privacy disclosure.** Sem telemetria, sem analytics, sem upload que não seja a
geração de embedding do provider escolhido, para o endpoint daquele provider. O
usuário deve poder ver provider, modelo, local/remoto, última indexação e estado
do índice. Um vetor é dado derivado de nota privada e não é "não sensível" por
não ser texto: permissões restritas e nunca publicado.

**LLM independence.** O Note-it não está construindo uma IA local, nem um cliente
da OpenAI, nem um cliente do Gemini. Está construindo uma memória semântica
independente de fornecedor, cuja fonte da verdade são as notas e cuja recuperação
usa o mecanismo que o usuário escolher. Claude, ChatGPT, Gemini ou qualquer host
MCP futuro usam o mesmo Segundo Cérebro, com qualquer provider de embeddings.

**Vendor lock-in.** Trocar de provider não migra nota: no máximo reindexa.
Nenhum metadado de provider entra na nota — provider pertence à configuração e ao
cache, e o Markdown continua portátil e sem saber que embeddings existem. Os
serviços gerenciados de vetores dos fornecedores **não** são adotados por
conveniência: mudariam a arquitetura de "o Note-it é dono do seu índice" para "o
fornecedor é dono do estado da recuperação". Seriam decisão separada e de alto
impacto; a preferência desta fase é explícita e contrária.

**Embedding não valida fato.** Embedding mede proximidade representacional. Score
alto não torna um texto verdadeiro; score baixo não o torna falso. Ele decide *o
que talvez valha a pena ler*, não *o que é verdade*. Daí a forma do fluxo: o
agente recebe **texto da nota atual** e nunca vetores, e é a leitura do estado
atual que vira evidência. Uma arquitetura em que o LLM recebe números não tem como
ser verificada por ninguém.

**Providers remotos, verificados em 2026-09-04 nas fontes oficiais e não
medidos.** OpenAI `text-embedding-3-small` 1536 dim reduzíveis, 8 192 tokens,
US$ 0,02/1M; `-3-large` 3072, US$ 0,13/1M; `ada-002` 1536 fixa, US$ 0,10/1M;
array por requisição e Batch a −50%. Gemini `gemini-embedding-2` 128–3072, 8 192
tokens, US$ 0,20/1M texto; `gemini-embedding-001` 128–3072, 2 048 tokens,
US$ 0,15/1M, com tipos de tarefa por parâmetro; mais de 100 idiomas, Batch a
−50%, faixa gratuita. Voyage série 4 com 1024 padrão e 256/512/2048 por
Matryoshka, contexto de 32 000, `input_type` query/document, lote de até 1 000
textos, quantização na resposta, 200 milhões de tokens grátis por conta;
`voyage-4-lite` US$ 0,02/1M, `voyage-4` US$ 0,06/1M, `voyage-4-large`
US$ 0,12/1M. **Nenhum foi medido**: sem credencial nesta sessão, e documentação
de fornecedor não é benchmark interno. `voyage-4-nano` tem pesos abertos sob
Apache-2.0 e é o único candidato que poderia um dia ser provider local e remoto
no mesmo espaço vetorial — não avaliado.

**Padrão recomendado, em três níveis inequívocos** (desambiguado na 4.3A.R1, que
encontrou "LOCAL — o padrão" numa seção e "DEFAULT lexical" em outra). O padrão de
fábrica é `mode: lexical_only`: BM25 e mais nada, sem download, sem modelo, sem
credencial, sem rede, e é o estado de uma instalação nova **e** de uma instalação
atualizada que nunca foi configurada. `mode: semantic` é ato do usuário, e dentro
dele `provider: local` é o padrão — quem liga a semântica sem dizer mais nada
recebe a local. Um provider remoto nunca é alcançado sem ser nomeado. **Nenhuma
leitura desta configuração permite que uma atualização passe a enviar conteúdo ou
a baixar modelo**; se o padrão de fábrica um dia mudar, isso é decisão de produto
com ADR própria e não efeito colateral de release.

`PRIVACIDADE/OFFLINE` local estático, quando o usuário ligar.
`MELHOR QUALIDADE` por medir, provavelmente remoto e **sem evidência ainda**.
`REMOTO OPCIONAL` sempre opt-in. Nenhuma chave é requisito do primeiro uso, e nem
o modelo local é: o padrão de fábrica leva R@3 de 0,367 a 0,767 e não baixa nada.
Benchmark não é lock-in — mesmo que o local ganhe, os remotos continuam; mesmo
que um remoto ganhe em qualidade, o local continua.

**O motivo nomeia o canal, não o texto.** `Reason::SemanticMatch` significa
"admitido pelo canal semântico" e nada além disso. A primeira redação dizia "sem
palavra em comum", o que estava errado duas vezes: é falso, porque uma nota pode
casar por semelhança *e* compartilhar termos; e transforma um motivo — que é um
fato sobre o que o servidor fez — numa afirmação sobre o texto, que o servidor
teria de calcular e provar. Todo motivo do catálogo é do mesmo tipo, o canal que
admitiu o candidato, e um candidato carrega todos os que se aplicam. O que um
agente ganha continua real: `SemanticMatch` **sozinho**, sem `TextMatch` nem
`TermMatch` ao lado, diz que nenhuma palavra da consulta foi encontrada — e ele
lê isso da ausência dos outros motivos, não de uma promessa embutida neste.

**Metodologia do BM25, para não medir o próprio ajuste.** `k1 = 1.2` e
`b = 0.75`, os valores canônicos, **congelados antes** da medição final — em vez
de mandar a subfase seguinte "confirmá-los contra o corpus", que era o que a
primeira redação dizia e que teria sido ajustar num conjunto e apresentar a
métrica desse mesmo conjunto como validação. Com 32 consultas o ajuste caberia
inteiro dentro do ruído. O corpus é **régua de regressão e não conjunto de
validação independente**: responde "esta mudança piorou algo que funcionava?", que
é a pergunta para a qual foi construído, e não responde "este motor é bom".
Ajustar qualquer peso exige antes um conjunto de tuning separado e um de
avaliação que não seja usado no ajuste — e o corpus de hoje é pequeno demais para
ser partido em dois de forma útil, o que significa escrever mais casos primeiro.

**Questões deixadas para a implementação.** Qualidade sob quantização int8 dos
modelos estáticos; verificação da licença de `model2vec-rs`, que o `crates.io`
publica como `non-standard` enquanto o card do modelo diz MIT; RSS real em Rust,
já que os números desta fase vêm de um processo Python e não são representativos;
e um corpus maior, já que 32 consultas separam arquiteturas com folga e não
separam dois modelos parecidos.

**`k1` e `b` não são uma dessas questões, e deixaram de ser listadas como tal.**
Estão congelados em 1.2 e 0.75, e a implementação os usa exatamente assim.
Reabri-los exige um conjunto de tuning novo, um conjunto de avaliação separado que
não seja usado no ajuste, e a decisão explícita de reabrir — nesta ordem. O corpus
desta fase é régua de regressão e não serve para nenhum dos dois primeiros.

## ADR-057: O motor tem três canais e uma única autoridade, e a classe é que protege o acerto exato

**Contexto.** A 4.3A mediu, a 4.3A.R1 corrigiu o contrato, a R1.1 fechou a
proveniência e a R1.2 congelou a política de admissão e ranking. A 4.3B é a
primeira das quatro que escreve Rust: casamento por termo com BM25 no Context
Engine real, mais a infraestrutura semântica **sem nenhum provider**.

**Problema.** Três coisas podiam dar errado ao materializar aquele contrato, e
duas delas são silenciosas.

A primeira é ter dois motores. Um `semantic_context.rs` ao lado do
`context.rs` seria um segundo lugar que lê o store, aplica filtros, monta
snippet, conta tarefas, aplica tetos e ordena — e dois desses discordam na
primeira semana. **A 4.3B evoluiu `noteit-core/src/context.rs`.** Há uma
autoridade de recuperação contextual, e os canais novos são camadas dentro dela.

A segunda é o acerto exato rebaixado por um número. É o defeito que a 4.3A mediu
na Reciprocal Rank Fusion: melhor R@3, e um acerto exato empurrado para baixo.

A terceira é o vetor obsoleto — um candidato publicado a partir de uma revisão
que não existe mais.

**Decisão 1: a classe é explícita, e a contagem que ordena a classe 1 não
enxerga os canais novos.** Cada candidato recebe uma `CandidateClass` — quatro
valores, atribuída a partir dos sinais e não de `reasons.len()` nem do ordinal
de uma variante. A resposta é a concatenação das classes.

O detalhe que faz a garantia ser real está uma camada abaixo. A classe 1 ordena
por **quantos sinais declarados** admitiram o candidato, e `TermMatch` e
`SemanticMatch` não são sinais declarados. Se a ordenação contasse o comprimento
da lista de motivos, acrescentar BM25 teria reembaralhado candidatos que já
existiam — silenciosamente, e em todo store. Com a contagem cega para as classes
2 e 3, elas são **estritamente aditivas**: acrescentam candidatos abaixo de tudo
o que havia e não movem nada.

Medido: as 32 consultas do corpus, uma a uma, nenhuma respondeu pior.

```text
                       R@1     R@3     R@5     MRR
baseline (pré-BM25)   0,333   0,367   0,367   0,350
4.3B em Rust          0,633   0,767   0,833   0,711
protótipo 4.3A        0,667   0,767   0,833   0,728
```

**A diferença para o protótipo é uma consulta, e ela é a garantia funcionando.**
`q08 "sono"`: `n25` casa a frase inteira e fica em primeiro; o ground truth vem
em segundo — exatamente onde já estava no baseline. Um BM25 puro teria posto o
ground truth em primeiro e rebaixado `n25`. O protótipo podia; o motor não pode,
e não é para poder. R@3, R@5 e todo o resto reproduzem o protótipo.

**Decisão 2: os parâmetros não se mexem, e o corpus não é conjunto de
validação.** `k1 = 1.2`, `b = 0.75`, escritos como constantes e usados como
foram congelados. Nenhuma busca em grade, nenhum "1,3 melhora o número". A
fórmula está fixada em teste unitário contra a aritmética escrita à mão, não
contra uma fixture.

**Decisão 3: uma dobra, e nenhuma segunda definição de palavra.** Os termos são
as sequências maximais de `[0-9a-z]` sobre o texto que `search::fold` já
produz — a mesma dobra da busca global desde muito antes de a recuperação ter
fase própria. Sem stemming, sem lematização, sem stopwords, sem sinônimos, sem
tabela por idioma: cada um deles é uma afirmação sobre a língua que precisaria da
sua própria evidência, e nenhum foi medido.

**O custo disso, medido e não escondido.** Sem stopwords, uma consulta cujo único
termo comum é `de` admite toda nota que tem `de`. No corpus, as duas consultas
sem resposta passaram de 0 para 22 e 13 candidatos, **inteiramente** por causa de
`de` e `do` — os outros termos (`receita`, `bolo`, `fuba`, `configuracao`,
`roteador`, `wifi`) somam uma nota entre todos. Eles pontuam quase nada e ficam
no fim, mas são admitidos, porque admissão é booleana e ranking é numérico.
Registrado como o que é: o preço medido de admitir por termo. Mudar isso — um
piso de IDF na admissão, ou uma lista de palavras funcionais — é decisão de outra
fase, com evidência, e não um ajuste feito de passagem para o número do corpus
ficar mais bonito.

**Decisão 4: o padrão lexical não depende de configuração.** `RetrievalMode` tem
duas variantes e a primeira, `LexicalOnly`, **não tem campo onde um provider
caiba**. Não é "o provider não foi configurado" nem "a flag está desligada": não
há o que ligar por acidente, por arquivo faltando ou por inicialização que
falhou. Um provider de teste que entra em pânico se for chamado prova o caminho
de produção com zero chamadas.

**Decisão 5: comparabilidade é identidade, nunca forma.** `EmbeddingSpaceId`
carrega provider, modelo, identidade do artefato, dimensão, versão da receita do
par e versão da normalização, e a igualdade é de todos. A regressão é a medição
da 4.3A: dois espaços truncados para a mesma dimensão dão cosseno perfeitamente
calculável e R@3 de 0,133 contra 0,933. O papel — documento ou consulta — fica
**fora** do espaço, porque uma busca é justamente a comparação entre os dois.

A identidade do artefato é `sha256("noteit.artifact.v1\n" ‖ codificação canônica
do manifesto)`. Uma autoridade de codificação, não um `format!` espalhado: a
concatenação de componentes de comprimento variável é ambígua, e o encoder aqui
**recusa** qualquer valor fora de um alfabeto estreito em vez de escapá-lo — um
encoder canônico cuja correção depende de uma rotina de escape é um encoder
canônico com um bug esperando. O separador de domínio dá separação semântica
entre formatos e versões; ele **não** promete ausência de colisão, que continua
sendo propriedade do SHA-256.

**Decisão 6: proveniência é a revisão canônica, e o snippet nasce da leitura.**
Um registro do índice é preliminar. O motor lê o `NoteDocument` — a leitura que
D-27 já obriga —, calcula `NoteRevision::for_document` e compara. Diferente:
descarta e **esquece** o registro. Igual: o snippet, os motivos e as tarefas saem
dessa leitura, nunca do cache. O índice não guarda texto de nota, então não há
segundo lugar de onde publicar conteúdo velho.

O teste que fecha isso é o da mudança **só de metadado**: mexer numa tag deixa
`updated_at` parado — foi verificado no teste, não suposto — e move a revisão. É
a prova de que ninguém voltou a usar `updated_at` como token de versão, que é o
defeito de onde a R1-002 saiu.

**O que a 4.3B deliberadamente não fez.** Nenhum provider real, nenhum modelo,
nenhum byte de peso, nenhuma rede, nenhuma credencial, nenhuma dependência nova,
`Cargo.lock` byte-idêntico, catálogo ainda em 16 tools, `semantic_match` no
esquema e inalcançável pelo produto. Fundir com a 4.3C teria sido um commit a
menos e teria custado a única coisa que importa depois: quando a recuperação
semântica responder mal, distinguir bug do motor de bug do modelo. A separação
vale o commit.
