# Integração com o compositor Niri

Note-it foi projetado e testado para o compositor Wayland de blocos roláveis ​​[Niri](https://github.com/YaLTeR/niri).

## Configuração do Layer Shell

Note-it registra janelas com o namespace Wayland Layer Shell `note-it`.

- **Camada da área de trabalho:** As notas são anexadas à camada `bottom` com `exclusive_zone = 0`, mantendo-as no fundo atrás dos blocos Niri ativos.
- **Camada de sobreposição:** As notas mudam para a camada `overlay`, aparecendo sobre áreas de trabalho ativas para edição imediata.

## Ligando para Note-it de qualquer lugar

Os atalhos do aplicativo Note-it são eventos-chave comuns dentro do WebView da nota. Um cliente Wayland só recebe eventos importantes enquanto mantém o foco do teclado, portanto, esses atalhos funcionam quando a nota em si está focada e não fazem nada enquanto o navegador ou o terminal estão na frente. A convocação da nota, portanto, deve vir do compositor.

A alternância da camada autoritativa é uma ligação do compositor. Ele ativa a ação `toggle-layer` GApplication no processo Note-it já em execução, portanto não depende de uma nota, janela GTK ou WebView mantendo o foco.

```text
Ctrl+Shift+Space
    ↓
atalho global do Niri (repeat=false, allow-inhibiting=false)
    ↓  gapplication action io.github.theghols.NoteIt toggle-layer
a instância do Note-it que já está em execução
    ↓
uma decisão compartilhada de camada, aplicada ao vivo a todas as superfícies de notas
```

## Atalhos de teclado recomendados

Adicione o seguinte à configuração que Niri realmente carrega. Pode ser `~/.config/niri/config.kdl`, um arquivo incluído nele, como `binds.kdl`, ou o caminho selecionado por `NIRI_CONFIG`. Execute `niri validate` após editá-lo.

```kdl
// Inicia o daemon em segundo plano junto com o compositor
spawn-at-startup "note-it" "--background"

binds {
    // Alternância global oficial Desktop ↔ Overlay. `Space` é o nome no XKB.
    Ctrl+Shift+Space repeat=false allow-inhibiting=false {
        spawn "gapplication" "action" "io.github.theghols.NoteIt" "toggle-layer"
    }

    // Convoca o Note-it de qualquer aplicativo: restaura as notas e as traz
    // para a frente. Este é o atalho principal para acessá-las.
    Mod+Shift+N { spawn "note-it"; }

    // Recolhe todas as notas às respectivas barras ou expande todas novamente
    Mod+Shift+M repeat=false { spawn "note-it" "toggle-collapse-all"; }

    // Cria uma nota rapidamente
    Mod+Alt+N { spawn "note-it" "new"; }
}
```

Mantenha um alias `Mod+Shift+D` existente se ele já pertencer a Note-it e não entrar em conflito com outro aplicativo. Pode continuar a chamar `note-it toggle`; não é necessário para o fluxo de trabalho principal.

A entrada da área de trabalho `io.github.theghols.NoteIt.desktop` deve ser instalada em um diretório de aplicativos XDG para que `gapplication` possa resolver o ID do aplicativo:

```bash
install -Dm644 resources/io.github.theghols.NoteIt.desktop \
    ~/.local/share/applications/io.github.theghols.NoteIt.desktop
update-desktop-database ~/.local/share/applications
```

`note-it toggle` continua sendo o substituto de CLI e atinge a mesma transição compartilhada, mas iniciar um segundo processo GTK o torna mais lento do que a ação direta do aplicativo.

## O que uma invocação faz com a camada

Uma superfície de camada `bottom` é sempre pintada abaixo de janelas comuns; não há como aumentá-lo enquanto o mantém nessa camada. Uma nota deixada na área de trabalho, portanto, não pode ficar visível no navegador sem movê-la para a camada `overlay`.

A invocação aumenta as notas para `overlay` **sem reescrever a preferência armazenada**. A nota é genuinamente visível e `note-it toggle`, `Ctrl+Shift+Space` e a próxima reinicialização ainda refletem a camada que o usuário escolheu. A elevação dura até a próxima alteração ou reinicialização explícita da camada.

`note-it show` é diferente propositalmente: é uma solicitação explícita para colocar as notas no modo de sobreposição e armazena isso como preferência.

### Voltando da camada Desktop

A ligação Niri acima é o fluxo de trabalho `Ctrl+Shift+Space` real. Funciona quando um navegador, terminal ou editor está focado, quando a nota está completamente coberta e quando a nota nunca foi clicada desde que foi movida para a área de trabalho.

```kdl
Ctrl+Shift+Space repeat=false allow-inhibiting=false {
    spawn "gapplication" "action" "io.github.theghols.NoteIt" "toggle-layer"
}
```

O WebView ainda trata `Ctrl+Shift+Space` como um substituto local quando a nota já possui o foco. É útil, mas não é oficial e não pode fazer com que uma superfície `bottom` coberta receba entradas do teclado.

Em Niri 26.04 com protocolo Layer-Shell versão 4, alterar `bottom` para `overlay` não recria inerentemente a superfície. Uma superfície inferior ocluída pode, no entanto, esperar por um quadro antes que sua solicitação de camada seja confirmada. Para tornar a promoção imediata, Note-it remapeia deliberadamente apenas essa direção, com a interatividade do teclado temporariamente desativada para que o navegador mantenha o foco. `overlay` para `bottom` usa a transição de protocolo ao vivo diretamente. Nenhum dos caminhos ao vivo chama cegamente `present()`.

## Recolher uma nota ou todas elas

`Ctrl+Shift+M` dentro de uma nota recolhe apenas essa nota; é um evento chave no próprio WebView da nota e atinge apenas a nota que mantém o foco do teclado.

Recolher cada nota é uma combinação de teclas do compositor pelo mesmo motivo que uma invocação: nenhuma nota pode ser focada quando o usuário deseja que todas elas sejam tiradas do caminho. Ele executa `note-it toggle-collapse-all`, que recolhe tudo o que ainda está expandido e expande tudo quando todos estão recolhidos. Cada nota mantém seu próprio sinalizador `collapsed` e seu próprio tamanho expandido em `state.json`.

O comando deve ser acessível a partir do compositor, o que o gera em um ambiente simples. Instalar o binário em algum lugar em `PATH` — ou um inicializador apontando para a compilação — faz parte da configuração do atalho de teclado; uma ligação que nomeia um comando que não resolve falha silenciosamente.

Como uma invocação gerada é entregue à instância em execução por meio do despachante de instância única, o ambiente com o qual ela é gerada não importa: a instância que já possui as notas é aquela que atua sobre elas.

### O que a matriz mostra e o que ela não pode provar

O colapso foi relatado como falha na camada da área de trabalho. A execução de cada gatilho em uma sessão Niri real, com um armazenamento isolado e um barramento privado, encontrou o próprio colapso funcionando em ambas as camadas e através de cada ponto de entrada: **Recolher nota**, `Ctrl+Shift+M` e `toggle-collapse-all` do menu, antes de uma mudança de camada, depois de uma, e em ambas as direções de uma. A superfície realmente encolhe até sua barra em `bottom`, ocluída ou não.

O que é específico da camada é o alcance, não o colapso. `Ctrl+Shift+M` é um evento chave no próprio WebView da nota, então ele precisa da nota para manter o foco do teclado, e uma superfície `bottom` fica atrás de cada janela: uma vez que o foco foi para outro lugar, não há mais nada para clicar. Na camada de sobreposição, a nota está no topo e um clique traz o foco de volta, e é por isso que o mesmo acorde parece funcionar ali e não aqui. O caminho de volta são as ligações do compositor, que é exatamente para isso que servem - `Ctrl+Shift+Space` para promover as notas, ou `Mod+Shift+M` para recolher e expandir todas elas sem focar em nada.

Os conjuntos sintéticos cobrem o que um processo pode decidir por si só: que uma mudança de camada nunca altera o sinalizador `collapsed` de uma nota, que um colapso nunca altera a camada, que qualquer ordem produz exatamente uma mudança de cada e que nenhum acorde é acionado duas vezes (`src/state.rs`, `src/layer_shell.rs`, `ui/tests/layer_collapse.test.ts`). Se o compositor ainda direciona o teclado para uma superfície `bottom`, e se clicar em uma delas restaura o foco para ela, são respostas de Niri e devem ser questionadas durante uma sessão em execução.

## O tema do sistema e a área de trabalho

**Tema → Sistema** segue o esquema de cores do desktop, lido dentro de WebView até `prefers-color-scheme`. WebKitGTK deriva isso das configurações GTK da sessão em que o aplicativo foi iniciado, portanto, uma sessão Wayland que não relata nenhuma preferência simplesmente resolve o tema claro - as notas são sempre totalmente estilizadas de qualquer maneira.

A preferência é observada enquanto o aplicativo é executado, portanto, alternar a área de trabalho entre claro e escuro atinge notas abertas sem reiniciar. **Claro** e **Escuro** são escolhas explícitas e ignoram totalmente a área de trabalho.

O tema é global e vive em `config.toml`. Ele veste os menus e popovers do aplicativo; cada nota mantém a cor e o padrão do papel que foi fornecido, portanto, uma nota amarela permanece amarela em uma área de trabalho escura.
