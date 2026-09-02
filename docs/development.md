# Guia de desenvolvimento

## Pré-requisitos de compilação

Certifique-se de que todos os pacotes de sistema necessários estejam instalados em sua distribuição Linux:

```bash
# Arch Linux
sudo pacman -S --needed gtk4 gtk4-layer-shell webkitgtk-6.0 rust nodejs pnpm pkgconf base-devel dbus
```

O pacote `dbus` fornece `dbus-daemon` e `dbus-send`, necessários para que o harness de testes isolados crie um barramento de sessão exclusivo para os testes. Consulte a seção **Executando com um store descartável** abaixo.

## Compilando o projeto

1. **Compilar os assets do frontend:**
   ```bash
   cd ui
   pnpm install
   pnpm build
   cd ..
   ```

2. **Compilar os binários em Rust:**
   ```bash
   cargo build --workspace
   # Ou individualmente:
   cargo build -p note-it      # Adaptador GUI desktop
   cargo build -p noteit-cli   # Adaptador CLI headless (binário: noteit)
   ```

3. **Executar os testes:**
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

A crate dedicada `noteit-core` define a fronteira de domínio e persistência. A crate `noteit-cli` é o adaptador de CLI headless. Ambas devem permanecer utilizáveis sem GTK, GDK, WebKitGTK, layer-shell, Wayland, Niri ou sessões gráficas. Os scripts `scripts/check-core-boundary` e `scripts/check-cli-boundary` verificam as árvores de dependência do Cargo para impedir bibliotecas desktop proibidas; a compilação previne de forma independente que o código-fonte do Core e da CLI importe bibliotecas não declaradas em seus manifestos.

## Executando com um store descartável

Qualquer execução experimental ou teste de integração deve utilizar o utilitário de isolamento em vez de variáveis de ambiente configuradas manualmente:

```bash
scripts/note-it-isolated                          # árvore descartável, removida ao sair
scripts/note-it-isolated --keep                   # mantém a árvore para inspeção
scripts/note-it-isolated -- new                   # repassa argumentos para o note-it

# Sessão que sobrevive a um comando, como exige um teste de instância única:
scripts/note-it-isolated --root /tmp/t -- --background &
scripts/note-it-isolated --root /tmp/t -- new     # atinge a mesma instância
scripts/note-it-isolated --root /tmp/t --verify   # valida conexão no barramento privado
scripts/note-it-isolated --root /tmp/t --stop     # encerra a instância e para o barramento
```

### Isolar XDG não é suficiente

Note-it é uma `GApplication` de instância única, e a instância única é identificada por um nome bem conhecido no **barramento de sessão**. O segundo processo que inicia detecta que o nome já possui proprietário, repassa sua linha de comando ao proprietário via D-Bus e encerra — e o proprietário realiza o trabalho no store com o qual *ele* foi iniciado.

Dessa forma, substituir apenas as quatro variáveis XDG afeta exclusivamente o processo lançado pelo utilitário. **Se um daemon Note-it já estiver em execução no barramento de sessão real, o novo processo nunca abre nenhum store**: ele apenas encaminha o comando e encerra, e o daemon real grava no store real do usuário. O isolamento XDG é autêntico, mas insuficiente.

Isso não é hipotético: durante os testes físicos da Fase 3.7, havia um daemon em execução no sistema, cada comando isolado foi repassado a ele e uma nota de teste foi criada no diretório de notas real do usuário. A Fase 3.7R introduziu a correção definitiva.

O utilitário, portanto, isola **ambos**:

- **XDG** — as quatro variáveis `XDG_CONFIG_HOME`, `XDG_DATA_HOME`, `XDG_STATE_HOME` e `XDG_CACHE_HOME` definidas em conjunto. Sobrescrever apenas parte delas faria o restante apontar para o store real.
- **D-Bus** — um `dbus-daemon` privado exclusivo, com `DBUS_SESSION_BUS_ADDRESS` apontando para ele e `DBUS_STARTER_ADDRESS`/`DBUS_STARTER_BUS_TYPE` limpos para que a biblioteca GIO não recorra à sessão real. Nesse barramento o nome bem conhecido está livre, permitindo que o processo isolado se torne a instância primária e execute suas operações em seu próprio store isolado.

O daemon real do usuário nunca precisa ser finalizado e não sofre interferências.

`XDG_RUNTIME_DIR` deliberadamente **não** é sobrescrito: `WAYLAND_DISPLAY` é resolvido dentro dele, de modo que alterá-lo quebraria a conexão gráfica com o compositor. A definição de `DBUS_SESSION_BUS_ADDRESS` determina o barramento e sempre prevalece sobre o socket do diretório de runtime.

O lease de escritor e o socket de controle residem no mesmo diretório de runtime, mantidos isolados por outro mecanismo: cada store recebe seu próprio diretório de coordenação, nomeado a partir do digest do caminho de suas notas. Assim, uma instância isolada e a real nunca disputam o mesmo lease, e nenhuma bloqueia a outra. Os resíduos deixados após a execução pertencem ao store sintético do teste, e ambos os harnesses removem exatamente esse diretório ao finalizar — identificado pelo arquivo marcador que o Note-it grava em seu interior indicando o store atendido, impedindo qualquer toque no diretório do store real. Após execuções isoladas, `find "$XDG_RUNTIME_DIR/note-it" -mindepth 1` deve listar exclusivamente o diretório do store real.

### Falha de modo seguro (fail-closed)

Todas as verificações são executadas *antes* que o Note-it seja iniciado, e não existe fallback tolerante do tipo "bem, ao menos o XDG foi isolado":

| exit | significado |
| --- | --- |
| 90 | um diretório configurado é, ou está dentro de, um diretório base XDG real ou da home do usuário |
| 91 | binário `note-it` não encontrado; execute `cargo build` |
| 92 | o barramento privado não pôde ser iniciado, ficou inacessível ou revelou-se o barramento real |
| 93 | o processo lançado não contém o ambiente isolado |

O código de saída 93 é verificado diretamente pelo kernel: o processo é iniciado, `/proc/<pid>/environ` é inspecionado para as quatro variáveis XDG e o endereço do barramento privado, e o processo é finalizado imediatamente caso algum deles não corresponda ao ambiente isolado.

### Sessões persistentes

Com a flag `--root DIR`, o barramento privado é salvo em `DIR/session` e **reutilizado** por cada invocação subsequente com o mesmo `--root DIR`; assim, um daemon iniciado por um comando e um comando `new` enviado pelo próximo atuam na mesma instância. Encerre com `--stop`, que fecha a instância isolada em seu próprio barramento e finaliza o daemon D-Bus; diretórios `--root` fornecidos pelo usuário nunca são excluídos automaticamente. Sem `--root`, todo o ambiente é desmontado assim que o comando finaliza.

### O teste de regressão

O script `scripts/test-isolation` reproduz o cenário da Fase 3.7 e comprova que ele não pode ocorrer: inicializa uma sessão própria — barramento, store e, quando houver display disponível, um daemon real `note-it --background` detendo o nome bem conhecido real —, calcula fingerprints desse store com precisão de nanossegundos, executa o harness contra ele e valida que a nota foi criada exclusivamente no store descartável, sem qualquer alteração no store do ambiente. Ele roda como parte de `cargo test` via `tests/isolation.rs` e requer `dbus-daemon` e `dbus-send`. A parte do daemon é ignorada de forma explícita caso não haja display gráfico disponível.

Executar o teste localmente abrirá brevemente uma janela real de nota: esse é o propósito da validação de fidelidade, apontada para um store descartável durante todo o tempo.

### Medindo a busca em vez de tentar adivinhar

A afirmação de que Note-it não precisa de um índice de busca é comprovada por testes contínuos:

```bash
cargo test --release searching_a_thousand_notes -- --nocapture
```

O teste cria mil notas em um diretório temporário, executa quatro consultas de busca — uma correspondendo a poucas notas, uma correspondendo a todas, uma não correspondendo a nenhuma e uma contendo acentuação — de ponta a ponta através de listagem, leitura, normalização de acentos, comparação e extração de snippets, imprimindo cada tempo e assegurando que os timestamps de modificação das notas não foram alterados. No ambiente de desenvolvimento, a varredura completa consome cerca de 26–40 ms por consulta em modo release e menos de 200 ms em modo debug.

A Fase 3.8R aproximadamente dobrou esse tempo em relação aos 18–20 ms anteriores, sendo a causa a ordenação por `updated_at` de cada nota, o que exige abrir o cabeçalho de todas as notas e processá-lo. Cerca de metade do tempo adicional vem da leitura de E/S e metade do parsing de YAML. Isso garante que "mais recente" signifique a mesma coisa em toda a aplicação — uma nota que apenas mudou de cor não é uma nota editada —, e 40 ms permanece bem abaixo dos 120 ms de debounce que a paleta de busca aguarda antes de enviar a requisição.

Esse é o fundamento empírico da ADR-027. Se esse tempo deixar de ser confortável, a evidência para adoção de índice virá da saída dos testes, onde deve estar — e não de palpites subjetivos.

### Inspecionando um backup

Um snapshot é um diretório com arquivos comuns do sistema de arquivos:

```bash
ls ~/.local/share/note-it/backups/
cat ~/.local/share/note-it/backups/*/manifest.json
diff -r ~/.local/share/note-it/backups/<data>/notes ~/.local/share/note-it/notes
```

O procedimento de recuperação — incluindo a recuperação de uma única nota individual em vez de todo o store — está documentado em [docs/storage.md](storage.md#recuperando-a-partir-de-um-snapshot). O processo resume-se a `cp` com o aplicativo fechado. Não há botão de restauração em um clique na interface gráfica, e o teste `a_snapshot_round_trips_into_a_fresh_isolated_store` comprova o funcionamento do procedimento copiando um snapshot para uma árvore XDG limpa e abrindo o resultado.

Para testar a regra de 24 horas contra um daemon em execução sem ter de esperar um dia, altere a data do snapshot mais recente no manifesto (`manifest.json`) e reinicie:

```bash
scripts/note-it-isolated --root /tmp/t --stop
# renomeie o diretório do snapshot e configure created_at em seu manifest.json para > 24h atrás
scripts/note-it-isolated --root /tmp/t -- --background &
scripts/note-it-isolated --root /tmp/t -- new     # a próxima alteração criará um novo snapshot
```

### Tabela de composição do GTK

Uma árvore `XDG_CACHE_HOME` limpa faz o GTK reconstruir sua tabela de composição de caracteres, gerando o lote único de avisos `Can't handle >16bit keyvals` descrito na ADR-006. Esse comportamento é esperado na primeira inicialização com um cache limpo e não se repete nas execuções seguintes.
