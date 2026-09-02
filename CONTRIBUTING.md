# Contribuindo para Note-it

Obrigado pelo seu interesse em contribuir para Note-it!

## Princípios orientadores

1. **Local-First e privacidade:** nunca introduza telemetria, dependência obrigatória de sincronização em nuvem, rastreamento ou dependências de rede no fluxo principal de notas.
2. **Wayland e desempenho em primeiro lugar:** mantenha alta responsividade, baixo uso ocioso de CPU/RAM e o uso correto do protocolo Layer Shell.
3. **WYSIWYG verdadeiro:** preserve Markdown limpo no disco enquanto apresenta texto formatado no editor, sem marcadores de sintaxe bruta interferindo na edição.
4. **Código e commits limpos:** mantenha os pull requests focados, escreva testes abrangentes e use mensagens de commit convencionais.

## Fluxo de trabalho de desenvolvimento

1. Certifique-se de que todas as dependências do sistema estejam instaladas:
   - `gtk4`, `gtk4-layer-shell`, `webkitgtk-6.0`
   - toolchain Rust
   - Node.js & pnpm
2. Execute formatadores e linters antes de enviar alterações:
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   cd ui && pnpm lint && pnpm test && pnpm build
   ```
3. Nunca inclua caminhos, credenciais ou arquivos de configuração pessoais específicos do desenvolvedor em commits.

## Diretrizes de commits

Use mensagens de commit profissionais e claras seguindo os commits convencionais:

- `feat: adicionar persistência de notas em Markdown`
- `fix: impedir perda de foco na transição para overlay`
- `test: adicionar cobertura de ida e volta (round-trip) do Markdown`
- `docs: atualizar exemplos de atalhos do Niri`
- `chore: atualizar dependências`
