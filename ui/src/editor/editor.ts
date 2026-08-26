import { Editor } from '@tiptap/core';
import { editorExtensions } from './extensions.ts';

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
      content: options.initialContent || '',
      contentType: 'markdown',
      autofocus: true,
      onUpdate: () => {
        if (this.debounceTimer !== null) {
          window.clearTimeout(this.debounceTimer);
        }
        this.debounceTimer = window.setTimeout(() => {
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
    this.editor.commands.setContent(content, {
      contentType: 'markdown',
      emitUpdate: false,
    });
  }

  public focus(): void {
    this.editor.commands.focus();
  }

  public destroy(): void {
    if (this.debounceTimer !== null) {
      window.clearTimeout(this.debounceTimer);
    }
    this.editor.destroy();
  }

  public getRawEditor(): Editor {
    return this.editor;
  }
}
