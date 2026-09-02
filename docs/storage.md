# Armazenamento e diretórios XDG

Note-it adere à especificação de diretório base XDG:

| Caminho | Propósito | Exemplo de substituto |
| --- | --- | --- |
| `$XDG_DATA_HOME/note-it/notes/` | Arquivos de notas Markdown persistentes (`<uuid>.md`) | `~/.local/share/note-it/notes/` |
| `$XDG_DATA_HOME/note-it/trash/` | Notas excluídas, aguardando para serem restauradas | `~/.local/share/note-it/trash/` |
| `$XDG_DATA_HOME/note-it/assets/` | Imagens que as notas contêm, um diretório por nota | `~/.local/share/note-it/assets/` |
| `$XDG_DATA_HOME/note-it/backups/` | Instantâneos locais do armazenamento recuperável | `~/.local/share/note-it/backups/` |
| `$XDG_DATA_HOME/note-it/study.json` | Cronogramas versionados e atividades de estudo agregadas | `~/.local/share/note-it/study.json` |
| `$XDG_CONFIG_HOME/note-it/config.toml` | Opções de configuração do usuário | `~/.config/note-it/config.toml` |
| `$XDG_STATE_HOME/note-it/state.json` | Geometria da janela, modo ativo e estado transitório UI | `~/.local/state/note-it/state.json` |
| `$XDG_RUNTIME_DIR/note-it/<store>/` | Lease de escrita e soquete de controle para um store | `/run/user/<uid>/note-it/<store>/` |

`study.json` contém apenas chaves de revisão SHA-256 opacas, níveis, carimbos de data/hora absolutos UTC, classificações e contadores diários codificados por data civil local. Perguntas, respostas, Markdown, títulos, HTML, bytes de imagem e caminhos absolutos nunca entram nele. Ausente significa uma história vazia; dados corrompidos ou mais recentes são deixados byte por byte e tornam o Estudo indisponível em vez de serem substituídos. Cada classificação cria um próximo estado e o confirma com a mesma primitiva de gravação atômica das notas antes de o aplicativo adotá-lo.

## Coordenação de gravação em runtime

Exatamente um processo Note-it pode gravar em um store por vez. A exclusividade é garantida por um `flock` consultivo sobre um arquivo de bloqueio no diretório de runtime, nunca pela mera existência do arquivo: se um processo falha, o bloqueio é liberado assim que o kernel fecha seus descritores, e um arquivo deixado por um processo morto não bloqueia ninguém.

```text
$XDG_RUNTIME_DIR/note-it/            0700
  <store key>/                       0700
    store                            0600   diretório de notas representado pela chave
    writer.lock                      0600   lease
    control.sock                     0600   soquete privado da autoridade
```

`<store key>` é o digest FNV-1a de 64 bits do caminho do diretório de notas, escrito como dezesseis caracteres hexadecimais minúsculos. A chave por store permite que um store de teste isolado e o store real tenham, ao mesmo tempo, um gravador legítimo cada um, sem que um espere pelo outro.

Nada aqui pertence ao store. Ele descreve esta inicialização, não tem sentido após uma reinicialização e nunca é feito backup. Quando a sessão não tem `$XDG_RUNTIME_DIR`, o substituto é `/tmp/note-it-<uid>`, com escopo definido para o usuário em vez de um nome que qualquer um poderia usar primeiro - e de qualquer forma, ambos os diretórios são recusados ​​se forem um link simbólico, pertencerem a outro usuário ou forem acessíveis por um.

A instância de desktop adquire o lease antes de poder salvar qualquer coisa e o mantém até o processo terminar. `noteit` o adquire durante um comando quando está livre; quando o lease está retido, envia a alteração ao proprietário por `control.sock`; quando está retido e o proprietário está inacessível, não altera nada e informa o problema. Consulte o ADR-038.

## Campos de aparência de nota

| Campo | Significado | Padrão quando ausente |
| --- | --- | --- |
| `color` | Cor do papel: `yellow`, `blue`, `green`, `pink`, `purple`, `gray`, `black` | `yellow` |
| `paper_type` | Padrão de fundo: `blank`, `lined`, `dotted`, `grid-small`, `grid-large` | `blank` |
| `paper_intensity` | Quão fortemente esse padrão é desenhado: `subtle`, `normal`, `strong` | `normal` |
| `font_size` | Tamanho base do texto da nota | `15` |

Eles descrevem como a nota é exibida, de modo que ficam no front matter ao lado da nota, e não em `state.json`, e acompanham o arquivo. Alterar qualquer um deles salva a nota sem tocar em seu conteúdo ou em seu `updated_at` e - como um salvamento de conteúdo - a alteração é adotada na memória apenas depois de escrita, de modo que aquela que falha não é deixada para trás como se tivesse sido armazenado.

Cada um é armazenado como uma string simples e resolvido em relação ao conjunto suportado na leitura, portanto, um valor escrito por uma versão mais recente — ou manualmente — é degradado para o padrão em vez de falhar na análise e levar a anotação com ele. Uma nota escrita antes da existência desses campos abre como papel comum com intensidade normal e ganha os campos na próxima vez que for salva.

`paper_intensity` é mantido par para `blank`, onde não há padrão para agir, portanto, alternar o papel para frente e para trás nunca perde a escolha.

## Configuração do aplicativo

`config.toml` mantém preferências compartilhadas por cada nota:

| Campo | Significado | Padrão |
| --- | --- | --- |
| `default_color` | Cor do papel dada a uma nova nota | `yellow` |
| `default_font_size` | Tamanho base do texto atribuído a uma nova nota | `15` |
| `default_width`, `default_height` | Tamanho dado a uma nova nota | `360`, `300` |
| `autosave_interval_ms` | Debounce antes que uma edição seja escrita | `300` |
| `theme` | Tema da interface: `system`, `light`, `dark` | `system` |

O tema é a aparência do chrome do aplicativo – menus, popovers, bordas, estados de foco – e deliberadamente **não** por nota: uma nota mantém a cor e o papel que recebeu, seja qual for o tema. `system` segue o esquema de cores da área de trabalho e continua seguindo-o enquanto o aplicativo é executado.

## Carimbos de data e hora no front matter da nota

`created_at` registra quando a nota foi criada e nunca muda depois. `updated_at` registra a última alteração no **conteúdo** da nota.

Conteúdo significa o Markdown que é persistido. Se esse texto for diferente do que já está armazenado, a alteração é registrada - seja por digitação, título, lista, tarefa, negrito, itálico, tachado, cor do texto, realce ou tamanho embutido, já que tudo isso está escrito na nota.

Todo o resto deixa `updated_at` deliberadamente em paz:

- aparência: cor do papel, tipo de papel, intensidade do padrão, tamanho da fonte;
- o tema da interface, que não é armazenado na nota;
- janela e estado de visualização: arrastar, redimensionar, ampliar, recolher/expandir, modo de camada;
- abrindo o menu ou passando o mouse sobre o cabeçalho;
- **e visitando a nota.** Abrir e fechar, convocar, ocultar, mostrar ou sair sem editar, tudo deixa-a intacta.

Este último ponto é aplicado e não assumido. Fechar e liberar envia tudo o que o editor contém, editado ou não, de modo que o caminho único pelo qual todo o conteúdo salva o funil compara o texto recebido com o que já está armazenado e não faz nada quando eles correspondem. Uma nota inalterada não é reescrita: nenhum arquivo temporário, nenhuma renomeação, nenhum fsync e o arquivo mantém seu próprio horário de modificação.

Essa comparação é apenas sólida enquanto a nota mantida na memória é a nota que está no disco, por isso é mantida dessa forma: uma alteração é preparada em uma cópia, escrita e adotada na memória somente quando a gravação for bem-sucedida. Um salvamento que falha, portanto, deixa a nota descrevendo exatamente o que está armazenado, e o mesmo texto chegando novamente - que é o que cada um desses caminhos reenvia - ainda é uma diferença e é escrito de verdade. Uma carga útil nunca é tratada como armazenada porque corresponde a um estado que veio de uma gravação que nunca ocorreu, e salvar e fechar nunca finaliza um fechamento sobre um salvamento que falhou.

Ambos os campos são opcionais na leitura. Uma nota cujo front matter os omite ainda será aberta; o valor ausente é relatado como desconhecido (`—`) em vez de substituído por uma data fabricada, e salvar novamente a nota também não inventa nenhuma.

## Qual nota uma convocação traz de volta

Quando cada nota é fechada, o aplicativo reabre a última escrita, ordenada pelo próprio `updated_at` de cada nota — o campo front matter que registra a última alteração em seu **texto**. Fechar uma nota que você não digitou não a move para frente, porque uma nota inalterada nunca é reescrita; nem alterar sua cor, papel, intensidade do padrão ou tamanho da fonte, o que reescreve o arquivo, mas não é uma edição. Uma nota sem `updated_at` legível - uma escrita antes da existência do campo, uma sem front matter, uma cujo cabeçalho não pode ser analisado - volta para o próprio `mtime` do arquivo, que é o que cada nota usava antes de haver um campo para ler. Os empates são desfeitos por identificador, portanto a mesmo store lista sempre na mesma ordem.

A mesma ordem é o que a pesquisa e o alternador rápido mostram, portanto, "mais recente" significa uma coisa em todo o aplicativo. Ler custa uma leitura limitada do cabeçalho de cada nota; nada está escrito e um cabeçalho ilegível custa registrar seu carimbo de data e hora em vez de falhar na listagem.

Esta é a leitura pretendida de "a nota usada por último" - a nota realmente escrita. A reabertura, a convocação e o envio de instância única não são afetados. Uma necessidade futura de "a nota que abri pela última vez", como algo distinto de "a nota que escrevi pela última vez", pertence a `state.json` como um estado explícito, e não a um carimbo de data / hora do sistema de arquivos.

## Campos de estado da janela

`state.json` armazena uma entrada por nota:

| Campo | Significado |
| --- | --- |
| `x`, `y` | Posição da nota em seu monitor |
| `width`, `height` | Tamanho atual da superfície; enquanto recolhido, `height` é a altura da barra de cabeçalho |
| `is_open` | Se a nota é restaurada na inicialização |
| `monitor` | Nome do conector ao qual a nota pertence |
| `collapsed` | Se a nota é reduzida à barra de cabeçalho |
| `expanded_width`, `expanded_height` | Tamanho para restaurar ao expandir; apenas significativo enquanto `collapsed` |
| `zoom_percent` | Ver escala do conteúdo da nota, 75–300, padrão 100 |

Cada campo tem um padrão, portanto, um `state.json` escrito por uma versão anterior é carregado inalterado: ausente `collapsed` significa expandido, e ausência de geometria expandida volta ao tamanho de nota padrão.

## Formatação embutida em Markdown

Markdown não possui sintaxe para cor, realce ou tamanho de fonte, portanto, eles são armazenados como um pequeno conjunto de elementos HTML controlados. Somente os atributos próprios de Note-it são aceitos, e apenas com valores da lista de permissões correspondente — qualquer outra coisa é descartada quando a nota é carregada.

| Formatação | Representação | Valores aceitos |
| --- | --- | --- |
| Cor do texto | `<span data-note-it-color="#2563EB">` | `#rgb` / `#rrggbb` |
| Destaque | `<mark data-note-it-highlight="#FDE68A">` | `#rgb` / `#rrggbb` |
| Tamanho do texto | `<span data-note-it-font-size="22">` | 12, 14, 16, 18, 22, 26, 32 |
| Conclusão da tarefa | `- [x] texto <!-- note-it:completed_at=… -->` | ISO 8601 com deslocamento ou `Z` |

Nenhum deles é visível como marcação no editor. O comentário dos metadados da tarefa é o único comentário HTML que o sanitizador preserva; todos os outros comentários ainda serão removidos.

## Lixeira

Excluir uma nota move seu arquivo para fora do armazenamento ativo:

```text
notes/<uuid>.md   →   trash/<uuid>.md
                      trash/<uuid>.json   (quando foi excluída)
```

Uma nota em `trash/` não é uma nota. Ele não está listado, não é pesquisado, não é oferecido pelo switcher rápido, não é restaurado na inicialização e não é trazido de volta por uma convocação - não porque cada um deles o exclui, mas porque cada um deles lê `notes/`, e o arquivo não está mais lá.

**A movimentação é o ponto de confirmação.** A sequência é:

```text
flush da nota   →   mover o arquivo   →   atualizar o estado da janela   →   fechar a janela
```

Tudo antes da movimentação pode falhar com a nota ainda aberta, ativa e editável — inclusive, principalmente, um flush que não conseguiu escrever o texto mais recente. Uma nota cujo texto não é seguro nunca desaparece. A partir do movimento, a nota *está* na lixeira, então nada depois informa o contrário: a gravação do estado da janela é o melhor esforço e a janela fecha de qualquer maneira.

**O arquivo não é lido, analisado ou reescrito.** Mover para a lixeira é `rename` e restaurar é `hard_link` mais `remove_file`; ambos preservam o byte da nota por byte, front matter, aparência, tarefas, links e cálculos incluídos. Uma nota cujo front matter está danificado - uma Note-it nem consegue abrir - ainda vai para a lixeira e volta inalterada.

**A restauração nunca substitui uma nota ativa.** A restauração cria o nome em `notes/` com `hard_link`, o que falhará se o nome já existir. Essa é uma propriedade do syscall, não uma verificação que possa ser executada: se uma nota com o mesmo identificador já estiver ativa, nenhum dos arquivos será tocado e o leitor será informado disso.

**Nem é uma edição.** `updated_at` não se move quando uma nota é excluída ou restaurada, portanto, uma nota recuperada retorna à posição no alternador rápido que estava, em vez de fingir que acabou de ser escrita.

**Quando ele foi excluído fica ao lado da nota, nunca dentro dela.** O sidecar `<uuid>.json` contém `deleted_at` e nada mais. Se estiver ausente ou ilegível, a listagem da lixeira retornará ao horário de modificação do próprio arquivo; nada está escrito para repará-lo. Qualquer coisa em `trash/` que não seja `<uuid>.md` é ignorada pela listagem.

**Não há exclusão permanente nem "esvaziar a lixeira".** A lixeira cresce até que você mesmo remova os arquivos dela, o que é uma escolha deliberada para uma fase de recuperação - e possível com qualquer gerenciador de arquivos, porque uma nota na lixeira é um `.md` comum no disco.

## Backups locais

Um snapshot é um diretório de arquivos comuns:

```text
backups/2026-08-29T09-30-00Z/
  manifest.json
  notes/<uuid>.md …
  trash/<uuid>.md, <uuid>.json …
  assets/<note-uuid>/<asset-uuid>.<ext> …
  config.toml
  state.json
  study.json
```

`manifest.json` registra a versão, quando o instantâneo foi tirado, se foi automático ou manual, quantas notas, entradas de lixo e imagens ele contém e se a configuração, o estado da janela e o histórico de estudo estavam presentes. Um diretório em `backups/` conta como um instantâneo somente se for um diretório real, seu nome não começar com `.` e contiver um manifesto legível.

Manifesto **versão 3** é a versão 2 mais o sinalizador opcional de histórico de estudo; a versão 2 é a versão 1 mais a contagem de imagens. Os instantâneos mais antigos permanecem válidos porque ambos os campos posteriores são padronizados como ausente/zero.

**O que entra:** `notes/`, `trash/`, `assets/`, `config.toml`, `state.json` e `study.json` quando existe. Um arquivo de estudo existente é um dado recuperável: se não puder ser copiado como um arquivo normal, o instantâneo não será confirmado como completo.

Uma nota que diz `![](../assets/…)` é apenas meia nota sem o arquivo para o qual a referência aponta, então `assets/` é copiado com as mesmas garantias que as próprias notas: a mesma forma, um diretório por nota, byte por byte, e um instantâneo que não pôde copiar um não é confirmado. Uma imagem que nenhuma nota aponta mais também é copiada – um backup é um instantâneo do armazenamento gerenciado, não uma decisão sobre quais de seus arquivos ainda são desejados.

`assets/` é copiado de forma mais estrita do que `notes/` e deliberadamente. Uma pessoa pode razoavelmente ter colocado algo próprio em `notes/`, então uma estranheza é ignorada com um aviso; `assets/` foi escrito por Note-it e por nada mais, então qualquer coisa que não seja `<note-uuid>/<asset-uuid>.<ext>` significa que o armazenamento não está no estado que Note-it acredita que esteja e o backup falha em vez de omitir silenciosamente o conteúdo gerenciado enquanto relata o sucesso. Uma store criado antes da existência de imagens não tem `assets/`, e essa é um store sem fotos, em vez de um store quebrado.

**O que nunca entra:** `backups/` em si, portanto, um instantâneo nunca pode conter instantâneos; qualquer coisa cujo nome comece com `.`, que é o que impede um `.tmp.…` de um salvamento interrompido fora de um snapshot; qualquer coisa que não seja um arquivo normal; e qualquer coisa alcançada através de um link simbólico, que nunca é seguido — uma entrada criada no armazenamento não pode fazer a cópia de backup `/etc` ou um diretório inicial.

**Quando isso acontecer.** No máximo um instantâneo automático a cada 24 horas, tirado **antes da primeira alteração qualificada** depois que esse período tiver passado — uma nota salva ou uma movimentação para a lixeira. A questão é considerar primeiro: a finalidade de um backup é voltar a ser como as coisas eram, então o momento que vale a pena capturar é aquele antes da edição. Não há cronômetro nem thread; um daemon que ninguém está usando não funciona, e um daemon deixado aberto por dias tira seu instantâneo no momento em que seu proprietário começa a digitar novamente. "Quando foi o último backup" é respondido pelo próprio manifesto do snapshot mais recente, portanto não há nenhum arquivo de contabilidade que possa discordar do disco.

**Backup manual.** *Dados › Fazer backup agora* faz um backup imediatamente, nunca é ignorado e sempre relata sucesso ou falha. Ele satisfaz a regra das 24 horas como qualquer outro instantâneo.

**Atomicidade.** Um instantâneo é criado em `backups/.tmp.<pid>.<n>/` e renomeado para o local inteiro; a renomeação é o ponto de confirmação. Um processo eliminado no meio deixa um diretório `.tmp.…`, que não é um instantâneo – nome errado, sem manifesto – e o próximo backup o remove. Somente os diretórios que carregam esse prefixo são varridos.

**Retenção.** Sete snapshots são mantidos em um pool, independentemente do que os tenha gerado, e a retenção é executada **somente depois que um novo snapshot for confirmado**. Um backup antigo nunca é excluído para dar espaço a um que possa falhar. Um instantâneo que não pode ser removido é relatado e o novo backup ainda permanece.

**Falha.** Um instantâneo que não pode ser feito nunca bloqueia um salvamento: o erro vai para `stderr` e a nota é escrita normalmente, e a tentativa é repetida na próxima alteração elegível, em vez de a cada pressionamento de tecla.

### Recuperando-se de um instantâneo

Deliberadamente, não há "restaurar tudo" com um clique no aplicativo: colocar um instantâneo de volta em um armazenamento ativo é uma transação de vários arquivos, e um botão para isso seria o controle mais destrutivo que Note-it possui. O procedimento manual, com o aplicativo fechado, é:

```bash
note-it quit                       # nenhum processo pode estar em execução

SNAP=~/.local/share/note-it/backups/2026-08-29T09-30-00Z
cat "$SNAP/manifest.json"          # confira se este é o instantâneo desejado

# Preserve o conteúdo atual para que esta etapa também seja reversível.
mv ~/.local/share/note-it/notes  ~/.local/share/note-it/notes.antes
mv ~/.local/share/note-it/trash  ~/.local/share/note-it/trash.antes
mv ~/.local/share/note-it/assets ~/.local/share/note-it/assets.antes
mv ~/.local/share/note-it/study.json ~/.local/share/note-it/study.json.antes  # se existir

cp -a "$SNAP/notes"  ~/.local/share/note-it/notes
cp -a "$SNAP/trash"  ~/.local/share/note-it/trash
cp -a "$SNAP/assets" ~/.local/share/note-it/assets            # se existir
cp -a "$SNAP/config.toml" ~/.config/note-it/config.toml       # se existir
cp -a "$SNAP/state.json"  ~/.local/state/note-it/state.json   # se existir
cp -a "$SNAP/study.json"  ~/.local/share/note-it/study.json   # se existir
```

Para recuperar uma **única** nota, copie apenas esse `<uuid>.md` do diretório `notes/` do instantâneo — e, se contiver imagens, o diretório `assets/<note-uuid>/` correspondente ao lado dele. A nota refere-se às suas imagens por um caminho relativo a `notes/`, então as duas viajam juntas e nenhuma delas precisa de edição.

Que o resultado seja legível não é uma esperança: `a_snapshot_round_trips_into_a_fresh_isolated_store` copia um instantâneo em uma árvore XDG vazia exatamente desta forma, abre-o e verifica se as notas, identificadores, Markdown, lixo, configuração, estado da janela e cronograma de estudo voltaram.

### Contra o que um backup local protege e o que não protege

Ele protege contra: exclusão acidental, corrupção lógica, edição que você deseja desfazer, versão para a qual deseja voltar.

Ele **não** protege contra um disco morto, uma máquina perdida ou roubada ou um sistema de arquivos que falha como um todo — os instantâneos estão no mesmo disco que as notas. Não é criptografado. Qualquer pessoa que precise de proteção contra falhas de hardware precisa de uma cópia em outro hardware, e Note-it não faz uma.

## Gravação atômica de arquivos

Para evitar a corrupção de dados durante perdas inesperadas de energia ou falhas de processo:
1. Grave o conteúdo da nota em um arquivo temporário (`.tmp.<uuid>.<nanos>`) no mesmo diretório.
2. Libere e sincronize dados no disco.
3. Renomeie/substitua atomicamente o arquivo de destino usando `std::fs::rename`.
4. Sincronize o diretório de notas para que a renomeação seja durável.

**A renomeação é o ponto de confirmação.** Ou ela chega e a nota é a nova, ou não e a nota ainda é a anterior; não há estado intermediário e o leitor nunca vê um arquivo rasgado. Se alguma coisa, incluindo a renomeação, falhar, o arquivo temporário será removido em vez de deixado no diretório de notas, já que nada mais o coletaria.

Um salvamento relata falha em qualquer coisa antes ou durante a renomeação e sucesso a partir da renomeação. Essa é a regra da qual depende o documento na memória: ele é substituído apenas por uma versão que realmente foi escrita e é sempre substituído por uma que já foi escrita.

A etapa 4 vem após o ponto de confirmação. Os bytes da nota já estão em armazenamento estável - a etapa 2 os sincroniza - então o que a sincronização de diretório compra é que **renomear** sobrevive a uma perda de energia. Se falhar, o salvamento ainda será bem-sucedido e ainda será relatado como tal; é impresso um aviso, pois o que está em dúvida é a durabilidade e não se a nota foi escrita. Chamar isso de falha no salvamento deixaria o aplicativo descrevendo uma nota que o arquivo não contém mais.

Nada rastreia uma sincronização perdida. A sincronização de um diretório libera todas as entradas pendentes nele, não apenas a última, de modo que o próximo salvamento bem-sucedido de qualquer nota torna a renomeação anterior também durável.

O que isso **não** afirma: a sincronização não é repetida, um salvamento cuja sincronização falhou não tem durabilidade garantida e o arquivo de notas não é sincronizado novamente após a renomeação. A garantia é que uma nota nunca é escrita pela metade e nunca é revertida silenciosamente enquanto o aplicativo está em execução; uma perda de energia dentro dessa janela pode custar o último salvamento, nunca o arquivo.
