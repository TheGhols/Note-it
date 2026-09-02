# Contribuindo para Note-it

Obrigado pelo seu interesse em contribuir para Note-it!

## Princípios orientadores

1. **Local-First e privacidade:** nunca introduza telemetria, dependência obrigatória de sincronização em nuvem, rastreamento ou dependências de rede no fluxo principal de notas.
2. **Wayland e desempenho em primeiro lugar:** mantenha alta responsividade, baixo uso ocioso de CPU/RAM e o uso correto do protocolo Layer Shell.
3. **WYSIWYG verdadeiro:** preserve Markdown limpo no disco enquanto apresenta texto formatado no editor, sem marcadores de sintaxe bruta interferindo na edição.
4. **Código e commits limpos:** mantenha os pull requests focados, escreva testes abrangentes e use mensagens de commit convencionais.

## Fluxo de trabalho de desenvolvimento

Três comandos, nesta ordem, para uso local. Os gates que eles rodam são os mesmos
do CI — não há uma segunda lista para manter em dia —, mas o CI não executa estes
três comandos: ele chama `scripts/doctor` por domínio e os estágios de
`scripts/check` um a um, e o build release fica só aqui.

```bash
scripts/doctor all    # a máquina tem o necessário?
scripts/check all     # todos os gates do repositório
scripts/build.sh      # build release do projeto inteiro
```

1. **`scripts/doctor all`** diagnostica o ambiente e diz o que falta. É somente
   leitura: não instala nada, não usa `sudo` e não altera a máquina. Instalar os
   pacotes de sistema continua sendo com você — consulte
   [docs/development.md](docs/development.md) para a linha de comando da sua
   distribuição.
2. **`scripts/check all`** roda todos os gates: formato, `cargo check`, Clippy,
   os dois boundary scripts, as suítes headless do Core e da CLI, os testes do
   workspace e os quatro gates do frontend. Ele para no primeiro que falhar e
   devolve o código daquele gate. Para rodar um só, use o nome do estágio —
   `scripts/check rust-clippy`, `scripts/check frontend-test`;
   `scripts/check --help` lista todos. Sem argumento, equivale a
   `scripts/check all`.
3. **`scripts/build.sh`** compila o frontend com o lockfile congelado e o
   workspace Rust em release, e confere que `target/release/note-it` e
   `target/release/noteit` existem. Ele constrói e não instala.

Em uma sessão gráfica, `scripts/check all` abre brevemente uma janela real do
Note-it: é a metade de fidelidade do harness de isolamento, apontada o tempo
todo para um store descartável em um barramento próprio. Nenhuma nota real é
tocada.

Nunca inclua caminhos, credenciais ou arquivos de configuração pessoais
específicos do desenvolvedor em commits.

## Diretrizes de commits

Use mensagens de commit profissionais e claras seguindo os commits convencionais:

- `feat: adicionar persistência de notas em Markdown`
- `fix: impedir perda de foco na transição para overlay`
- `test: adicionar cobertura de ida e volta (round-trip) do Markdown`
- `docs: atualizar exemplos de atalhos do Niri`
- `chore: atualizar dependências`
