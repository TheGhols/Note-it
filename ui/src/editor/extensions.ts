import StarterKit from '@tiptap/starter-kit';
import Underline from '@tiptap/extension-underline';
import Highlight from '@tiptap/extension-highlight';
import { TextStyle } from '@tiptap/extension-text-style';
import Color from '@tiptap/extension-color';
import TaskList from '@tiptap/extension-task-list';
import TaskItem from '@tiptap/extension-task-item';
import Placeholder from '@tiptap/extension-placeholder';
import { Markdown } from '@tiptap/markdown';

// Custom Underline serialized to <u>...</u>
const NoteItUnderline = Underline.extend({
  renderMarkdown(node: any, helpers: any) {
    return `<u>${helpers.renderChildren(node)}</u>`;
  },
  markdownTokenizer: {
    name: 'underline',
    level: 'inline',
    start(src: string) {
      return src.indexOf('<u>');
    },
    tokenize(src: string, _tokens: any, lexer: any) {
      const match = /^(<u>)([\s\S]+?)(<\/u>)/i.exec(src);
      if (!match) return;
      const innerContent = match[2];
      return {
        type: 'underline',
        raw: match[0],
        text: innerContent,
        tokens: lexer.inlineTokens(innerContent),
      };
    },
  },
});

// Custom Highlight serialized to <mark data-note-it-highlight="..." style="...">
const NoteItHighlight = Highlight.extend({
  renderMarkdown(node: any, helpers: any) {
    const color = node.attrs?.color;
    if (color) {
      return `<mark data-note-it-highlight="${color}" style="background-color:${color}">${helpers.renderChildren(node)}</mark>`;
    }
    return `<mark>${helpers.renderChildren(node)}</mark>`;
  },
  markdownTokenizer: {
    name: 'highlight',
    level: 'inline',
    start(src: string) {
      return src.indexOf('<mark');
    },
    tokenize(src: string, _tokens: any, lexer: any) {
      const match = /^<mark(?:[^>]*?data-note-it-highlight=["']([^"']+)["'][^>]*)?>([\s\S]+?)<\/mark>/i.exec(src);
      if (!match) return;
      const color = match[1];
      const innerContent = match[2];
      return {
        type: 'highlight',
        raw: match[0],
        text: innerContent,
        attrs: color ? { color } : {},
        tokens: lexer.inlineTokens(innerContent),
      };
    },
  },
});

// Custom Color serialized to <span data-note-it-color="..." style="...">
const NoteItColor = Color.extend({
  renderMarkdown(node: any, helpers: any) {
    const color = node.attrs?.color;
    if (color) {
      return `<span data-note-it-color="${color}" style="color:${color}">${helpers.renderChildren(node)}</span>`;
    }
    return helpers.renderChildren(node);
  },
  markdownTokenizer: {
    name: 'color',
    level: 'inline',
    start(src: string) {
      return src.indexOf('<span');
    },
    tokenize(src: string, _tokens: any, lexer: any) {
      const match = /^<span[^>]*?data-note-it-color=["']([^"']+)["'][^>]*>([\s\S]+?)<\/span>/i.exec(src);
      if (!match) return;
      const color = match[1];
      const innerContent = match[2];
      return {
        type: 'color',
        raw: match[0],
        text: innerContent,
        attrs: { color },
        tokens: lexer.inlineTokens(innerContent),
      };
    },
  },
});

export const editorExtensions = [
  StarterKit.configure({
    heading: {
      levels: [1, 2, 3],
    },
    undoRedo: {
      depth: 100,
    },
    underline: false,
  }),
  NoteItUnderline,
  NoteItHighlight.configure({
    multicolor: true,
  }),
  TextStyle,
  NoteItColor,
  TaskList,
  TaskItem.configure({
    nested: true,
  }),
  Placeholder.configure({
    placeholder: 'Type your note here...',
  }),
  Markdown.configure({
    indentation: {
      style: 'space',
      size: 2,
    },
  }),
];
