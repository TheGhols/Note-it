import { Editor } from '@tiptap/core';
import { editorExtensions } from './extensions.ts';
import { sanitizeHtml, sanitizeMarkdown } from '../markdown/sanitizer.ts';

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

  public destroy(): void {
    this.cancelPendingSave();
    this.editor.destroy();
  }

  public getRawEditor(): Editor {
    return this.editor;
  }
}
