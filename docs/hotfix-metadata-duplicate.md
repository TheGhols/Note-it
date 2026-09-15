# Hotfix — `metadata_changed` enviado em duplicata

**Data:** 15/09/2026 · **Versão:** `0.1.2` → `0.1.3` · **Escopo:** um defeito de emissão no frontend

Defeito identificado durante o hotfix do botão X e deliberadamente adiado ali
para não ampliar aquele escopo. Corrigido agora por si.

---

## A. Causa

A mesma leitura errada de `deferDocumentEdit` que quebrou o botão X, no handler
`save` do painel de metadados em `ui/src/main.ts`:

```ts
if (
  deferDocumentEdit(() => {
    /* envia metadata_changed */
  })
)
  return;

noteEditor.cancelPendingSave();
/* envia metadata_changed de novo */
```

`ExternalWriteBarrier.defer(action)` não é uma pergunta. Com o documento livre —
o caminho comum — ele **executa** a ação e responde `false`. O handler lia esse
`false` como "não aconteceu" e repetia o envio, então uma única ação do leitor
colocava a mesma alteração na ponte duas vezes.

O caminho com escrita externa segurando o documento **já estava correto**:
`defer` enfileirava, devolvia `true` e o `return` acontecia — exatamente uma
mensagem na liberação.

---

## B. Reprodução

Determinística, contra o código anterior, dirigindo o caminho real da UI
(`index.html` publicado → `main.ts` → menu → painel → formulário de tag):

```text
1 ação do leitor (adicionar a tag "Cardiologia")
→ 2 mensagens metadata_changed
→ payloads byte a byte idênticos, mesmo requestId
→ nenhuma outra mensagem enviada
```

```text
#1 {"type":"metadata_changed","payload":{"requestId":2,"id":"1111…","content":"corpo da nota","generation":0,"tags":["Cardiologia"],"properties":[]}}
#2 {"type":"metadata_changed","payload":{"requestId":2,"id":"1111…","content":"corpo da nota","generation":0,"tags":["Cardiologia"],"properties":[]}}
IDENTICAS? true
```

### O que o defeito causava — medido, não inferido

**Não** duas gravações. O store recusa um save idêntico:

```rust
if doc.content == content && doc.user_metadata == metadata {
    return Ok(());
}
```

`save_user_metadata` devolve `Ok(())` sem escrever quando conteúdo e metadados
não mudaram, e a segunda mensagem chega exatamente assim. Medido com
`inotifywait` no WebKitGTK real, em store isolado, nas **duas** builds:

| Build | Ação | Eventos no diretório de notas |
| --- | --- | --- |
| Defeituosa (`d48b7e6`) | adicionar 1 tag | 1 `CREATE` de temporário + 1 `MOVED_TO` |
| Corrigida | adicionar 1 tag | 1 `CREATE` de temporário + 1 `MOVED_TO` |

O que o defeito produzia, então, era **trabalho duplicado na ponte e no host** —
duas mensagens, duas validações, dois `metadata_save_result` de volta — e a
dependência de um curto-circuito lá embaixo para permanecer inofensivo. Um
defeito de protocolo, não de dados.

Uma afirmação anterior de que o host gravava a nota duas vezes estava errada:
foi inferida do handler sem conferir o curto-circuito do store. A medição acima
é o que vale.

---

## C. Auditoria do padrão

Os quatro usos de `deferDocumentEdit` em `ui/src/main.ts`:

| Linha | Uso | Classificação |
| --- | --- | --- |
| 427 | captura do AutoPaste — passa o trabalho, ignora o retorno | correto |
| 626 | `saveAndClose` — corrigido no hotfix do X | correto |
| **770** | `metadataPanel.save` — `if (defer(…)) return;` + reenvio | **duplicado** |
| 1315 | inserção de imagem — passa o trabalho, ignora o retorno | correto |

Era o único sítio restante com o padrão defeituoso, e os dois envios de
`metadata_changed` do arquivo eram os dois lados dele — mesma causa-raiz, por
isso corrigidos juntos. Nenhum outro defeito do mesmo padrão ficou pendente.

---

## D. Correção

`ui/src/main.ts`, um handler. O envio é a ação deferida e não há nada depois
dele — a forma já validada no hotfix do X. `cancelPendingSave()` entra junto,
onde o envio está, porque é ele que supera o autosave pendente.

Nenhuma linha de Rust. A barreira, a geração, a ordem em relação à escrita
externa e a semântica de metadata ficam como estavam.

---

## E. Teste de regressão

`ui/tests/metadata_emission.test.ts` — dois casos.

`tests/metadata.test.ts` cobre o painel, mas entrega a ele um `save` dublê
(`save: vi.fn()`), então não vê o que o `main.ts` faz com a chamada — que é onde
o defeito estava. Este teste dirige o caminho real: o `index.html` publicado, a
ligação do `main.ts`, o menu, a entrada Metadados e o formulário de adicionar
tag do próprio painel. A propriedade é uma **contagem**, porque é o que estava
errado.

1. **Documento livre** — uma alteração produz **exatamente uma**
   `metadata_changed`, com `generation: 0`, a tag e o corpo da nota.
2. **Documento segurado** — **zero** mensagens durante o hold; **exatamente uma**
   na liberação, citando a geração nova e o conteúdo comprometido.

### Prova negativa

Contra `d48b7e6` em worktree isolada, sem tocar o histórico:

```text
× sends exactly one metadata_changed when the document is free
  → one change by the reader must reach the host once:
    expected [ …(2) ] to have a length of 1 but got 2

✓ holds the change for an external write and sends it once on release
```

O caso 1 falha pelo motivo exato do defeito. O caso 2 passa nas duas versões —
honestamente, porque aquele caminho já estava certo; ele fica como guarda contra
regredir no outro sentido, por exemplo removendo o `defer`.

---

## F. Hotfix do botão X

Não regrediu. `saveAndClose` e `deferDocumentEdit` não foram tocados — o diff é
um único hunk no handler de metadados — e `tests/close_button.test.ts` continua
verde nos dois casos, incluindo o de escrita externa do fechamento.

---

## G. Smoke test no WebKitGTK real

Sway aninhado, store e barramento D-Bus isolados, build corrigida:

1. metadados alterados pelo menu → painel → formulário de tag;
2. tag persistida — `tags: [- Cardiologia]` no arquivo, `Metadados salvos` no painel;
3. interface responsiva — texto digitado na nota depois disso;
4. uma ação → **uma** gravação atômica (`inotifywait`);
5. fechar pelo X continua funcionando — papel 26 120 → 0, conteúdo e tag salvos,
   processo residente conforme o contrato.

Nenhuma das 42 notas reais foi usada como massa de teste.

---

## H. Gates

`scripts/check` — **exit 0**, os 24 estágios verdes. Frontend: **64 arquivos,
1346 testes**. `cargo fmt --check` limpo, `tsc --noEmit` limpo.

---

## I. CI

| Run | SHA | Rust Checks & Tests | Frontend Checks & Tests | Resultado |
| --- | --- | --- | --- | --- |
| `34936608706` | `cc0781dd` — a correção e a versão 0.1.3 | success | success | **success** |

Passou na primeira tentativa; nenhum pipeline reexecutado.

---

## J. Pacote

Construído do commit `cc0781dd` — o SHA que o CI aprovou.

| Item | Valor |
| --- | --- |
| Arquivo | `packaging/arch/note-it-0.1.3.r232.gcc0781dd-1-x86_64.pkg.tar.zst` |
| Versão / `pkgver` | `0.1.3` / `0.1.3.r232.gcc0781dd-1` |
| Commit de origem | `cc0781ddc9b9308170bdc245ff59b267d8156b32` |
| Tamanho | 8 426 321 bytes (8,1 MiB) |
| SHA256 | `58a89164579a8fce5193ba38321a000c3515d00408e4937d3bc468ebb3434491` |

`pacman -Qp` responde `note-it 0.1.3.r232.gcc0781dd-1`. `pacman -Qlp` lista os
quatro binários, o `.desktop`, o serviço D-Bus, os oito tamanhos de ícone, a
licença e o frontend. `namcap` não aponta nenhum erro — zero `E:`, só os avisos
habituais de binário Rust.

**As duas correções verificadas dentro do pacote, não presumidas.** O
`index-rJwMgyvm.js` extraído é byte a byte o bundle validado no WebKitGTK real
(`sha256 29c7c8d3…`), e nele `"metadata_changed"` e `"save_and_close"` aparecem
**uma vez cada**, os dois dentro da ação deferida. A 0.1.3 descende da 0.1.2,
então este pacote carrega o hotfix do botão X e este — instalar a 0.1.2 antes é
desnecessário.

`makepkg` precisou de `-d`, pela mesma razão da 0.1.2: `pnpm` está no PATH mas
não é pacote pacman neste sistema.

**Retenção.** Ficam em disco `0.1.3`, `0.1.2`, `0.1.1` (a instalada) e `0.1.0`.
Pela ADR-062 nada é removido antes que a nova esteja instalada e validada, e a
instalada é justamente a que precisa existir para rollback.

---

## K. Estado final

| Item | Valor |
| --- | --- |
| Instalação diária | `note-it 0.1.1.r223.g79332043-1` — **intocada** |
| Dados reais | 42 notas, fingerprint `ef7371a3…` — **inalterados** |
| Fase 6 | **NÃO INICIADA** |

A instalação não foi atualizada automaticamente:

```bash
sudo pacman -U ./packaging/arch/note-it-0.1.3.r232.gcc0781dd-1-x86_64.pkg.tar.zst
```
