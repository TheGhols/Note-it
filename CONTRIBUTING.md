# Contribuindo com o Note-it

Obrigado pelo seu interesse em contribuir com o Note-it!

## Princípios orientadores

1. **Local-First e privacidade:** Nunca introduza telemetria, dependência obrigatória de serviços em nuvem, rastreamento ou dependências de rede no fluxo principal de notas.
2. **Wayland e desempenho em primeiro lugar:** Mantenha alta responsividade, consumo mínimo de CPU/RAM em repouso e uso correto do protocolo Layer Shell.
3. **WYSIWYG verdadeiro:** Preserve Markdown limpo em disco enquanto apresenta texto formatado no editor, sem marcadores de sintaxe bruta interferindo na edição.
4. **Código e commits limpos:** Mantenha pull requests focados, escreva testes abrangentes e siga o padrão Conventional Commits.

## Fluxo de trabalho de desenvolvimento

1. Certifique-se de que todas as dependências do sistema estejam instaladas:
   - `gtk4`, `gtk4-layer-shell`, `webkitgtk-6.0`
   - Toolchain Rust
   - Node.js & pnpm
2. Execute os formatadores e linters antes de submeter alterações:
   ```bash
   cargo fmt --check
   cargo clippy -- -D warnings
   cargo test
   cd ui && pnpm lint && pnpm test && pnpm build
   ```
3. Nunca inclua caminhos locais de desenvolvimento, credenciais ou arquivos de configuração pessoal nos commits.

## Diretrizes de commit

Utilize mensagens de commit profissionais e claras seguindo o padrão Conventional Commits:

- `feat: adicionar persistência de notas em Markdown`
- `fix: impedir perda de foco na transição para overlay`
- `test: adicionar cobertura de ida e volta (round-trip) do Markdown`
- `docs: atualizar exemplos de atalhos do Niri`
- `chore: atualizar dependências`
