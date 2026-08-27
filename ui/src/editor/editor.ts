import { Editor } from '@tiptap/core';
import { editorExtensions } from './extensions.ts';
import { sanitizeHtml, sanitizeMarkdown } from '../markdown/sanitizer.ts';
import { isValidHexColor } from '../markdown/sanitizer.ts';
import {
  largerTextSize,
  normalizeTextSize,
  smallerTextSize,
  TextSize,
} from './textSize.ts';

export interface NoteEditorOptions {
  element: HTMLElement;
  initialContent?: string;
  onUpdate?: (markdown: string) => void;
}

export class NoteEditor {
  private editor: Editor;
  private debounceTimer: number | null = null;
  private onUpdateCallback?: (markdown: string) => void;

  constructor(options: NoteEditorOptions) {
    this.onUpdateCallback = options.onUpdate;

    this.editor = new Editor({
      element: options.element,
      extensions: editorExtensions,
      content: sanitizeMarkdown(options.initialContent || ''),
      contentType: 'markdown',
      autofocus: true,
      editorProps: {
        transformPastedHTML: sanitizeHtml,
      },
      onUpdate: () => {
        if (this.debounceTimer !== null) {
          window.clearTimeout(this.debounceTimer);
        }
        this.debounceTimer = window.setTimeout(() => {
          this.debounceTimer = null;
          const markdown = this.getMarkdown();
          if (this.onUpdateCallback) {
            this.onUpdateCallback(markdown);
          }
        }, 300);
      },
    });
  }

  public getMarkdown(): string {
    if (typeof this.editor.getMarkdown === 'function') {
      return this.editor.getMarkdown();
    }
    return this.editor.getText();
  }

  public setMarkdown(content: string): void {
    this.cancelPendingSave();
    this.editor.commands.setContent(sanitizeMarkdown(content), {
      contentType: 'markdown',
      emitUpdate: false,
    });
  }

  public cancelPendingSave(): void {
    if (this.debounceTimer !== null) {
      window.clearTimeout(this.debounceTimer);
      this.debounceTimer = null;
    }
  }

  public hasPendingSave(): boolean {
    return this.debounceTimer !== null;
  }

  public focus(): void {
    this.editor.commands.focus();
  }

  public toggleStrike(): void {
    this.editor.chain().focus().toggleStrike().run();
  }

  /**
   * Applies one of the whitelisted sizes to the selection, or clears the
   * custom size when given `null`. With no selection this sets the stored
   * mark, so the next typed characters use it — the same semantics as bold.
   */
  public setTextSize(size: TextSize | null): void {
    const chain = this.editor.chain().focus();
    if (size === null) {
      chain.unsetMark('noteItFontSize').run();
      return;
    }
    const normalized = normalizeTextSize(size);
    if (normalized === null) return;
    chain.setMark('noteItFontSize', { fontSize: normalized }).run();
  }

  /** Size covering the selection, or `null` for the theme default. */
  public currentTextSize(): TextSize | null {
    return normalizeTextSize(this.editor.getAttributes('noteItFontSize').fontSize);
  }

  /** True when the selection mixes sizes, so no single value is current. */
  public hasMixedTextSize(): boolean {
    const { from, to } = this.editor.state.selection;
    if (from === to) return false;

    const seen = new Set<number | null>();
    this.editor.state.doc.nodesBetween(from, to, (node) => {
      if (!node.isText) return;
      const mark = node.marks.find((candidate) => candidate.type.name === 'noteItFontSize');
      seen.add(normalizeTextSize(mark?.attrs?.fontSize));
    });
    return seen.size > 1;
  }

  public increaseTextSize(): void {
    this.setTextSize(largerTextSize(this.currentTextSize()));
  }

  public decreaseTextSize(): void {
    this.setTextSize(smallerTextSize(this.currentTextSize()));
  }

  /** Applies a text colour, or clears it when given `null`. */
  public setTextColor(color: string | null): void {
    const chain = this.editor.chain().focus();
    if (color === null) {
      chain.unsetColor().run();
      return;
    }
    if (!isValidHexColor(color)) return;
    chain.setColor(color).run();
  }

  public currentTextColor(): string | null {
    const color = this.editor.getAttributes('textStyle').color;
    return isValidHexColor(color) ? color : null;
  }

  /** Applies a highlight colour, or removes the highlight when given `null`. */
  public setHighlight(color: string | null): void {
    const chain = this.editor.chain().focus();
    if (color === null) {
      chain.unsetHighlight().run();
      return;
    }
    if (!isValidHexColor(color)) return;
    chain.setHighlight({ color }).run();
  }

  public currentHighlight(): string | null {
    const color = this.editor.getAttributes('highlight').color;
    return isValidHexColor(color) ? color : null;
  }

  public destroy(): void {
    this.cancelPendingSave();
    this.editor.destroy();
  }

  public getRawEditor(): Editor {
    return this.editor;
  }
}
