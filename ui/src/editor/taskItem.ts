import { InputRule } from '@tiptap/core';
import TaskItem from '@tiptap/extension-task-item';
import { Plugin, PluginKey } from '@tiptap/pm/state';
import { formatNoteTimestamp } from '../format/datetime.ts';
import {
  extractCompletedAt,
  isValidCompletedAt,
  renderCompletedAtComment,
} from '../markdown/taskMeta.ts';

/**
 * Matches the checkbox part of a task marker. The list marker itself (`- `) has
 * already been consumed by the bullet list input rule by the time this runs.
 */
export const TASK_INPUT_REGEX = /^\s*\[([ xX])?\]\s$/;

const completedAtPluginKey = new PluginKey('noteItTaskCompletedAt');

/**
 * Task items that remember when they were completed.
 *
 * Typing `- [ ] ` first creates a plain bullet, because the bullet rule fires
 * on `- ` before the checkbox is typed. The input rule below converts that
 * bullet into a task item as soon as the checkbox completes it, so the whole
 * sequence lands on a real task node rather than a bullet containing `[ ]`.
 */
export const NoteItTaskItem = TaskItem.extend({
  addAttributes() {
    return {
      ...this.parent?.(),
      completedAt: {
        default: null,
        parseHTML: (element: HTMLElement) => {
          const value = element.getAttribute('data-completed-at');
          return isValidCompletedAt(value) ? value : null;
        },
        renderHTML: (attributes: Record<string, unknown>) => {
          const value = attributes.completedAt;
          if (!isValidCompletedAt(value)) return {};
          return {
            'data-completed-at': value,
            // Rendered by CSS as a discreet suffix, so the date never becomes
            // editable text inside the task.
            'data-completed-label': `Concluído ${formatNoteTimestamp(value)}`,
          };
        },
      },
    };
  },

  addInputRules() {
    return [
      new InputRule({
        find: TASK_INPUT_REGEX,
        handler: ({ range, match, chain, state }) => {
          const checked = /[xX]/.test(match[1] ?? '');
          const { $from } = state.selection;

          // Already inside a task item: just set its state.
          for (let depth = $from.depth; depth > 0; depth -= 1) {
            if ($from.node(depth).type.name === this.name) {
              chain()
                .deleteRange(range)
                .updateAttributes(this.name, { checked })
                .run();
              return;
            }
          }

          chain()
            .deleteRange(range)
            .toggleList('taskList', this.name, false)
            .updateAttributes(this.name, { checked })
            .run();
        },
      }),
    ];
  },

  addProseMirrorPlugins() {
    const parentPlugins = this.parent?.() ?? [];
    const typeName = this.name;

    return [
      ...parentPlugins,
      new Plugin({
        key: completedAtPluginKey,
        /**
         * Keeps `completedAt` in step with `checked` for every path that can
         * toggle a task: the checkbox, the input rule, and any command.
         *
         * Only a genuine unchecked-to-checked transition mints a timestamp. A
         * task that arrives already checked — loaded from Markdown, pasted, or
         * restored by undo — has no previous node here and is left alone, so a
         * task completed outside Note-it never gets an invented date.
         */
        appendTransaction: (transactions, oldState, newState) => {
          if (!transactions.some((transaction) => transaction.docChanged)) return null;

          const tr = newState.tr;
          const now = new Date().toISOString();
          let modified = false;

          newState.doc.descendants((node, pos) => {
            if (node.type.name !== typeName) return;

            let previousPos = pos;
            for (let i = transactions.length - 1; i >= 0; i -= 1) {
              previousPos = transactions[i].mapping.invert().map(previousPos);
            }
            const previous = oldState.doc.nodeAt(previousPos);
            const existedBefore = previous?.type.name === typeName;

            if (node.attrs.checked) {
              if (existedBefore && !previous.attrs.checked) {
                // Completed just now.
                tr.setNodeMarkup(pos, undefined, { ...node.attrs, completedAt: now });
                modified = true;
              }
              return;
            }

            if (node.attrs.completedAt !== null) {
              // Reopened: the old completion date must not linger.
              tr.setNodeMarkup(pos, undefined, { ...node.attrs, completedAt: null });
              modified = true;
            }
          });

          return modified ? tr : null;
        },
      }),
    ];
  },

  renderMarkdown(this: any, node: any, helpers: any) {
    // Keep the upstream renderer responsible for the list marker and the
    // nesting indentation; only the metadata comment is appended here.
    const rendered = this.parent?.(node, helpers) ?? '';
    if (!node.attrs?.checked) return rendered;

    const comment = renderCompletedAtComment(node.attrs.completedAt);
    return comment ? `${rendered.replace(/[ \t]+$/, '')} ${comment}` : rendered;
  },

  parseMarkdown(this: any, token: any, helpers: any) {
    const { completedAt } = extractCompletedAt(String(token.text ?? ''));
    // Drop the metadata comment so it never becomes visible content.
    const kept = (token.tokens ?? []).filter(
      (child: any) => !(child?.type === 'html' && /note-it:completed_at=/.test(child.raw ?? '')),
    );
    // The space that separated the task text from the comment would otherwise
    // survive as trailing whitespace inside the task.
    const last = kept[kept.length - 1];
    if (kept.length !== (token.tokens ?? []).length && last?.type === 'text') {
      kept[kept.length - 1] = {
        ...last,
        raw: String(last.raw ?? '').replace(/[ \t]+$/, ''),
        text: String(last.text ?? '').replace(/[ \t]+$/, ''),
      };
    }

    const cleaned = {
      ...token,
      text: extractCompletedAt(String(token.text ?? '')).text,
      mainContent: extractCompletedAt(String(token.mainContent ?? '')).text,
      tokens: kept,
    };

    const node = this.parent?.(cleaned, helpers);
    if (!node || !token.checked || completedAt === null) return node;

    return { ...node, attrs: { ...(node.attrs ?? {}), completedAt } };
  },
});
