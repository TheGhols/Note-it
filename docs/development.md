# Guia de desenvolvimento

## Pré-requisitos de compilação

Certifique-se de que todos os pacotes de sistema necessários estejam instalados em sua distribuição Linux:

```bash
# Arch Linux
sudo pacman -S --needed gtk4 gtk4-layer-shell webkitgtk-6.0 rust nodejs pnpm pkgconf base-devel dbus
```

`dbus` fornece `dbus-daemon` e `dbus-send`, usados pelo harness de testes isolados para dar à execução um barramento de sessão próprio. Consulte **Executando com um store descartável** abaixo.

## Os três comandos

O repositório tem três entrypoints canônicos para uso local:

```bash
scripts/doctor all    # a máquina tem o necessário?
scripts/check all     # todos os gates
scripts/build.sh      # build release do projeto inteiro
```

Não há uma segunda lista de comandos para manter sincronizada — os gates são os
mesmos —, mas **o CI não executa estes três comandos**. Ele reutiliza
`scripts/doctor` por domínio e invoca os estágios de `scripts/check` um a um, de
propósito: um step por gate é o que faz um run vermelho apontar exatamente o que
quebrou, coisa que um único `scripts/check all` perderia. O que o workflow roda,
step a step:

| Job | Comandos |
| --- | --- |
| Rust Checks & Tests | `scripts/doctor rust`, depois `scripts/check` com `rust-format`, `rust-check`, `rust-clippy`, `core-boundary`, `cli-boundary`, `mcp-boundary`, `core-tests`, `cli-tests`, `mcp-tests`, `workspace-tests` |
| Frontend Checks & Tests | `scripts/doctor frontend`, depois `scripts/check` com `frontend-install`, `frontend-lint`, `frontend-test`, `frontend-build` |

`scripts/build.sh` **não** faz parte do CI: o build release é entrypoint local. O
que o CI compila, ele compila pelos próprios estágios (`rust-check` e as suítes de
teste) e pelo `frontend-build`.

Os três funcionam de qualquer diretório — cada um resolve a raiz do repositório
a partir do próprio caminho, então `cd /tmp && /caminho/Note-it/scripts/check
rust` faz o que se espera.

### `scripts/doctor` — o ambiente

```bash
scripts/doctor          # equivale a `all`
scripts/doctor rust
scripts/doctor frontend
scripts/doctor all
```

Diagnóstico **somente leitura**. Verifica presença e versão do que o build
realmente precisa e diz o que falta. Não instala nada, não usa `sudo`, não
altera PATH, dotfiles, configuração do Git nem gerenciador de pacotes:
instalar pacotes de sistema é decisão de quem opera a máquina, e no CI é
responsabilidade do workflow.

O que ele verifica:

| Modo | Verificações |
| --- | --- |
| `rust` | `bash`, `git`, `cargo`, `rustc`, `pkg-config`; módulos `gtk4`, `gtk4-layer-shell-0`, `webkitgtk-6.0`; `dbus-daemon` e `dbus-send` |
| `frontend` | `node` e `pnpm`, com a versão de cada um |

A versão mínima do Rust é lida de `rust-version` no `Cargo.toml` — o doctor não
tem opinião própria sobre a toolchain. Uma versão anterior à declarada é
**erro**. Para `node` e `pnpm` o projeto não declara mínimo, então ausência é
erro e uma versão atrás da que o CI usa é apenas aviso.

Ao contrário do `check`, o doctor **não** para no primeiro problema: ele roda
todas as verificações e o resumo final é o veredito, para você instalar tudo o
que falta de uma vez.

Códigos de saída: `0` ambiente pronto · `1` requisito ausente ou incompatível ·
`2` uso inválido.

### `scripts/check` — os gates

```bash
scripts/check           # equivale a `all`
scripts/check rust
scripts/check frontend
scripts/check all
scripts/check <estágio>
scripts/check --help    # lista os estágios
```

Estágios, e o comando exato que cada um executa:

| Estágio | Comando |
| --- | --- |
| `rust-format` | `cargo fmt --all -- --check` |
| `rust-check` | `cargo check --workspace` |
| `rust-clippy` | `cargo clippy --workspace --all-targets --all-features -- -D warnings` |
| `core-boundary` | `scripts/check-core-boundary` |
| `cli-boundary` | `scripts/check-cli-boundary` |
| `mcp-boundary` | `scripts/check-mcp-boundary` |
| `core-tests` | `env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noteit-core` |
| `cli-tests` | `env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS cargo test -p noteit-cli` |
| `mcp-tests` | `env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS cargo test -p noteit-mcp` |
| `workspace-tests` | `cargo test --workspace` |
| `frontend-install` | `pnpm install --frozen-lockfile`, em `ui/` |
| `frontend-lint` | `pnpm run lint`, em `ui/` |
| `frontend-test` | `pnpm run test`, em `ui/` |
| `frontend-build` | `pnpm run build`, em `ui/` |

`core-tests`, `cli-tests` e `mcp-tests` não são redundantes com
`workspace-tests`, embora repitam testes: eles provam uma propriedade diferente
— que o Core, a CLI e o servidor MCP ainda funcionam **sem display, sem
compositor e sem barramento de sessão**. Rodá-los dentro da sessão ambiente não
provaria nada, por isso as variáveis são removidas. Para o MCP isso não é um
detalhe: é exatamente o ambiente em que um host o inicia.

O frontend usa pnpm e só pnpm. Não há fallback para npm, yarn ou bun: um
gerenciador diferente resolve uma árvore diferente, e um build que trocou de
dependências em silêncio é pior que um build que parou. Sem pnpm, o estágio
falha com saída `1`.

**Fail-closed.** O primeiro estágio que falhar interrompe a execução, e o código
de saída do `check` é o código daquele estágio. Nenhum erro é convertido em
sucesso, nenhum gate é amaciado com `|| true`. Uso inválido sai com `2` sem
executar nada.

**Isolamento.** `scripts/test-isolation` **não** é invocado aqui: ele já roda
dentro de `cargo test --workspace`, por `tests/isolation.rs`, e executá-lo
duas vezes só custaria tempo. Numa sessão gráfica, a metade de fidelidade desse
harness **abre brevemente uma janela real do Note-it**, apontada o tempo todo
para um store descartável em um barramento próprio — comportamento esperado e
descrito em **O teste de regressão**, mais abaixo.

**Artefatos.** Os artefatos do projeto vão para onde o Git já os ignora:
`target/`, `ui/node_modules/` e `ui/dist/`. Nenhum gate toca o store real de
notas, instala binário em `~/.local/bin`, altera o PATH, usa `sudo`, executa
gerenciador de pacotes do sistema ou cria hook de Git.

Isso não é o mesmo que prometer que nada é escrito fora do repositório. Cargo e
pnpm usam a infraestrutura normal de desenvolvimento — o registry e os caches
do Cargo, o store do pnpm — como fariam em qualquer outro projeto. É esperado, é
compartilhado com o resto da máquina e não tem relação com o store de notas nem
com instalar o Note-it.

### `scripts/build.sh` — o build

```bash
scripts/build.sh
```

Compila o frontend com `pnpm install --frozen-lockfile` seguido de
`pnpm run build`, depois o workspace Rust inteiro com
`cargo build --release --workspace`, e por fim confere que
`target/release/note-it` e `target/release/noteit` existem e são executáveis —
não anuncia sucesso sem olhar. Constrói e **não instala**: nada vai para
`~/.local/bin`, nada entra no PATH.

## Compilando partes isoladas

Quando quiser menos que o build completo:

```bash
cargo build --workspace
cargo build -p note-it      # Adaptador da GUI desktop
cargo build -p noteit-cli   # Adaptador da CLI headless (binário: noteit)
cargo build -p noteit-mcp   # Adaptador MCP local por stdio (binário: noteit-mcp)
```

`scripts/build.sh` só diz `pronto` depois de conferir que os três binários
existem e são executáveis: `target/release/note-it`, `target/release/noteit` e
`target/release/noteit-mcp`.

O crate dedicado `noteit-core` define o limite do domínio e da persistência. `noteit-cli` é o adaptador da CLI headless e `noteit-mcp` é o adaptador MCP local. Os três devem continuar utilizáveis sem GTK, GDK, WebKitGTK, layer-shell, Wayland, Niri ou uma sessão gráfica. `scripts/check-core-boundary`, `scripts/check-cli-boundary` e `scripts/check-mcp-boundary` verificam suas árvores de dependências do Cargo em busca de bibliotecas de desktop proibidas; de forma independente, a compilação impede que o código-fonte importe bibliotecas não declaradas em seus manifestos.

`scripts/check-mcp-boundary` verifica mais do que a árvore de dependências,
porque o servidor MCP tem mais coisas a não fazer: nenhuma pilha HTTP, TLS,
OAuth, SSE ou WebSocket; nenhum banco de dados nem watcher; nenhuma abertura
direta de arquivo, travessia de diretório ou processo filho no `noteit-mcp/src`;
nenhuma escrita em stdout, que pertence ao protocolo; e — a regra central da
fase — exatamente um lugar onde uma mutação de nota existente é construída, com
uma `revision` obrigatória e nunca opcional. Ver `docs/mcp.md`.

## Executando com um store descartável

Qualquer execução experimental ou de integração deve passar pelo auxiliar de isolamento, em vez de um conjunto escrito à mão de variáveis ​​de ambiente:

```bash
scripts/note-it-isolated                          # árvore descartável, removida ao sair
scripts/note-it-isolated --keep                   # mantém a árvore para inspeção
scripts/note-it-isolated -- new                   # repassa argumentos ao note-it

# Sessão que sobrevive a um comando, como exige um teste de instância única:
scripts/note-it-isolated --root /tmp/t -- --background &
scripts/note-it-isolated --root /tmp/t -- new     # alcança a mesma instância
scripts/note-it-isolated --root /tmp/t --verify   # confirma o uso do barramento privado
scripts/note-it-isolated --root /tmp/t --stop     # encerra a instância e o barramento
```

### Isolar XDG não é suficiente

Note-it é uma `GApplication` de instância única, e essa exclusividade é definida por um nome bem conhecido no **barramento de sessão**. O segundo processo iniciado encontra o nome já ocupado, entrega sua linha de comando ao proprietário por D-Bus e sai; o proprietário então executa o trabalho no store com o qual *ele* foi iniciado.

Portanto, substituir as quatro variáveis ​​XDG configura apenas o processo que o auxiliar inicia. **Se um daemon Note-it já estiver rodando no barramento de sessão real, esse processo nunca abre um store**: ele encaminha o comando e sai, e o daemon real grava no store real. O isolamento XDG é real e completamente irrelevante.

Isso não é hipotético. Durante os testes físicos da Fase 3.7, um daemon já estava em execução, todos os comandos isolados foram encaminhados para ele e uma nota de teste foi criada no diretório de notas do próprio usuário. A Fase 3.7R é a solução.

O auxiliar, portanto, isola **ambos**:

- **XDG** — todos os quatro `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME` e `XDG_CACHE_HOME`, definidos juntos. Substituir apenas alguns deixa o resto resolvido para o store real.
- **D-Bus** — um `dbus-daemon` privado próprio, com `DBUS_SESSION_BUS_ADDRESS` apontando para ele e `DBUS_STARTER_ADDRESS`/`DBUS_STARTER_BUS_TYPE` limpo para que GIO não possa retornar à sessão real. Nesse barramento, o nome conhecido não tem dono, então o processo isolado se torna a instância primária e faz seu próprio trabalho em seu próprio store.

O verdadeiro daemon nunca precisa ser interrompido e nunca percebe.

`XDG_RUNTIME_DIR` é deliberadamente **não** substituído: `WAYLAND_DISPLAY` é resolvido dentro dele, portanto, substituí-lo interromperia a conexão do monitor. A configuração `DBUS_SESSION_BUS_ADDRESS` é o que decide o barramento e sempre vence o soquete do diretório de tempo de execução.

O lease de escrita e o soquete de controle residem no mesmo diretório de runtime, mas são separados por store: cada store recebe seu próprio diretório de coordenação, nomeado com um resumo do caminho de suas notas. Assim, uma instância isolada e a real nunca disputam o mesmo lease nem bloqueiam uma à outra. Ao final resta apenas o diretório do store criado pelo teste, e os dois harnesses removem exatamente esse diretório na saída. Eles o identificam pelo arquivo marcador que o Note-it grava dentro dele com o nome do store atendido, de modo que o diretório do store real nunca possa ser tocado. Após qualquer execução isolada, `find "$XDG_RUNTIME_DIR/note-it" -mindepth 1` não deve listar nada além do próprio store real.

### Falha de modo seguro (fail-closed)

Cada verificação é executada *antes* de Note-it ser iniciado e não há caminho que retorne a "bem, pelo menos XDG está isolado":

| saída | significado |
| --- | --- |
| 90 | um diretório configurado é, ou fica dentro, de um diretório base XDG real ou do diretório inicial |
| 91 | nenhum binário `note-it`; execute `cargo build` |
| 92 | o barramento particular não pôde ser iniciado, não pôde ser alcançado ou acabou sendo o verdadeiro |
| 93 | o processo lançado não carrega o ambiente isolado |

A saída 93 é lida do kernel: o processo é iniciado, `/proc/<pid>/environ` é verificado quanto às quatro variáveis ​​XDG e ao endereço do barramento privado, e o processo é encerrado se algum deles não for o isolado.

### Sessões persistentes

Com `--root DIR` o barramento privado é registrado em `DIR/session` e **reutilizado** por cada invocação posterior com o mesmo nome `DIR`, portanto, um daemon iniciado por um comando e um `new` enviado pelo próximo pousam na mesma instância. Termine com `--stop`, que encerra a instância isolada em seu próprio barramento e interrompe esse barramento; um `--root` fornecido pelo chamador nunca é excluído. Sem `--root`, tudo é destruído quando o comando retorna.

### O teste de regressão

`scripts/test-isolation` reproduz o incidente da Fase 3.7 e comprova que ele não pode ocorrer: inicia uma sessão própria — barramento, store e, quando há um display, um daemon `note-it --background` real que possui o nome conhecido —, registra fingerprints do store até o nanossegundo, executa o harness e verifica se a nota foi criada apenas no store descartável e se nada mudou no store do ambiente. Ele faz parte de `cargo test` por meio de `tests/isolation.rs` e requer `dbus-daemon` e `dbus-send`. A metade que usa o daemon é ignorada, com aviso explícito, quando não há display.

Executá-lo localmente abrirá brevemente uma janela de notas real: esse é o ponto da metade da fidelidade, e ela está apontada para um store descartável o tempo todo.

### Testar o que só um terminal sabe responder

`noteit-cli/tests/presentation.rs` cobre a apresentação e a política de ANSI da Fase 4.0G no binário
real. Quase tudo ali é decidido por perguntas que nenhuma variável de ambiente responde — se o que
está na saída padrão é um terminal e quantas colunas ele tem —, e a CLI recusa, de propósito, acreditar
em uma variável sobre isso. Então a suíte fornece a coisa real: abre um pseudoterminal de tamanho
declarado, aponta a saída padrão do processo para ele, mantém a saída de erro em um cano e lê de volta
o que foi escrito. É assim que "um terminal largo recebe o logotipo", "uma janela de 20 colunas recebe
duas linhas" e "a saída de erro redirecionada não recebe cor porque a saída padrão é um terminal" viram
afirmações em vez de observações.

O pseudoterminal fica na própria suíte, e não no harness compartilhado: é a única que precisa de um, e
um auxiliar compilado em suítes que nunca o chamam é código morto em todas elas. Duas armadilhas que já
custaram tempo: enquanto o `Command` não é descartado, o processo de teste ainda segura uma ponta de
escrita e a leitura nunca vê fim de arquivo; e um terminal entrega `\r\n` onde a CLI escreveu `\n`, o que
é obra do terminal e não da CLI.

A forma da tela — as três larguras, a escolha de frase, as duas cores — também é testada sem terminal
nenhum, direto sobre `OutputContext`, porque as capacidades são um valor. As duas metades se cobrem: a
unitária percorre cada largura de 1 a 200, a de processo prova que o binário real chega às mesmas
conclusões.

### Medir a pesquisa em vez de adivinhá-la

A afirmação de que Note-it não precisa de índice de pesquisa é um teste, não uma memória:

```bash
cargo test --release searching_a_thousand_notes -- --nocapture
```

Ele cria mil notas em um diretório temporário, executa quatro consultas - uma que corresponde a algumas notas, uma que corresponde a todas elas, uma que não corresponde a nenhuma, uma com acentos - de ponta a ponta por meio de listagem, leitura, dobra, correspondência e trechos, imprime cada tempo e afirma que os tempos de modificação das notas não mudaram. Na máquina de desenvolvimento, toda a varredura leva cerca de 26 a 40 ms por consulta no lançamento e menos de 200 ms na depuração.

A Fase 3.8R quase dobrou dos 18–20 ms que era, e a causa não é o limite de varredura removido: ela está ordenando pelo próprio `updated_at` de cada nota, o que significa abrir o cabeçalho de cada nota e analisá-lo. Cerca de metade do tempo adicionado são as leituras e a outra metade é o YAML. Ele compra "mais recente", significando a mesma coisa em todos os lugares - uma nota repintada não é uma nota escrita - e 40 ms ainda está bem dentro dos 120 ms que a paleta espera antes de perguntar.

Esse é o número em que o ADR-027 se baseia. Se deixar de ser confortável, a evidência para adicionar um índice estará na saída do teste, que é onde deveria estar - e não em um palpite.

### Inspecionando um backup

Um snapshot é um diretório de arquivos comuns, e é por isso que é um:

```bash
ls ~/.local/share/note-it/backups/
cat ~/.local/share/note-it/backups/*/manifest.json
diff -r ~/.local/share/note-it/backups/<data>/notes ~/.local/share/note-it/notes
```

O procedimento de recuperação — incluindo a recuperação de uma única nota em vez de todo o store — está em [docs/storage.md](storage.md#recuperando-se-de-um-instantâneo). É `cp`, com o aplicativo fechado. Não há restauração com um clique no aplicativo e `a_snapshot_round_trips_into_a_fresh_isolated_store` é o que prova que o procedimento funciona: ele copia um instantâneo em uma árvore XDG vazia exatamente dessa maneira e abre o resultado.

Para exercer a regra das vinte e quatro horas contra um daemon em execução sem esperar um dia, envelheça o instantâneo mais recente — o próprio registro do store de quando foi feito o último backup é o manifesto desse instantâneo — e reinicie:

```bash
scripts/note-it-isolated --root /tmp/t --stop
# renomeie o diretório do instantâneo e defina created_at no manifest.json como > 24 h atrás
scripts/note-it-isolated --root /tmp/t -- --background &
scripts/note-it-isolated --root /tmp/t -- new     # a próxima alteração cria outro instantâneo
```

### Tabela de composição do GTK

Um `XDG_CACHE_HOME` frio faz o GTK reconstruir sua tabela de composição, o que produz a explosão de aviso `Can't handle >16bit keyvals` única descrita em ADR-006. É esperado na primeira corrida contra uma árvore nova e desaparece na próxima.
