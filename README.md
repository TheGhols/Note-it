<p align="center">
  <img src="assets/note-it-logo.png" alt="Logo do Note-it" width="180">
</p>

<h1 align="center">Note-it</h1>

<p align="center">
  Notas adesivas minimalistas para Linux Wayland.
</p>

> **Status:** experimental, em desenvolvimento ativo.
> Niri é o compositor primário apoiado nesta fase.

---

## Visão geral

O Note-it é um aplicativo de notas leve, local-first e sem distrações, feito nativamente para
Wayland com o protocolo `wlr-layer-shell`. Ele serve para capturar uma ideia rapidamente e deixá-la
fixada na área de trabalho — ou trazê-la para cima de todas as janelas com um atalho.

Cada nota é um arquivo Markdown (`.md`) comum no disco, com front matter YAML. O arquivo continua
legível e editável em qualquer outro programa, enquanto a edição no Note-it é WYSIWYG de verdade:
sem marcadores de sintaxe atrapalhando o cursor.

## Principais recursos

Tudo listado aqui já está implementado.

**Janela e área de trabalho**

- Nativo em Wayland, com GTK4, `gtk4-layer-shell` e WebKitGTK 6.0.
- Modo **Área de trabalho** (camada `bottom`), acima do papel de parede e atrás das janelas, e modo
  **Sempre no topo** (camada `overlay`), acima de tudo — alternáveis a qualquer momento.
- Arrastar, redimensionar e posicionar por monitor, com a geometria preservada entre sessões.
- Recolher a nota à sua barra de título e expandir de volta ao tamanho anterior, individualmente ou
  em todas as notas de uma vez.
- Instância única com IPC, para integrar com atalhos globais do compositor.

**Aparência**

- Sete cores de papel: amarelo, azul, verde, rosa, roxo, cinza e preto.
- Cinco tipos de papel — liso, pautado, pontilhado, quadriculado pequeno e quadriculado grande — em
  três intensidades.
- Tema da interface **Sistema**, **Claro** ou **Escuro**, compartilhado por todas as notas. O tema
  veste os menus e as bordas do aplicativo; a nota mantém a cor e o papel que recebeu.
- Zoom de 75% a 200% por nota, que escala o texto sem alterar o documento.

**Edição**

- Títulos H1–H6, listas, sublistas, negrito, itálico, tachado e sublinhado.
- Listas de tarefas com caixas reais, aninhadas, e data de conclusão registrada por tarefa.
- Cor do texto, marca-texto e tamanho do texto por trecho, a partir de paletas compactas no menu.
- Blocos de código com linguagem preservada e realce de sintaxe para 16 linguagens.
- Callouts no formato dos alertas do GitHub — `NOTE`, `TIP`, `IMPORTANT`, `WARNING`, `CAUTION`.
- Citações e comentários. O comentário fica guardado no arquivo como `<!-- ... -->` e continua
  editável, sem fazer parte do texto visível da nota.
- `->` vira uma seta de verdade enquanto se digita, exceto dentro de código.

**Confiabilidade e privacidade**

- Salvamento automático com escrita atômica em disco: ou a nota nova está gravada, ou a anterior
  continua intacta, nunca um arquivo pela metade.
- A data de modificação muda apenas quando o conteúdo realmente muda — abrir e fechar uma nota não
  conta como editá-la.
- Sanitização de HTML e de URLs em tudo que entra na nota, inclusive ao colar.
- Zero telemetria, zero analytics, zero requisições de rede, zero contas.

## Requisitos

**Sistema**

- Linux com compositor Wayland que ofereça `wlr-layer-shell` (testado no Arch Linux com Niri).

**Dependências**

- `gtk4`
- `gtk4-layer-shell`
- `webkitgtk-6.0`
- `glib2`
- `pkgconf`

**Para compilar**

- Rust (stable) e Cargo
- Node.js (>= 20) e pnpm

## Executando localmente

No Arch Linux, instale os pré-requisitos uma vez:

```bash
sudo pacman -S --needed gtk4 gtk4-layer-shell webkitgtk-6.0 rust nodejs pnpm pkgconf base-devel
```

Depois, a partir da raiz do repositório:

```bash
./scripts/run-note-it
```

O script prepara o frontend quando necessário, compila o host de forma incremental e inicia a
instância única do aplicativo. Para subir apenas o daemon, sem criar janelas:

```bash
./scripts/run-note-it --background
```

Para experimentar sem tocar nas suas notas reais, use o ambiente isolado:

```bash
scripts/note-it-isolated          # árvore XDG temporária, removida ao sair
scripts/note-it-isolated --keep   # mantém a árvore para inspeção
```

## Comandos disponíveis

```bash
note-it                       # traz as notas de volta e reaproveita a instância em execução
note-it new                   # cria uma nota
note-it show                  # mostra todas as notas em Sempre no topo
note-it hide                  # salva e esconde todas as notas
note-it toggle                # alterna entre Área de trabalho e Sempre no topo
note-it toggle-collapse-all   # recolhe todas as notas, ou expande todas
note-it quit                  # salva tudo e encerra o aplicativo
```

Os mesmos comandos funcionam com `./scripts/run-note-it <comando>` durante o desenvolvimento.

Atalhos dentro de uma nota: `Ctrl+N` cria outra nota, `Ctrl+W` fecha a atual, `Ctrl+=` / `Ctrl+-` /
`Ctrl+0` controlam o zoom e `Ctrl+Shift+M` recolhe ou expande. A nota também aceita
`Ctrl+Shift+Espaço` localmente quando já está focada; o caminho global e autoritativo para alternar
a camada é a ligação do Niri abaixo.

## Integração com o Niri

Adicione ao seu `~/.config/niri/config.kdl`:

```kdl
// Sobe o daemon do Note-it junto com a sessão
spawn-at-startup "note-it" "--background"

// Atalho global autoritativo para alternar a camada
binds {
    Ctrl+Shift+Space repeat=false allow-inhibiting=false {
        spawn "gapplication" "action" "io.github.theghols.NoteIt" "toggle-layer"
    }
}
```

## Armazenamento e privacidade

As notas ficam em arquivos Markdown individuais, seguindo a especificação XDG Base Directory:

| Caminho | Conteúdo |
| --- | --- |
| `$XDG_DATA_HOME/note-it/notes/` | as notas, uma por arquivo `<uuid>.md` |
| `$XDG_CONFIG_HOME/note-it/config.toml` | preferências compartilhadas |
| `$XDG_STATE_HOME/note-it/state.json` | geometria das janelas e estado da interface |
| `$XDG_RUNTIME_DIR/note-it/` | arquivos de IPC da instância única |

Nada sai da máquina. O aplicativo não faz requisições de rede, não coleta métricas e não tem conta,
login ou sincronização. Como cada nota é um `.md` comum, os arquivos podem ser versionados,
copiados ou lidos por qualquer outro editor sem passar pelo Note-it.

## Documentação

A documentação técnica está em [`docs/`](docs/), em inglês:

- [Visão e princípios](docs/vision.md)
- [Arquitetura](docs/architecture.md)
- [Formato Markdown das notas](docs/markdown-format.md)
- [Armazenamento e caminhos XDG](docs/storage.md)
- [Integração com o Niri](docs/niri.md)
- [Segurança e sanitização de HTML](docs/security.md)
- [Guia de desenvolvimento](docs/development.md)
- [Decisões arquiteturais](docs/decisions.md)
- [Roadmap](docs/roadmap.md)

## Estado atual

O editor e o ciclo de vida das notas estão completos até a Fase 3.5 (Smart Blocks). O que vem
depois — motor de cálculo, conversões, busca global, lixeira e backup, além do núcleo compartilhado
com uma CLI completa — está planejado no [roadmap](docs/roadmap.md) e **ainda não existe**.

## Licença

Distribuído sob a [Licença MIT](LICENSE).
