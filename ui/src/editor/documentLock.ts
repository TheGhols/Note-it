import { Extension } from '@tiptap/core';
import { Plugin, PluginKey } from '@tiptap/pm/state';

/**
 * One gate every change to the document has to pass.
 *
 * While an external write is in flight the document has already been handed to
 * the host and is about to be replaced by the committed version. A change that
 * lands in between is either lost when the committed text arrives, or — worse —
 * overwrites a change that has just been written to disk.
 *
 * Making the editor non-editable is not enough on its own. That stops the
 * *reader*: typing, pasting, dropping. It does not stop the page itself, and
 * the page has several ways to change a document — inserting an image the host
 * has just imported, a replace from the find bar, a formatting shortcut. Each
 * of those is a command, and commands run whether or not the editor is
 * editable.
 *
 * So the gate is at the transaction. `filterTransaction` is the single point
 * every document change in ProseMirror goes through, whatever produced it, and
 * whatever a later version of the editor adds. A list of blocked entry points
 * would need extending each time one appeared; this cannot be gone round.
 *
 * Only transactions that change the *document* are refused. Selection, focus
 * and decoration transactions carry on, so a held note still scrolls, still
 * renders and still looks alive rather than broken.
 */
export const documentLockKey = new PluginKey('noteItDocumentLock');

export function DocumentLock(isLocked: () => boolean): Extension {
  return Extension.create({
    name: 'noteItDocumentLock',
    addProseMirrorPlugins() {
      return [
        new Plugin({
          key: documentLockKey,
          filterTransaction: (transaction) => !(isLocked() && transaction.docChanged),
        }),
      ];
    },
  });
}
