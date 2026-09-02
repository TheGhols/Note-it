# Storage e diretórios XDG

Note-it segue rigorosamente a especificação de diretórios base XDG:

| Diretório | Finalidade | Padrão |
| --- | --- | --- |
| `$XDG_DATA_HOME/note-it/notes/` | Arquivos Markdown de notas persistidas | `~/.local/share/note-it/notes/` |
| `$XDG_DATA_HOME/note-it/trash/` | Notas excluídas de forma recuperável | `~/.local/share/note-it/trash/` |
| `$XDG_DATA_HOME/note-it/assets/` | Imagens locais referenciadas por notas | `~/.local/share/note-it/assets/` |
| `$XDG_DATA_HOME/note-it/study.json` | Log e agendamento de repetição espaçada Study Hub | `~/.local/share/note-it/study.json` |
| `$XDG_DATA_HOME/note-it/backups/` | Snapshots locais recuperáveis do store | `~/.local/share/note-it/backups/` |
| `$XDG_CONFIG_HOME/note-it/` | Preferências de configuração da aplicação | `~/.config/note-it/config.toml` |
| `$XDG_STATE_HOME/note-it/` | Estado de janelas, posições e geometrias persistidos | `~/.local/state/note-it/state.json` |

Todas as gravações no sistema de arquivos em `notes/`, `trash/`, `config.toml` e `state.json` são atômicas: os dados são gravados primeiro em um arquivo temporário exclusivo no mesmo diretório, liberados para o disco via `fsync` e renomeados atomicamente sobre o destino final. O salvamento de imagens valida cada arquivo contra o estado do store e o confirma com a mesma primitiva atômica antes que a aplicação o adote.

## Coordenação de gravação em runtime

Exatamente um processo Note-it pode gravar em um store por vez. A posse exclusiva é garantida por um `flock` consultivo em um arquivo de bloqueio no diretório de runtime, nunca pela mera existência de um arquivo: um processo que encerra inesperadamente libera o lock no instante em que o kernel fecha seus descritores de arquivo, e um arquivo de lock residual deixado por um processo encerrado não bloqueia ninguém.

```text
$XDG_RUNTIME_DIR/note-it/            0700
  <store key>/                       0700
    store                            0600   o diretório de notas representado por esta chave
    writer.lock                      0600   o lease de escritor
    control.sock                     0600   o socket privado da autoridade
```

`<store key>` é o digest FNV-1a 64 do caminho do diretório de notas, formatado como dezesseis caracteres hexadecimais minúsculos. Usar uma chave baseada no store permite que um store de teste isolado e o store real do usuário possuam cada um seu próprio escritor legítimo simultaneamente, sem interferências mútuas.

Nenhum arquivo nesta hierarquia de runtime pertence ao store persistido. Eles descrevem apenas a sessão atual, perdem o sentido após uma reinicialização e nunca entram em backups. Quando a sessão não possui `$XDG_RUNTIME_DIR`, o sistema recorre a `/tmp/note-it-<uid>`, com escopo restrito ao UID do usuário em vez de um caminho público previsível — e, em ambos os casos, os diretórios são rejeitados se forem symlinks, se pertencerem a outro usuário ou se puderem ser acessados por terceiros.

A instância desktop adquire o lease antes de poder realizar salvamentos e o mantém até o encerramento do processo. A CLI `noteit` adquire o lease durante a execução de um comando quando ele estiver livre; quando estiver ocupado, encaminha a alteração para o detentor através de `control.sock`; quando estiver ocupado e inacessível, não altera nada e relata o fato de modo seguro (fail-closed). Consulte ADR-038.

## Campos de aparência da nota

| Campo | Significado | Padrão quando ausente |
| --- | --- | --- |
| `color` | Cor do papel: `yellow`, `blue`, `green`, `pink`, `purple`, `gray`, `black` | `yellow` |
| `paper_type` | Padrão de fundo: `blank`, `lined`, `dotted`, `grid-small`, `grid-large` | `blank` |
| `paper_intensity` | Intensidade do padrão desenhado: `subtle`, `normal`, `strong` | `normal` |
| `font_size` | Tamanho base da fonte da nota | `15` |

Estes campos descrevem como a nota é exibida; portanto, residem no front matter junto à própria nota e não no `state.json`, viajando com o arquivo Markdown. Alterar qualquer um deles salva a nota sem modificar seu conteúdo textual nem alterar seu `updated_at`, e — assim como no salvamento de conteúdo — a alteração é adotada na memória somente após ter sido gravada em disco com sucesso.

Cada campo é armazenado como texto simples e validado contra o conjunto suportado durante a leitura; dessa forma, um valor gravado por uma versão futura — ou editado manualmente — degrada de maneira segura para o padrão em vez de falhar no parsing. Uma nota criada antes da introdução desses campos abre como papel liso em intensidade normal e recebe os campos na próxima vez em que for salva.

`paper_intensity` é preservado mesmo quando `paper_type` for `blank`, garantindo que alternar o tipo de papel não descarte a preferência prévia de intensidade.

## Configuração da aplicação

`config.toml` armazena preferências compartilhadas por todas as notas:

| Campo | Significado | Padrão |
| --- | --- | --- |
| `default_color` | Cor padrão de papel para novas notas | `yellow` |
| `default_font_size` | Tamanho de fonte padrão para novas notas | `15` |
| `default_width`, `default_height` | Dimensões padrão para novas notas | `360`, `300` |
| `autosave_interval_ms` | Intervalo de debounce antes de gravar edições | `300` |
| `theme` | Tema da interface: `system`, `light`, `dark` | `system` |

O tema define a aparência do chrome da aplicação — menus, popovers, bordas e estados de foco — e deliberadamente **não** é configurado por nota: cada nota preserva a cor e o papel atribuídos independentemente do tema ativo. O modo `system` segue dinamicamente o esquema de cores do desktop enquanto a aplicação estiver em execução.

## Timestamps no front matter da nota

`created_at` registra o momento de criação da nota e nunca é alterado posteriormente.
`updated_at` registra a última alteração no **conteúdo** da nota.

Conteúdo refere-se ao Markdown persistido. Se o texto diferir do que já está armazenado em disco, a alteração é registrada — tenha ela sido originada por digitação, títulos, listas, tarefas, negrito, itálico, tachado, cores de texto, destaques ou tamanhos inline, pois todos esses elementos são gravados no corpo da nota.

Quaisquer outras operações deliberadamente não alteram `updated_at`:
- alterações de aparência: cor do papel, tipo de papel, intensidade do padrão, tamanho base da fonte;
- tema da interface, que não é armazenado no arquivo da nota;
- estado de janelas e visualização: arrastar, redimensionar, zoom, recolher/expandir, modo de camada;
- abertura de menus ou foco na barra de cabeçalho;
- **e visualização da nota.** Abrir, fechar, invocar (summon), ocultar, exibir ou sair da aplicação sem editar o texto mantém o arquivo intocado.

Este último comportamento é rigorosamente validado. O fechamento de notas e a sincronização de buffer enviam o estado do editor, esteja ele editado ou não; portanto, o caminho único de salvamento de conteúdo compara o texto recebido com o que já está persistido em disco e não executa gravações caso sejam idênticos. Uma nota inalterada não é reescrita: nenhum arquivo temporário é gerado, nenhum rename ocorre, nenhum fsync é disparado e o arquivo preserva seu timestamp de modificação (`mtime`) original.

Essa verificação só é confiável enquanto a nota mantida em memória for idêntica à do disco; portanto, alterações são preparadas sobre cópias temporárias, persistidas e adotadas na memória apenas após a gravação em disco ter sido concluída com sucesso. Um salvamento com falha deixa a nota em memória descrevendo exatamente o que está no disco, e a repetição do mesmo texto continua sendo detectada como alteração pendente para nova tentativa real.

Ambos os campos são opcionais na leitura. Uma nota cujo front matter os omita abre normalmente; valores ausentes são exibidos como desconhecidos (`—`) em vez de substituídos por datas fictícias, e salvar a nota novamente não gera valores inventados.

## Qual nota uma invocação traz de volta

Quando todas as janelas de notas estão fechadas, a aplicação reabre a nota gravada mais recentemente, ordenada pelo campo `updated_at` de seu front matter — o campo que registra a última alteração em seu **texto**. Fechar uma nota na qual nada foi digitado não a coloca no topo da ordenação, pois notas inalteradas não são regravadas; alterar sua cor, papel ou tamanho de fonte regrava o arquivo, mas não constitui edição de conteúdo. Uma nota sem `updated_at` legível recorre ao `mtime` do arquivo no sistema de arquivos. Empates são desfeitos deterministicamente pelo identificador UUID, garantindo que o mesmo store produza sempre a mesma ordem.

Essa mesma ordenação é compartilhada pela busca e pelo seletor rápido (quick switcher), unificando a semântica de "mais recente" em toda a aplicação. A leitura desse dado consome uma leitura limitada do cabeçalho de cada arquivo; nada é gravado em disco, e um cabeçalho corrompido custa à nota apenas seu timestamp, sem interromper a listagem geral.

Essa é a definição pretendida de "a última nota utilizada" — a nota na qual se escreveu por último. Reaberturas, invocações globais e despacho de instância única seguem essa regra.

## Campos de estado da janela

`state.json` armazena uma entrada por nota:

| Campo | Significado |
| --- | --- |
| `x`, `y` | Posição da nota em seu monitor |
| `width`, `height` | Dimensão atual da superfície; enquanto recolhida, `height` é a altura do cabeçalho |
| `is_open` | Se a nota é restaurada na inicialização |
| `monitor` | Nome do conector do monitor ao qual a nota pertence |
| `collapsed` | Se a nota está recolhida exibindo apenas a barra de cabeçalho |
| `expanded_width`, `expanded_height` | Dimensões a restaurar ao expandir; significativo apenas quando `collapsed` |
| `zoom_percent` | Escala de visualização do conteúdo da nota, 75–300, padrão 100 |

Todos os campos possuem valores padrão, garantindo que um `state.json` gravado por versões anteriores carregue perfeitamente: a ausência de `collapsed` implica estado expandido, e a ausência de geometrias expandidas recorre às dimensões padrão.

## Formatação inline em Markdown

O Markdown padrão não possui sintaxe para cor, destaque ou tamanho de fonte no texto; portanto, esses dados são representados como um conjunto controlado de elementos HTML seguros. Apenas atributos exclusivos do Note-it são aceitos, e estritamente com valores validados por allowlist — quaisquer outros atributos são descartados no carregamento.

| Formatação | Representação | Valores aceitos |
| --- | --- | --- |
| Cor do texto | `<span data-note-it-color="#2563EB">` | `#rgb` / `#rrggbb` |
| Destaque | `<mark data-note-it-highlight="#FDE68A">` | `#rgb` / `#rrggbb` |
| Tamanho do texto | `<span data-note-it-font-size="22">` | 12, 14, 16, 18, 22, 26, 32 |
| Conclusão da tarefa | `- [x] texto <!-- note-it:completed_at=… -->` | ISO 8601 com offset ou `Z` |

Nenhum desses marcadores é exibido como texto bruto no editor. O comentário de metadados da tarefa é o único comentário HTML preservado pelo sanitizador; todos os demais comentários HTML são removidos.

## Lixeira

Excluir uma nota move seu arquivo para fora do store ativo:

```text
notes/<uuid>.md   →   trash/<uuid>.<timestamp>.md
```

`<timestamp>` é o horário UTC em que a nota foi enviada para a lixeira, formatado como `YYYYMMDDTHHMMSSZ`. O identificador UUID é preservado, permitindo restaurar a nota ou inspecionar suas tarefas e metadados históricos.

A movimentação é atômica no mesmo sistema de arquivos através de `std::fs::rename`. Caso a movimentação falhe, a nota original permanece intacta em `notes/`. Se o arquivo de destino em `trash/` já existir, a operação falha de modo seguro (fail-closed) sem sobrescrever dados.

A lixeira pode ser listada e esvaziada através da interface gráfica (*Notas › Lixeira*) ou via CLI (`noteit lixeira listar`, `noteit lixeira limpar`). Restaurar uma nota move o arquivo de volta para `notes/<uuid>.md`. Se uma nota ativa com o mesmo UUID já existir no store, a restauração é recusada.

## Backups locais

O Note-it cria snapshots locais automáticos do store para proteger contra exclusões acidentais ou corrupções de dados.

Cada snapshot é um diretório autônomo dentro de `$XDG_DATA_HOME/note-it/backups/<timestamp>/`, nomeado com o timestamp ISO 8601 de sua criação. Um arquivo `manifest.json` na raiz do snapshot registra a versão do manifesto, a contagem de notas, a contagem de imagens, a contagem de notas na lixeira e se arquivos de configuração, estado de janela e histórico de estudo estavam presentes. Um diretório em `backups/` é considerado um snapshot válido apenas se for um diretório real, se seu nome não começar com `.` e se contiver um manifesto legível.

O manifesto **versão 3** inclui a versão 2 mais o indicador opcional de histórico de estudo; a versão 2 inclui a versão 1 mais a contagem de imagens. Snapshots mais antigos permanecem totalmente válidos.

**O que é incluído no backup:** `notes/`, `trash/`, `assets/`, `config.toml`, `state.json` e `study.json` quando presentes. Um arquivo de estudo existente é dado recuperável: se não puder ser copiado como arquivo regular, o snapshot não é comitado como completo.

Uma nota que referencie `![](../assets/…)` depende do arquivo de imagem correspondente; portanto, `assets/` é copiado com as mesmas garantias das notas: mesma estrutura, um subdiretório por nota, byte a byte, e um snapshot que falhe ao copiar uma imagem não é confirmado. Imagens que não sejam mais referenciadas por nenhuma nota também são copiadas — um backup é uma fotografia do store gerenciado, não uma triagem de arquivos.

`assets/` é copiado com validação mais estrita que `notes/`: uma anomalia em `notes/` é ignorada com aviso, mas em `assets/`, qualquer entrada que não corresponda ao padrão `<note-uuid>/<asset-uuid>.<ext>` indica que o store não está no estado esperado pelo Note-it, fazendo o backup falhar em vez de omitir silenciosamente conteúdo gerenciado. Um store criado antes da existência do suporte a imagens não possui o diretório `assets/`, o que é tratado como um store sem imagens e não como um store quebrado.

**O que nunca entra no backup:** o próprio diretório `backups/`, impedindo aninhamento recursivo; arquivos e diretórios iniciados por `.`, o que impede a inclusão de arquivos temporários (`.tmp.…`); qualquer item que não seja arquivo regular; e links simbólicos, que nunca são seguidos — garantindo que entradas forjadas no store não façam o backup copiar caminhos externos como `/etc` ou o diretório home do usuário.

**Quando ocorre:** no máximo um snapshot automático a cada 24 horas, executado **antes da primeira alteração elegível** após esse período — salvamento de nota ou envio para a lixeira. Executar antes da alteração é a essência do backup: a finalidade é permitir retornar ao estado anterior, de modo que o momento crucial a capturar é o instante imediatamente anterior à edição. Não há threads em segundo plano nem temporizadores contínuos; um daemon sem uso não consome recursos, e um processo mantido aberto por dias dispara o snapshot no instante em que o usuário recomeça a digitar. A data do último backup é determinada pelo próprio manifesto do snapshot mais recente, evitando arquivos auxiliares de controle que possam divergir do disco.

**Backup manual:** a opção *Dados › Fazer backup agora* gera um snapshot imediatamente, nunca é ignorada e sempre reporta sucesso ou falha, satisfazendo a janela de 24 horas como qualquer snapshot automático.

**Atomicidade:** um snapshot é montado em `backups/.tmp.<pid>.<n>/` e renomeado atomicamente para o caminho definitivo; o rename é o ponto de commit. Um processo interrompido deixa apenas um diretório temporário com prefixo `.tmp.`, que não é um snapshot válido e é removido na execução de backup seguinte. Apenas diretórios com esse prefixo temporário são removidos na limpeza.

**Retenção:** são mantidos os sete snapshots mais recentes em um pool unificado, e a retenção é aplicada **somente após um novo snapshot ter sido confirmado com sucesso**. Backups antigos nunca são removidos para abrir espaço antes que o novo backup esteja plenamente gravado.

**Tratamento de falhas:** a falha ao gerar um snapshot nunca bloqueia o salvamento de uma nota: o erro é emitido em `stderr`, a nota é persistida normalmente e nova tentativa de backup ocorre na próxima alteração elegível.

### Recuperando a partir de um snapshot

Não existe um botão de "restaurar tudo com um clique" na interface gráfica: sobrescrever um store ativo por um snapshot é uma operação transacional de múltiplos arquivos, e um botão automatizado seria o controle mais destrutivo da aplicação. O procedimento manual recomendado, com a aplicação fechada, é:

```bash
note-it quit                       # nenhum processo pode estar em execução

SNAP=~/.local/share/note-it/backups/2026-08-29T09-30-00Z
cat "$SNAP/manifest.json"          # confirme se é o snapshot desejado

# Preserve o estado atual para que o procedimento seja reversível
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

Para recuperar uma **única** nota, copie apenas o arquivo `<uuid>.md` desejado do diretório `notes/` do snapshot — e, caso contenha imagens, o diretório correspondente `assets/<note-uuid>/` ao lado. A nota referencia suas imagens por caminhos relativos a `notes/`, portanto ambos caminham juntos sem necessidade de edições manuais.

A legibilidade dos dados restaurados é garantida por testes de integração: o teste `a_snapshot_round_trips_into_a_fresh_isolated_store` copia snapshots para árvores XDG limpas exatamente dessa forma, abre os arquivos e valida notas, identificadores, Markdown, lixeira, configurações, estado de janelas e agendamento de estudos.

### Contra o que um backup local protege e não protege

O backup local protege contra: exclusão acidental, corrupção lógica de arquivos, edições indesejadas e necessidade de reverter para versões anteriores.

Ele **não** protege contra: falhas físicas de hardware de disco, perda ou roubo do computador ou corrupção global de sistemas de arquivos, pois os snapshots residem no mesmo meio de armazenamento físico das notas. Não há criptografia de backup. Caso seja necessária proteção contra falhas de hardware, deve-se manter cópias externas em outro dispositivo.

## Gravação atômica de arquivos

Para prevenir corrupção de dados em quedas de energia ou encerramento abrupto de processos:
1. Grava o conteúdo da nota em um arquivo temporário (`.tmp.<uuid>.<nanos>`) no mesmo diretório.
2. Executa flush e sincronização de dados (`fsync`) para o disco.
3. Renomeia e substitui atomicamente o arquivo de destino através de `std::fs::rename`.
4. Executa sync no diretório de notas, assegurando que o próprio rename seja durável em disco.

**A renomeação atômica (rename) é o ponto de commit.** Ou o rename se completa e a nota passa a ser a nova versão, ou não se completa e o arquivo permanece na versão anterior; não existe estado intermediário e um leitor nunca se depara com um arquivo corrompido ou parcialmente gravado (torn file). Se qualquer etapa até o rename falhar, o arquivo temporário é removido em vez de ser deixado como resíduo no diretório de notas.

O salvamento reporta erro para qualquer falha anterior ou durante o rename, e sucesso a partir do momento em que o rename é concluído. Essa é a regra fundamental da qual o documento em memória depende: ele é substituído apenas por uma versão que foi efetivamente gravada em disco com sucesso.

A etapa 4 ocorre após o ponto de commit. Os bytes da nota já estão persistidos com segurança no disco pela etapa 2 (`fsync`), de modo que o benefício obtido com a sincronização do diretório é garantir que o **rename** sobreviva a uma eventual queda de energia. Caso a sincronização do diretório falhe, a gravação em si foi bem-sucedida e é reportada como tal; um aviso (warning) é emitido porque o que ficou em dúvida foi a durabilidade da entrada de diretório, não a gravação dos dados da nota. Reportar como falha deixaria a aplicação com um estado em memória divergente do arquivo já gravado.

Nenhum mecanismo rastreia sincronizações de diretório pendentes. Executar sync em um diretório descarrega todas as entradas pendentes nele, de modo que o próximo salvamento bem-sucedido de qualquer nota torna durável também o rename anterior.

O que esta garantia **não** assegura: o sync não é indefinidamente repetido em caso de falha de hardware, um salvamento cujo sync de diretório falhou não tem durabilidade imediata garantida contra corte instantâneo de energia e o arquivo de notas não é resincronizado após o rename. A garantia inviolável é que uma nota nunca fica semi-escrita e nunca sofre reversão silenciosa enquanto a aplicação está em execução; uma queda de energia nessa janela estreita pode custar o último salvamento, mas nunca a integridade estrutural do arquivo.
