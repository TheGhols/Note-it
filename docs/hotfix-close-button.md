# Hotfix — o botão X não fechava a nota

**Data:** 14/09/2026 · **Versão:** `0.1.1` → `0.1.2` · **Escopo:** um defeito da GUI, encontrado na instalação diária

---

## A. Resumo

| Item | Valor |
| --- | --- |
| **Relato** | clicar no X não fechava a nota; a interface parava de responder logo depois |
| **Causa-raiz** | `saveAndClose` entregava a si mesma a `deferDocumentEdit`; com o documento livre a barreira executa a ação ali mesmo, então a chamada reentrava até estourar a pilha |
| **Correção** | passar o trabalho em vez do chamador — uma função, `ui/src/main.ts` |
| **Regressão** | `ui/tests/close_button.test.ts`, dois casos, ambos verificados falhando no código anterior |
| **WebKitGTK real** | reproduzido e validado — casos A a G |
| **Fase 6** | **NÃO INICIADA** |

---

## B. Relato

> Ao clicar no botão "X" dentro da própria nota/aplicativo, o Note-It não fecha.
> Depois do clique, a interface parece travar ou ficar sem responder normalmente.

Para encerrar o aplicativo foi preciso `pkill -TERM -x note-it`.

Estado medido no início: repositório em `a568cc84`, igual a `origin/main`, árvore
limpa; instalado `note-it 0.1.1.r223.g79332043-1`; nenhum processo `note-it` em
execução.

---

## C. Reprodução

Reproduzido no binário gráfico real, em store isolado e **compositor aninhado**
(`sway` dentro do niri), para que cliques de verdade pudessem ser injetados sem
tocar a sessão do usuário. `scripts/note-it-isolated` garantiu XDG **e** barramento
D-Bus privados — só isolar XDG não basta num `GApplication` de instância única.

| Item | Valor |
| --- | --- |
| Binário | `target/release/note-it`, construído do código com o defeito |
| Ambiente | Wayland, niri 	→ sway aninhado (`wayland-2`), saída 1263x1014 |
| WebKitGTK | `webkitgtk-6.0 2.52.6-1` · GTK `4.22.4-1` |
| Nota | uma, expandida, 360x300 em (100,100), sem edição pendente |

**Observado ao clicar no X:**

- a janela **permanecia visível** — 26 433 pixels de papel antes e depois;
- `is_open` continuava `true` no `state.json`;
- o processo continuava vivo;
- **o host não registrava uma única linha** — nem `Save-and-close failed`, nem
  `Close finalization failed`, nem `Rejected a stale save-and-close`.

O silêncio do host é o achado que orientou tudo: se a mensagem tivesse chegado,
qualquer desfecho — sucesso, id divergente, geração velha, falha de persistência —
teria deixado rastro ou fechado a janela. Nada disso aconteceu, logo a mensagem
**nunca foi enviada**.

**Discriminador decisivo.** `Ctrl+W` falhou exatamente igual, e `Ctrl+W` chama a
mesma `saveAndClose`. Já um clique no botão de menu **abriu o menu** normalmente.
Portanto o defeito não era entrega de clique, hit-testing, sobreposição de painel
nem o host: era a própria `saveAndClose`.

---

## D. Causa-raiz

`ExternalWriteBarrier.defer(action)` tem duas metades:

```ts
public defer(action: () => void): boolean {
  if (!this.active) {
    action();        // documento livre: executa agora
    return false;
  }
  this.queued.push(action);   // documento seguro: enfileira
  return true;
}
```

E `saveAndClose` entregava **a si mesma**:

```ts
if (deferDocumentEdit(() => saveAndClose())) return;
```

Com o documento livre — o caso comum, e o caso do usuário — `defer` chamava
`saveAndClose`, que chamava `defer`, que chamava `saveAndClose`… até a pilha
acabar. O `RangeError: Maximum call stack size exceeded` era lançado **antes** de
`bridge.sendMessage`, então o host nunca recebia `save_and_close`. A janela não
fechava, nada era registrado, e a página ficava sem responder enquanto desenrolava
a recursão — exatamente o que o usuário descreveu.

O caminho segurado também estava quebrado, pelo mesmo motivo: a ação enfileirada
era `saveAndClose`, e ao ser drenada a barreira já estava ociosa, então ela
recursava na liberação.

Os outros sítios de `deferDocumentEdit` — a captura do AutoPaste e a inserção de
imagem — sempre passaram o **trabalho** e ignoraram o retorno, que é a forma
correta. Só o fecho passava o chamador.

---

## E. Correção

`ui/src/main.ts`, uma função. O trabalho vai para dentro da ação:

```ts
deferDocumentEdit(() => {
  if (!activeNoteId || !noteEditor) return;
  const content = noteEditor.getMarkdown();
  noteEditor.cancelPendingSave();
  bridge.sendMessage({
    type: 'save_and_close',
    payload: { id: activeNoteId, content, generation: currentGeneration() },
  });
});
```

Fecha nos dois caminhos: agora, com o documento livre; ou no instante em que a
barreira libera, lendo texto e geração como estão nesse momento.

**Preservado, sem alteração:** autosave, integridade das notas, escrita externa e
sua barreira, geração, janelas múltiplas, D-Bus, atalho global, collapse/expand,
reabertura, cold activation e o processo residente. Nenhuma linha de Rust mudou.

**Contrato do fecho, medido e não alterado.** `AppState` guarda um
`gio::ApplicationHoldGuard` por toda a vida do processo (`src/app.rs`), então o X
fecha **a nota**, não o aplicativo: fechada a última nota, a janela some e o
processo permanece de propósito, para D-Bus e o atalho global. Um `pgrep note-it`
vivo depois de fechar a última nota é o comportamento projetado, não o defeito.

---

## F. Teste de regressão

`ui/tests/close_button.test.ts` — dois casos.

O teste **importa `main.ts`, monta o `index.html` publicado e clica no
`#btn-close` de verdade**, em vez de espelhar a ligação num duplo. Um espelho
teria sido escrito contra a forma já corrigida e passaria durante todo o defeito —
foi exatamente assim que a lacuna do painel de atalhos sobreviveu na 5.0E-GUI.

1. **Documento livre** — um clique produz **exatamente um** `save_and_close`,
   carregando o corpo da nota, e o clique não lança.
2. **Documento segurado** — com uma escrita externa em curso, o clique não envia
   nada; na liberação envia **exatamente um** `save_and_close`, citando a geração
   nova e o conteúdo comprometido.

### Prova negativa

Executados contra o código anterior num worktree isolado em `HEAD` (`a568cc8`),
sem tocar o histórico:

```text
× sends exactly one save_and_close, carrying the note, and does not throw
  → expected [Function] to not throw an error but
    'RangeError: Maximum call stack size exceeded' was thrown

× holds the close for an external write and sends it once on release
  → the close was lost, or was sent more than once: expected [] to have a length of 1
```

Ambos falham pelo motivo esperado e ambos passam com a correção.

---

## G. Validação no Note-It gráfico real

Repetida com a build corrigida, no mesmo compositor aninhado e store isolado.
`papel` é a contagem de pixels de post-it na tela: a janela está lá, ou não está.

| Caso | Cenário | Resultado |
| --- | --- | --- |
| **A** | nota aberta, sem alteração recente | papel 26 497 → **0**, `is_open` 1 → 0 · **PASS** |
| **B** | texto editado e X imediatamente | fechou; `Caso B conteudo nao salvo` **gravado em disco** · **PASS** |
| **C** | nota recolhida (30 px de barra) | papel 2 540 → **0**, conteúdo preservado · **PASS** |
| **D** | nota expandida normal | coberto por A e B · **PASS** |
| **E** | duas notas, fecha uma pelo X | papel 30 694 → 26 497; **só a clicada** fechou · **PASS** |
| **F** | fecha a última nota | janela fecha, **processo permanece** — contrato · **PASS** |
| **G** | escrita externa (`noteit adicionar`) e depois X | escrita comprometida, fechou normalmente, nada sobrescrito · **PASS** |

**Reabertura.** Invocado sem subcomando, o Note-it trouxe de volta a última nota
com `Caso G original` e `linha vinda de fora` — o conteúdo digitado e o escrito de
fora, ambos intactos.

---

## H. Gates

`scripts/check` — **exit 0**, os 24 estágios verdes: `ci-parity`, `rust-format`,
`rust-check`, `rust-clippy`, as sete fronteiras (`core`, `cli`, `mcp`,
`embedding`, `embed`, `tui`, `agent-bridge`), as nove suítes Rust, e
`frontend-install`, `frontend-lint`, `frontend-test`, `frontend-build`.

- Frontend: **63 arquivos, 1343 testes**, todos passando.
- `cargo fmt --check`: limpo.
- `tsc --noEmit`: limpo.

**Nota sobre uma falha encontrada e resolvida no caminho.** Na primeira execução,
`embed-boundary` reprovou apontando
`./packaging/arch/src/Note-it/noteit-embed/src/endpoint.rs`. Não era regressão: é
o `src/` regenerável que o `makepkg` deixou de uma build anterior — ignorado pelo
Git desde `1186224`, mas visível para um verificador que varre o disco. Removido o
diretório regenerável, o gate passou inteiro. O CI, que roda em checkout limpo,
nunca veria isso.

---

## I. Achado relacionado, **não corrigido** neste hotfix

O mesmo mal-entendido sobre `defer` existe em `ui/src/main.ts`, no salvamento de
metadados: ele usa `if (deferDocumentEdit(() => { …envia metadata_changed… })) return;`
e, logo abaixo, envia `metadata_changed` **outra vez**. Com o documento livre,
`defer` já executou a ação e devolveu `false`, então a mensagem sai **duplicada**.

É um defeito distinto, com reprodução e teste próprios, e corrigi-lo aqui
ampliaria o escopo de um hotfix. Fica registrado para ser tratado por si.

---

## J. Integridade dos dados reais

Medido antes e depois de toda a investigação. Nenhum teste tocou o store real:
tudo correu em `scripts/note-it-isolated`, com XDG e barramento D-Bus privados.

| Item | Antes | Depois |
| --- | --- | --- |
| Notas | 42 | **42** |
| Fingerprint | `ef7371a3de5019136f861fd27da7add273b26292f480814b37ae04ecfe239806` | **idêntico** |
| `config.toml` | `2050407911ea9fb37ac55740e7e60961dda8a422fc148c962a8edf00d23ebe36` | **idêntico** |

---

## K. Versão

ADR-062 é explícita e cobre este caso: o formato é `0.MINOR.PATCH`,
**deliberadamente não** SemVer, e *"um bugfix relevante na versão diária é uma
unidade"* — é corrigido, versionado e promovido sozinho. `PATCH` avança enquanto
for menor que 100.

`0.1.1` → **`0.1.2`**, na fonte canônica (`[workspace.package]` do `Cargo.toml`) e
nos dois espelhos exigidos (`ui/package.json`, `_appver` do `PKGBUILD`). Nenhuma
tag criada, nenhuma release publicada.

---

## L. CI

Acompanhado no SHA exato, não no branch.

| Run | SHA | Rust Checks & Tests | Frontend Checks & Tests | Resultado |
| --- | --- | --- | --- | --- |
| `34922859554` | `d85d19dd` — a correção e a versão 0.1.2 | success | success | **success** |
| `34924025336` | `35f1773a` — o `PKGBUILD` fixado | success | success | **success** |
| `34924713072` | `6c2dafd0` — `.gitignore` do clone do makepkg | success | success | **success** |

Nenhum pipeline foi reexecutado: os três passaram na primeira tentativa.

---

## M. Pacote

Construído do commit `d85d19dd` — o SHA que o CI aprovou — e de nenhum
intermediário.

| Item | Valor |
| --- | --- |
| Arquivo | `packaging/arch/note-it-0.1.2.r227.gd85d19dd-1-x86_64.pkg.tar.zst` |
| Versão | `0.1.2.r227.gd85d19dd-1` |
| Tamanho | 8 428 020 bytes (8,1 MiB) |
| SHA256 | `27cc88c4cbca0d5f8f04ca306ab3ff84536a44fc5c8a47e7dcf5d49fcae389f1` |
| Commit de origem | `d85d19ddf66ec20ea01fcbd0b96480cdb28c52b4` |

`pacman -Qp` responde `note-it 0.1.2.r227.gd85d19dd-1`. `pacman -Qlp` lista os
quatro binários, o `.desktop`, o serviço D-Bus, os oito tamanhos de ícone, a
licença e o frontend em `/usr/share/note-it/ui/dist`. `namcap` não aponta nenhum
erro — só os avisos habituais de binário Rust (`ld-linux` não usada, `glibc` e
`libgcc` implicitamente satisfeitas).

**O frontend dentro do pacote foi verificado, não presumido.** O
`index-w8SyGvVw.js` extraído do pacote é byte a byte o mesmo bundle validado no
WebKitGTK real — `sha256 803d2c7a…` nos dois — e o código minificado mostra a
forma corrigida: a ação entregue a `deferDocumentEdit` é o trabalho, e não a
função que a chama.

`makepkg` precisou de `-d`: `pnpm` existe no PATH (11.24.0) mas não é pacote
pacman neste sistema, então a checagem de dependências do makepkg não o encontra.
A build usa o `pnpm` real.

**Retenção.** Ficam em disco `0.1.2` (nova), `0.1.1` (atual, para rollback) e
`0.1.0`. Pela ADR-062, `N-2` só sai depois que a nova estiver instalada e
validada — que é justamente quando a anterior pode ser necessária. Nada foi
removido.

---

## N. Estado final

| Item | Valor |
| --- | --- |
| HEAD | `6c2dafd01398e414926e378b35d64b7a7ab79218`, igual a `origin/main` |
| Árvore | limpa |
| Instalação diária | `note-it 0.1.1.r223.g79332043-1` — **intocada** |
| Dados reais | 42 notas, fingerprint `ef7371a3…` — **inalterados** |
| Fase 6 | **NÃO INICIADA** |

A instalação **não** foi atualizada automaticamente. O comando é do dono:

```bash
sudo pacman -U ./packaging/arch/note-it-0.1.2.r227.gd85d19dd-1-x86_64.pkg.tar.zst
```

Depois da instalação, o que fecha a unidade pela ADR-062 é o smoke test na
instalação real: abrir uma nota, editar, fechar pelo X, reabrir e conferir o
texto.
