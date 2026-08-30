import { Editor } from '@tiptap/core';
import { handleLinkPaste } from './linkPaste.ts';
import { isImagePaste } from './imageTransfer.ts';
import { editorExtensions } from './extensions.ts';
import { CalloutType, calloutType } from './callout.ts';
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
  /**
   * A picture arrived by paste or by drop. Returning `true` means the editor
   * should leave the event alone: the host is importing it, and the document
   * changes when the reference comes back.
   */
  onImageTransfer?: (transfer: DataTransfer) => boolean;
}

export class NoteEditor {
  private editor: Editor;
  private debounceTimer: number | null = null;
  private onUpdateCallback?: (markdown: string) => void;
  private onImageTransfer?: (transfer: DataTransfer) => boolean;

  constructor(options: NoteEditorOptions) {
    this.onUpdateCallback = options.onUpdate;
    this.onImageTransfer = options.onImageTransfer;

    this.editor = new Editor({
      element: options.element,
      extensions: editorExtensions,
      content: sanitizeMarkdown(options.initialContent || ''),
      contentType: 'markdown',
      autofocus: true,
      editorProps: {
        transformPastedHTML: sanitizeHtml,
        // A URL pasted over selected text links that text instead of replacing
        // it. Everything else pastes exactly as it did.
        handlePaste: (view, event) => {
          // A pasted picture is taken in by the host and comes back as a
          // reference. A paste carrying text as well is a text paste, which is
          // what pasting has always done here.
          if (isImagePaste(event.clipboardData) && this.onImageTransfer?.(event.clipboardData!)) {
            return true;
          }
          return handleLinkPaste(view, event);
        },
        handleDrop: (_view, event) => {
          const transfer = (event as DragEvent).dataTransfer;
          if (!transfer) return false;
          return this.onImageTransfer?.(transfer) ?? false;
        },
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

  /**
   * Puts one image into the document at the selection.
   *
   * An ordinary edit through an ordinary transaction, so it is one undo step,
   * it moves the note's modification date, and the existing autosave writes
   * it. Nothing about an image takes a second path to the file.
   */
  public insertImage(src: string): void {
    (this.editor.commands as unknown as {
      setNoteItImage: (attrs: { src: string; alt?: string }) => boolean;
    }).setNoteItImage({ src, alt: '' });
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

  /** The ProseMirror view, for the find and replace commands. */
  public getView() {
    return this.editor.view;
  }

  /** Whatever is selected, as plain text. Empty when nothing is. */
  public selectedText(): string {
    const { from, to } = this.editor.state.selection;
    if (from === to) return '';
    return this.editor.state.doc.textBetween(from, to, '\n');
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

  /**
   * The block the cursor is in, as far as the menu needs to know it.
   *
   * Read on every selection change, so the menu can show what the cursor sits
   * in rather than offering the same rows whatever is under it.
   */
  public currentBlock(): {
    codeBlock: boolean;
    codeLanguage: string | null;
    blockquote: boolean;
    callout: CalloutType | null;
    comment: boolean;
  } {
    const language = this.editor.getAttributes('codeBlock').language;
    return {
      codeBlock: this.editor.isActive('codeBlock'),
      codeLanguage: typeof language === 'string' && language !== '' ? language : null,
      blockquote: this.editor.isActive('blockquote'),
      callout: calloutType(this.editor.getAttributes('blockquote').callout),
      comment: this.editor.isActive('noteItComment'),
    };
  }

  /** Turns the block under the cursor into a code block, or back out of one. */
  public toggleCodeBlock(): void {
    this.editor.chain().focus().toggleCodeBlock().run();
  }

  /**
   * Sets the fence's language, or clears it.
   *
   * Only ever called with an identifier from the menu's own list. A language
   * a note already carries is never rewritten by this — it is changed only
   * when the reader picks a different one.
   */
  public setCodeLanguage(language: string | null): void {
    if (!this.editor.isActive('codeBlock')) return;
    this.editor
      .chain()
      .focus()
      .updateAttributes('codeBlock', { language: language ?? null })
      .run();
  }

  /** Quotes the block under the cursor, or lifts it back out. */
  public toggleBlockquote(): void {
    this.editor.chain().focus().toggleBlockquote().run();
  }

  /**
   * Gives the quote a kind, or takes it away.
   *
   * A callout is a blockquote carrying an attribute, so choosing a kind for
   * ordinary text quotes it first, and clearing the kind leaves the quote
   * behind rather than deleting what it holds.
   */
  public setCallout(type: CalloutType | null): void {
    const chain = this.editor.chain().focus();
    if (!this.editor.isActive('blockquote')) {
      if (type === null) return;
      chain.setBlockquote();
    }
    chain.updateAttributes('blockquote', { callout: type }).run();
  }

  /** Inserts an empty comment block and puts the cursor in it. */
  public insertComment(): void {
    this.editor.chain().focus().insertContent({ type: 'noteItComment' }).run();
  }

  public destroy(): void {
    this.cancelPendingSave();
    this.editor.destroy();
  }

  public getRawEditor(): Editor {
    return this.editor;
  }
}
