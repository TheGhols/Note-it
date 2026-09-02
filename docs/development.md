# Guia de desenvolvimento

## Pré-requisitos de compilação

Certifique-se de que todos os pacotes de sistema necessários estejam instalados em sua distribuição Linux:

```bash
# Arch Linux
sudo pacman -S --needed gtk4 gtk4-layer-shell webkitgtk-6.0 rust nodejs pnpm pkgconf base-devel dbus
```

`dbus` fornece `dbus-daemon` e `dbus-send`, usados pelo harness de testes isolados para dar à execução um barramento de sessão próprio. Consulte **Executando com um store descartável** abaixo.

## Compilando o projeto

1. **Crie os ativos de front-end:**
   ```bash
   cd ui
   pnpm install
   pnpm build
   cd ..
   ```

2. **Criar binários Rust:**
   ```bash
   cargo build --workspace
   # Ou individualmente:
   cargo build -p note-it      # Adaptador da GUI desktop
   cargo build -p noteit-cli   # Adaptador da CLI headless (binário: noteit)
   ```

3. **Executar testes:**
   ```bash
   cargo test --workspace
   env -u DISPLAY -u WAYLAND_DISPLAY cargo test -p noteit-core
   env -u DISPLAY -u WAYLAND_DISPLAY -u DBUS_SESSION_BUS_ADDRESS cargo test -p noteit-cli
   scripts/check-core-boundary
   scripts/check-cli-boundary
   cd ui && pnpm test
   ```

4. **Verificações de qualidade de código:**
   ```bash
   cargo fmt --all -- --check
   cargo check --workspace
   cargo clippy --workspace --all-targets --all-features -- -D warnings
   cd ui && pnpm lint
   ```

O crate dedicado `noteit-core` define o limite do domínio e da persistência. O crate `noteit-cli` é o adaptador da CLI headless. Ambos devem continuar utilizáveis sem GTK, GDK, WebKitGTK, layer-shell, Wayland, Niri ou uma sessão gráfica. `scripts/check-core-boundary` e `scripts/check-cli-boundary` verificam suas árvores de dependências do Cargo em busca de bibliotecas de desktop proibidas; de forma independente, a compilação impede que o código-fonte do Core e da CLI importe bibliotecas não declaradas em seus manifestos.

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

Portanto, substituir as quatro variáveis ​​XDG configura apenas o processo que o auxiliar inicia. **Se um daemon Note-it já estiver rodando no barramento de sessão real, esse processo nunca abre um armazenamento**: ele encaminha o comando e sai, e o daemon real grava no armazenamento real. O isolamento XDG é real e completamente irrelevante.

Isso não é hipotético. Durante os testes físicos da Fase 3.7, um daemon já estava em execução, todos os comandos isolados foram encaminhados para ele e uma nota de teste foi criada no diretório de notas do próprio usuário. A Fase 3.7R é a solução.

O auxiliar, portanto, isola **ambos**:

- **XDG** — todos os quatro `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME` e `XDG_CACHE_HOME`, definidos juntos. Substituir apenas alguns deixa o resto resolvido para o armazenamento real.
- **D-Bus** — um `dbus-daemon` privado próprio, com `DBUS_SESSION_BUS_ADDRESS` apontando para ele e `DBUS_STARTER_ADDRESS`/`DBUS_STARTER_BUS_TYPE` limpo para que GIO não possa retornar à sessão real. Nesse barramento, o nome conhecido não tem dono, então o processo isolado se torna a instância primária e faz seu próprio trabalho em seu próprio armazenamento.

O verdadeiro daemon nunca precisa ser interrompido e nunca percebe.

`XDG_RUNTIME_DIR` é deliberadamente **não** substituído: `WAYLAND_DISPLAY` é resolvido dentro dele, portanto, substituí-lo interromperia a conexão do monitor. A configuração `DBUS_SESSION_BUS_ADDRESS` é o que decide o barramento e sempre vence o soquete do diretório de tempo de execução.

O lease de escrita e o soquete de controle residem no mesmo diretório de runtime, mas são separados por store: cada store recebe seu próprio diretório de coordenação, nomeado com um resumo do caminho de suas notas. Assim, uma instância isolada e a real nunca disputam o mesmo lease nem bloqueiam uma à outra. Ao final resta apenas o diretório do store criado pelo teste, e os dois harnesses removem exatamente esse diretório na saída. Eles o identificam pelo arquivo marcador que o Note-it grava dentro dele com o nome do store atendido, de modo que o diretório do store real nunca possa ser tocado. Após qualquer execução isolada, `find "$XDG_RUNTIME_DIR/note-it" -mindepth 1` não deve listar nada além do próprio store real.

### Falha fechada

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

Para exercer a regra das vinte e quatro horas contra um daemon em execução sem esperar um dia, envelheça o instantâneo mais recente — o próprio registro do armazenamento de quando foi feito o último backup é o manifesto desse instantâneo — e reinicie:

```bash
scripts/note-it-isolated --root /tmp/t --stop
# renomeie o diretório do instantâneo e defina created_at no manifest.json como > 24 h atrás
scripts/note-it-isolated --root /tmp/t -- --background &
scripts/note-it-isolated --root /tmp/t -- new     # a próxima alteração cria outro instantâneo
```

### Tabela de composição do GTK

Um `XDG_CACHE_HOME` frio faz o GTK reconstruir sua tabela de composição, o que produz a explosão de aviso `Can't handle >16bit keyvals` única descrita em ADR-006. É esperado na primeira corrida contra uma árvore nova e desaparece na próxima.
