# Note-it — Visão e princípios do produto

## Visão

Note-it é um aplicativo minimalista e sem distrações para notas adesivas (post-its) na área de trabalho, desenvolvido para ambientes Linux Wayland.

Não pretende substituir bases de conhecimento abrangentes como Obsidian ou Notion. Em vez disso, atende a uma necessidade clara e focada: **criar notas rápidas sem esforço, mantê-las naturalmente na área de trabalho e invocá-las instantaneamente sobre as janelas ativas quando necessário.**

## Princípios fundamentais

- **Local-First e Offline-First:** Todos os dados do usuário permanecem inteiramente no sistema de arquivos local.
- **Sem nuvem, sem contas:** Não é necessário registro, login, servidores de sincronização ou serviços externos.
- **Privacidade por design:** Zero coleta de métricas, telemetria, relatórios de falha ou chamadas de rede em segundo plano.
- **Nativo para Wayland:** Projetado para compositores Wayland modernos utilizando o protocolo `wlr-layer-shell`, com suporte de primeira classe para Niri.
- **Armazenamento padrão em Markdown:** Cada nota é um arquivo Markdown (`.md`) portátil e padrão em disco. Nenhum banco de dados proprietário para o texto das notas.
- **Verdadeiro WYSIWYG:** O que você vê é o texto formatado. Marcadores de sintaxe Markdown nunca poluem o fluxo de edição.
- **Foco no teclado:** Criação instantânea de notas (`Ctrl+N`), fechamento rápido (`Ctrl+W`) e controles intuitivos de formatação.
- **Alto desempenho:** Baixo consumo de recursos, uso de CPU próximo de zero quando ocioso e inicialização instantânea.
