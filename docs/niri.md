# Integração com o compositor Niri

Note-it integra-se com o compositor Wayland Niri via protocolo `wlr-layer-shell-unstable-v1` (usando `gtk4-layer-shell`):
- **Camada Desktop:** As notas são anexadas à camada `bottom`, abaixo das janelas normais, integrando-se à área de trabalho.
- **Camada Overlay:** As notas alternam para a camada `overlay`, aparecendo sobre as áreas de trabalho ativas para edição imediata.

## Invocando o Note-it de qualquer lugar

Os atalhos internos do aplicativo Note-it são eventos normais de teclado dentro do WebView da nota. Um cliente Wayland só recebe eventos de teclado enquanto detém o foco de entrada; portanto, esses atalhos funcionam quando a nota está em foco e não fazem nada enquanto o navegador ou o terminal estão na frente. A invocação da nota (summon), portanto, deve vir do compositor.

A alternância canônica de camada é um atalho configurado no compositor. Ela ativa a ação GApplication `toggle-layer` no processo Note-it já em execução, de modo que não depende de uma nota, janela GTK ou WebView manterem o foco de teclado.

```text
Ctrl+Shift+Space
    ↓
Atalho global Niri (repeat=false, allow-inhibiting=false)
    ↓  gapplication action io.github.theghols.NoteIt toggle-layer
instância Note-it já em execução
    ↓
decisão única de camada compartilhada aplicada a todas as superfícies de notas
```

## Atalhos de teclado recomendados

Adicione o seguinte trecho à configuração que o Niri efetivamente carrega (`~/.config/niri/config.kdl`, um arquivo incluído por ele como `binds.kdl`, ou o caminho definido por `NIRI_CONFIG`). Execute `niri validate` após a edição.

```kdl
// Iniciar daemon em segundo plano na inicialização do compositor
spawn-at-startup "note-it" "--background"

binds {
    // Alternância global canônica Desktop ↔ Overlay. `Space` é o nome XKB.
    Ctrl+Shift+Space repeat=false allow-inhibiting=false {
        spawn "gapplication" "action" "io.github.theghols.NoteIt" "toggle-layer"
    }

    // Invocar Note-it de qualquer aplicativo: restaura as notas e as
    // traz para a frente. Este é o principal atalho de uso.
    Mod+Shift+N { spawn "note-it"; }

    // Recolher todas as notas para suas barras, ou expandir todas novamente
    Mod+Shift+M repeat=false { spawn "note-it" "toggle-collapse-all"; }

    // Criação rápida de nova nota
    Mod+Alt+N { spawn "note-it" "new"; }
}
```

Mantenha um atalho existente `Mod+Shift+D` se ele já pertencer ao Note-it e não conflitar com outro aplicativo. Ele pode continuar chamando `note-it toggle`, embora não seja estritamente necessário para o fluxo principal.

O arquivo desktop `io.github.theghols.NoteIt.desktop` deve estar instalado em um diretório de aplicações XDG para que o `gapplication` resolva o identificador da aplicação:

```bash
install -Dm644 resources/io.github.theghols.NoteIt.desktop \
    ~/.local/share/applications/io.github.theghols.NoteIt.desktop
update-desktop-database ~/.local/share/applications
```

`note-it toggle` continua sendo o fallback via CLI e atinge a mesma transição compartilhada, mas lançar um segundo processo GTK o torna ligeiramente mais lento do que invocar a ação de aplicação direta.

## O que uma invocação faz com a camada

Uma superfície na camada `bottom` é sempre desenhada abaixo de janelas comuns; não há como elevá-la mantendo-a nessa mesma camada. Uma nota mantida na camada desktop, portanto, não pode se tornar visível sobre o navegador sem ser promovida para a camada `overlay`.

A invocação eleva as notas para `overlay` **sem sobrescrever a preferência armazenada**. A nota torna-se visível imediatamente, e `note-it toggle`, `Ctrl+Shift+Space` e a próxima reinicialização continuam refletindo a camada escolhida pelo usuário. Essa elevação temporária perdura até a próxima troca explícita de camada ou reinício.

`note-it show` é diferente por projeto: é uma solicitação explícita para colocar as notas em modo overlay, gravando essa escolha como preferência durável.

### Retornando da camada Desktop

O atalho do Niri descrito acima é o fluxo canônico para `Ctrl+Shift+Space`. Ele funciona quando um navegador, terminal ou editor está em foco, quando a nota está completamente coberta por outras janelas e mesmo quando a nota não recebeu nenhum clique desde que foi movida para a camada desktop.

```kdl
Ctrl+Shift+Space repeat=false allow-inhibiting=false {
    spawn "gapplication" "action" "io.github.theghols.NoteIt" "toggle-layer"
}
```

O WebView ainda trata `Ctrl+Shift+Space` como um fallback local quando a nota já detém o foco. Isso é útil, mas não é canônico e não permite que uma superfície na camada `bottom` coberta receba entrada do teclado.

No Niri 26.04 com protocolo layer-shell versão 4, alternar de `bottom` para `overlay` não recria a superfície. Uma superfície ocluída em `bottom` pode, contudo, aguardar um frame antes de seu pedido de camada ser comitado. Para tornar a promoção instantânea, Note-it remapeia deliberadamente apenas essa direção com a interatividade de teclado temporariamente desativada, preservando o foco na janela do navegador. A transição de `overlay` para `bottom` utiliza diretamente a transição dinâmica do protocolo. Nenhum dos caminhos chama cegamente `present()`.

## Recolhendo uma nota ou todas elas

`Ctrl+Shift+M` dentro de uma nota recolhe apenas aquela nota específica; é um evento de teclado no WebView da própria nota e só atinge a nota que detém o foco.

Recolher todas as notas é um atalho do compositor pelo mesmo motivo que a invocação global: nenhuma nota pode estar em foco quando o usuário deseja ocultá-las da visão. O atalho executa `note-it toggle-collapse-all`, que recolhe todas as notas que ainda estiverem expandidas e expande todas caso todas já estejam recolhidas. Cada nota preserva sua própria flag `collapsed` e seu tamanho expandido no `state.json`.

O comando precisa ser alcançável pelo compositor, que o executa em um ambiente limpo. Ter o binário instalado no `PATH` — ou um inicializador apontando para a compilação — faz parte da configuração do atalho; um atalho nomeando um comando não encontrado falha silenciosamente.

Como a invocação lançada é encaminhada para a instância ativa através do despachante de instância única, o ambiente em que ela foi gerada não afeta a execução: a instância que já gerencia as notas é a que atua sobre elas.

### O que a matriz comprova e o que ela não pode atestar

O recolhimento de notas foi outrora reportado como falho na camada desktop. Executar todos os disparos em uma sessão real do Niri, com um store isolado e um barramento D-Bus privado, confirmou que o recolhimento funciona perfeitamente em ambas as camadas e através de todos os pontos de entrada: o menu **Recolher nota**, `Ctrl+Shift+M` e `toggle-collapse-all`, antes de uma mudança de camada, depois dela e em ambas as direções. A superfície de fato encolhe para a sua barra de cabeçalho na camada `bottom`, esteja ela coberta por outras janelas ou não.

O que depende da camada é o alcance do foco, não a operação de recolhimento. `Ctrl+Shift+M` é um evento de teclado dentro do WebView da própria nota, dependendo de que ela detenha o foco de teclado; uma superfície na camada `bottom` fica atrás de todas as janelas: quando o foco está em outro aplicativo, não há superfície exposta para clicar. Na camada overlay a nota fica no topo e um clique recupera o foco imediatamente, razão pela qual o mesmo atalho parecia funcionar lá e não aqui. O caminho de retorno são os atalhos do compositor, projetados exatamente para essa finalidade — `Ctrl+Shift+Space` para promover as notas ou `Mod+Shift+M` para recolher e expandir todas sem necessidade de foco prévio.

As suítes sintéticas de teste cobrem o que um processo pode decidir por conta própria: que uma mudança de camada nunca altera a flag `collapsed` de uma nota, que um recolhimento nunca altera a camada, que qualquer ordem produz exatamente uma alteração de cada e que nenhum atalho dispara duas vezes (`src/state.rs`, `src/layer_shell.rs`, `ui/tests/layer_collapse.test.ts`). Se o compositor encaminha eventos de teclado para uma superfície na camada `bottom` e se clicar nela restaura o foco são comportamentos gerenciados pelo Niri, verificáveis em uma sessão real em execução.

## O tema do sistema e a área de trabalho

**Tema → Sistema** segue o esquema de cores do desktop, lido dentro do WebView através de `prefers-color-scheme`. O WebKitGTK obtém essa informação das configurações GTK da sessão em que o aplicativo foi iniciado; portanto, uma sessão Wayland que não declare preferências simplesmente adota o tema claro — as notas permanecem completamente estilizadas em qualquer caso.

A preferência é monitorada dinamicamente enquanto o aplicativo executa, de modo que alternar o desktop entre claro e escuro atualiza as notas abertas sem necessidade de reinicialização. As opções **Claro** e **Escuro** são escolhas explícitas que ignoram o tema do desktop.

O tema é uma configuração global armazenada em `config.toml`. Ele personaliza menus e popovers da aplicação; cada nota individual preserva a cor e o padrão de papel atribuídos, de modo que uma nota amarela permanece amarela mesmo sobre um desktop escuro.
