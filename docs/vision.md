# Note-it — Visão e princípios do produto

## Visão

Note-it é um aplicativo minimalista e sem distrações para notas adesivas na área de trabalho, criado para ambientes Linux Wayland.

Ele não pretende substituir bases de conhecimento abrangentes como Obsidian ou Notion. Em vez disso, atende a uma necessidade única e focada: **criar notas rápidas sem esforço, mantê-las naturalmente na área de trabalho e invocá-las instantaneamente acima das janelas ativas quando necessário.**

## Princípios fundamentais

- **Local-First e Offline-First:** Todos os dados do usuário permanecem inteiramente no sistema de arquivos local.
- **Sem nuvem, sem contas:** Não é necessário registro, login, servidores de sincronização ou serviços externos.
- **Privacidade desde o projeto:** nenhuma análise de uso, telemetria, envio de relatórios de falha ou chamada de rede em segundo plano.
- **Wayland Nativo:** Projetado para compositores Wayland modernos que usam o protocolo `wlr-layer-shell`, com suporte de primeira classe para Niri.
- **Armazenamento padrão:** Cada nota é um arquivo Markdown (`.md`) portátil e padrão em disco. Nenhum banco de dados proprietário para texto de notas.
- **Verdadeiro WYSIWYG:** O que você vê é texto formatado. Os marcadores de sintaxe Markdown nunca atrapalham o fluxo de edição.
- **Centrado no teclado:** Criação instantânea de notas (`Ctrl+N`), dispensa rápida (`Ctrl+W`) e controles de formatação intuitivos.
- **Alto desempenho:** consumo mínimo de recursos, uso de CPU ocioso próximo de zero e inicialização rápida.
