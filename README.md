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
- Zoom de 75% a 300% por nota, que escala o conteúdo sem alterar o documento, e escala global da
  interface de 90% a 160% sem afetar a nota.

**Edição**

- Títulos H1–H6, listas, sublistas, negrito, itálico, tachado e sublinhado.
- Listas de tarefas com caixas reais, aninhadas, e data de conclusão registrada por tarefa.
- Cor do texto, marca-texto e tamanho do texto por trecho, a partir de paletas compactas no menu.
- Blocos de código com linguagem preservada e realce de sintaxe para 16 linguagens.
- Callouts no formato dos alertas do GitHub — `NOTE`, `TIP`, `IMPORTANT`, `WARNING`, `CAUTION`.
- Citações e comentários. O comentário fica guardado no arquivo como `<!-- ... -->` e continua
  editável, sem fazer parte do texto visível da nota.
- `->` vira uma seta de verdade enquanto se digita, exceto dentro de código.

**Cálculos na nota**

- `= 2 + 2` mostra `4` ao lado da linha, enquanto se escreve. Sem botão, sem modo, sem recalcular.
- `preco := 120` declara um valor que as linhas abaixo podem usar. Mudar a declaração atualiza todos
  os resultados que dependem dela na hora.
- Porcentagens do dia a dia: `10% de 200`, `200 + 10%`, `200 - 10%`.
- `sum`, `avg` e `count` sobre o bloco de linhas de cálculo logo acima.
- Decimal com `.` ou com `,` — `10,5` funciona como se espera de um teclado brasileiro.
- O resultado é uma decoração do editor, nunca conteúdo: o `.md` guarda exatamente o que foi
  digitado, a data de modificação não se move por um recálculo, e reabrir a nota recalcula tudo.
- O interpretador não tem `eval` nem execução de código de espécie alguma, e não trouxe nenhuma
  dependência nova.

**Conversões de unidades**

- `= 10 km em m` mostra `10000 m` ao lado da linha, enquanto se escreve.
- Comprimento, massa, volume, temperatura, tempo, área, dados digitais e velocidade — a lista
  completa de unidades e apelidos está em [docs/features.md](docs/features.md).
- O lado esquerdo é uma expressão completa: `= (10 + 5) km em m` e `= distancia km em m` funcionam,
  e mudar a variável atualiza a conversão na hora.
- Temperatura converte como escala, não como fator: `= 0 C em F` é `32 °F`.
- `KB` é decimal e `KiB` é binário — `= 1 GB em MB` dá `1000 MB` e `= 1 GiB em MiB` dá `1024 MiB`.
- Unidade desconhecida, unidades incompatíveis e conversão impossível são avisos discretos ao lado
  da linha, nunca no arquivo.
- Tudo local, offline e determinístico. Moedas ainda não existem — e nenhuma cotação foi chumbada
  no código, justamente porque estaria errada no minuto seguinte.

**Busca e navegação**

- `Ctrl+K` busca em todas as notas, abertas ou fechadas, sem sair da nota em que se está.
- A busca ignora maiúsculas **e acentos**: `biopsia` encontra `Biópsia`, `coracao` encontra
  `Coração`.
- Consulta vazia lista as notas escritas mais recentemente — o mesmo campo serve de troca rápida
  entre notas. "Mais recente" é a última mudança de **conteúdo**: trocar a cor, o papel ou o
  tamanho da fonte não faz uma nota subir na lista.
- `Enter` abre o resultado: ativa a nota se já estiver aberta, abre se estiver fechada, expande se
  estiver recolhida, e leva até a ocorrência.
- A busca olha **todas** as notas do store, sem teto de varredura; o limite de 100 é de resultados
  exibidos, não de notas examinadas.
- Pesquisar não escreve nada. Nenhum arquivo é salvo, nenhuma data de modificação se move, e nenhum
  índice é criado em disco: mil notas são varridas em cerca de 40 ms.
- O que a busca encontra é o que está no arquivo. Um `4` que veio de `= 2 + 2` é decoração, não
  texto, e não aparece em busca alguma.

**Tags e propriedades**

- *☰ › Metadados* organiza Tags e Propriedades sem transformar a barra em formulário. Tags também
  aparecem numa única linha discreta de pílulas; em notas estreitas/baixas viram contador e o
  excesso vira `+N`.
- Tags aceitam Unicode, português e espaços. Comparação ignora caixa e acentos para evitar
  duplicatas, mas preserva a grafia escolhida para exibição. A cor vem deterministicamente da
  identidade da tag e não é gravada no Markdown.
- Propriedades são pares textuais simples, como `status → revisando` e `fonte → Harrison`, editados
  no painel com scroll interno. Sugestões vêm das notas vivas; não há índice, banco ou sidecar.
- Tudo fica no front matter YAML, fora do corpo: busca textual, título, flashcards e Study não leem
  Tags/Properties. Alterá-las não muda a data da última edição textual.

**Localizar e substituir**

- `Ctrl+F` localiza dentro da nota atual, com contador de ocorrências; `Enter` e `Shift+Enter`
  percorrem para frente e para trás, dando a volta nas duas pontas.
- `Ctrl+H` abre a substituição: uma ocorrência por vez ou todas de uma vez.
- `Substituir todas` é uma edição só: um `Ctrl+Z` desfaz as vinte substituições juntas.
- Marcas, listas, títulos e estrutura sobrevivem à substituição, porque ela acontece sobre o
  documento e não sobre o texto do arquivo.
- Diferente da busca global, localizar e substituir **respeita acentos** — substituir é destrutivo,
  e `saude` não deve reescrever `saúde`.

**Lixeira e backup**

- Apagar uma nota significa mandá-la para a lixeira, com confirmação que diz, em palavras, que dá
  para desfazer. O `×` continua sendo apenas fechar a janela — nunca apagou nada e continua sem
  apagar.
- A nota é salva antes de sair do lugar. Se o texto mais recente não puder ser gravado, nada é
  movido: a nota continua aberta e o erro aparece.
- Uma nota na lixeira some da busca, do `Ctrl+K`, do summon e da reabertura ao reiniciar — porque o
  arquivo saiu de `notes/`, não porque cada um deles a filtra.
- Restaurar devolve o mesmo arquivo, byte a byte, com o mesmo identificador. Restaurar não é editar:
  a data de modificação não muda, então a nota volta para o lugar que tinha na lista de recentes em
  vez de fingir que acabou de ser escrita.
- Restaurar nunca escreve por cima de uma nota viva com o mesmo identificador. Se houver, a operação
  é recusada e nenhum dos dois arquivos é tocado.
- Backup local automático: no máximo um a cada 24 h, tirado **antes** da primeira alteração depois
  desse intervalo — o estado que interessa guardar é o de antes da edição. Sem timer, sem thread:
  parado, o daemon não faz nada.
- Um snapshot é uma pasta comum com `notes/`, `trash/`, `config.toml` e `state.json`. Dá para ler
  com `ls` e recuperar com `cp`, sem depender de nada que também possa quebrar.
- *Dados › Fazer backup agora* tira um na hora e diz se deu certo, numa linha no rodapé da nota.
- Ficam os sete mais recentes, e os antigos só são removidos **depois** que o novo está pronto. Um
  backup que falha nunca custa a proteção que já existia, e nunca impede um salvamento normal.
- Tudo local. Nenhum servidor, nenhuma nuvem, nenhuma requisição de rede.

> **Backup local não é proteção contra desastre.** Os snapshots ficam no mesmo disco das notas.
> Protegem contra exclusão acidental, corrupção lógica, uma edição para desfazer e uma versão para
> voltar. **Não** protegem contra HD/SSD morto, máquina perdida ou roubada, e não são
> criptografados.

**Colar link**

- Com um trecho selecionado, colar uma URL transforma o trecho em link: selecione `site oficial`,
  cole `https://example.com`, e a nota guarda `[site oficial](https://example.com)`.
- Passa pela mesma lista de esquemas permitidos do resto do aplicativo. `javascript:` e companhia
  são colados como texto, nunca como link.
- Nada é buscado na internet: sem título remoto, sem favicon, sem prévia. E um `Ctrl+Z` desfaz.

**Confiabilidade e privacidade**

- Salvamento automático com escrita atômica em disco: ou a nota nova está gravada, ou a anterior
  continua intacta, nunca um arquivo pela metade.
- A data de modificação muda apenas quando o conteúdo realmente muda — abrir e fechar uma nota não
  conta como editá-la, e mandar para a lixeira ou restaurar também não.
- Apagar é reversível, e o arquivo só sai do lugar depois que o texto está salvo.
- Backup local automático dos arquivos recuperáveis, em pastas comuns no próprio disco.
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

### Aplicativo gráfico (`note-it`)

```bash
note-it                       # traz as notas de volta e reaproveita a instância em execução
note-it new                   # cria uma nota
note-it show                  # mostra todas as notas em Sempre no topo
note-it hide                  # salva e esconde todas as notas
note-it toggle                # alterna entre Área de trabalho e Sempre no topo
note-it toggle-collapse-all   # recolhe todas as notas, ou expande todas
note-it quit                  # salva tudo e encerra o aplicativo
```

### CLI headless (`noteit`)

A linha de comando `noteit` é um binário headless independente que não requer sessão gráfica nem WebKit:

```bash
noteit                        # orientação inicial de uso
noteit ajuda                  # mostra a ajuda dos comandos (alias: noteit help, noteit --help)
noteit versao                 # mostra a versão do Note-it (alias: noteit version, noteit --version)
noteit status                 # verifica os diretórios e ambiente XDG de forma estritamente read-only
```

Os mesmos comandos funcionam com `./scripts/run-note-it <comando>` durante o desenvolvimento.

Atalhos dentro de uma nota: `Ctrl+N` cria outra nota, `Ctrl+W` fecha a atual, `Ctrl+=` / `Ctrl+-` /
`Ctrl+0` controlam o zoom, `Ctrl+Shift+M` recolhe ou expande, `Ctrl+K` busca em todas as notas,
`Ctrl+F` localiza dentro da nota e `Ctrl+H` localiza e substitui. A nota também aceita
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

A Fase 4.0C (Headless CLI Foundation) está completa. A CLI headless `noteit` compartilha `noteit-core` com o aplicativo gráfico `note-it`. As próximas subfases da Fase 4 (Read API, Write API, Machine Interface/JSON e TUI interativa) estão planejadas no [roadmap](docs/roadmap.md).

## Licença

Distribuído sob a [Licença MIT](LICENSE).
