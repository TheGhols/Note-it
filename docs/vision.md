# Note-it — Visão e princípios do produto

## Visão

Note-it é um aplicativo minimalista e sem distrações para notas adesivas na área de trabalho, criado para ambientes Linux Wayland.

Ele não pretende substituir bases de conhecimento abrangentes como Obsidian ou Notion. Em vez disso, atende a uma necessidade única e focada: **criar notas rápidas sem esforço, mantê-las naturalmente na área de trabalho e invocá-las instantaneamente acima das janelas ativas quando necessário.**

## Princípios fundamentais

- **Local-First e Offline-First:** Todos os dados do usuário permanecem inteiramente no sistema de arquivos local.
- **Sem nuvem, sem contas:** Não é necessário registro, login, servidores de sincronização ou serviços externos.
- **Privacidade desde o projeto:** nenhuma análise de uso, telemetria, envio de relatórios de falha ou chamada de rede em segundo plano. Sobre a nuance que o MCP introduz, veja "Segundo Cérebro" abaixo.
- **Wayland Nativo:** Projetado para compositores Wayland modernos que usam o protocolo `wlr-layer-shell`, com suporte de primeira classe para Niri.
- **Armazenamento padrão em Markdown:** Cada nota é um arquivo Markdown (`.md`) portátil e padrão em disco. Nenhum banco de dados proprietário para texto de notas.
- **Verdadeiro WYSIWYG:** O que você vê é texto formatado. Os marcadores de sintaxe Markdown nunca atrapalham o fluxo de edição.
- **Centrado no teclado:** Criação instantânea de notas (`Ctrl+N`), fechamento rápido (`Ctrl+W`) e controles de formatação intuitivos.
- **Alto desempenho:** consumo mínimo de recursos, uso de CPU ocioso próximo de zero e inicialização rápida.

## Segundo Cérebro, e por que ele não muda esta visão

A partir da Fase 4.2 o Note-it expõe as notas como **contexto recuperável** para
uma IA externa, através do MCP. Isso parece estar em tensão com "minimalista e
sem distrações", e a tensão é resolvida assim:

- **A interface não muda.** A GUI continua sendo notas adesivas rápidas na área
  de trabalho. Não há painel de IA, chat, barra de assistente nem dashboard. O
  Note-it continua sem pretender substituir Obsidian ou Notion.
- **A complexidade é headless.** O Segundo Cérebro é um contrato para
  *programas*, não uma tela para pessoas: ele existe no `noteit-core` e no
  servidor MCP, e uma pessoa que nunca conectar uma IA não vê diferença nenhuma.
- **A IA fica fora.** Nenhum modelo é embutido, baixado ou executado pelo
  Note-it. Ele fornece contexto local e rastreável; quem raciocina é o programa
  do outro lado.

### A nuance de privacidade que precisa ser dita

O `noteit-mcp` não abre rede, e isso é verificado mecanicamente. Mas:

```text
"O Note-it não envia notas para a Internet."     verdadeiro
"Uma nota nunca poderá sair desta máquina."      FALSO, se o host de IA for de nuvem
```

Se a pessoa conectar um host de IA em nuvem, esse host encaminhará ao provedor o
que a tool devolveu, como faria com qualquer outro contexto. Essa decisão é da
pessoa, e o produto não vai fingir que ela não existe. Detalhes em
`docs/second-brain.md`.
