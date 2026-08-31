import type { Fragment, Node as ProseMirrorNode } from '@tiptap/pm/model';
import { reviewItems, type FlashcardSource } from '../flashcards/extract.ts';
import type { GlobalReviewItem, StudyCardState } from './types.ts';

export const REVIEW_KEY_VERSION = 'review-key-v1';

type SemanticNode =
  | readonly ['text', string]
  | readonly ['image', string, string]
  | readonly ['hard_break']
  | readonly ['node', string, Readonly<Record<string, unknown>>, readonly SemanticNode[]];

function semanticAttributes(node: ProseMirrorNode): Readonly<Record<string, unknown>> {
  switch (node.type.name) {
    case 'heading':
      return { level: node.attrs.level };
    case 'taskItem':
      return { checked: Boolean(node.attrs.checked) };
    case 'blockquote':
      return node.attrs.callout ? { callout: node.attrs.callout } : {};
    case 'codeBlock':
      return node.attrs.language ? { language: node.attrs.language } : {};
    case 'orderedList':
      return node.attrs.start && node.attrs.start !== 1 ? { start: node.attrs.start } : {};
    default:
      return {};
  }
}

function semanticNode(node: ProseMirrorNode): SemanticNode {
  if (node.isText) return ['text', (node.text ?? '').normalize('NFC')];
  if (node.type.name === 'noteItImage') {
    return ['image', String(node.attrs.src ?? ''), String(node.attrs.alt ?? '').normalize('NFC')];
  }
  if (node.type.name === 'hardBreak') return ['hard_break'];

  const children: SemanticNode[] = [];
  node.content.forEach((child) => children.push(semanticNode(child)));
  return ['node', node.type.name, semanticAttributes(node), children];
}

/** Presentation marks and image layout attributes deliberately do not appear. */
export function canonicalSide(content: Fragment): string {
  const nodes: SemanticNode[] = [];
  content.forEach((node) => nodes.push(semanticNode(node)));
  return JSON.stringify(nodes);
}

async function sha256(text: string): Promise<string> {
  const bytes = new TextEncoder().encode(text);
  const digest = await crypto.subtle.digest('SHA-256', bytes);
  return Array.from(new Uint8Array(digest), (byte) => byte.toString(16).padStart(2, '0')).join('');
}

export async function identifyReviews(
  noteId: string,
  noteTitle: string,
  sources: readonly FlashcardSource[],
  schedules: Readonly<Record<string, StudyCardState>>,
  orderOffset = 0,
): Promise<GlobalReviewItem[]> {
  const ordinals = new Map<string, number>();
  const reviews = reviewItems(sources);
  const result: GlobalReviewItem[] = [];

  for (let index = 0; index < reviews.length; index += 1) {
    const review = reviews[index];
    const source = sources[review.source];
    const semanticFront = canonicalSide(source.front.content);
    const semanticBack = canonicalSide(source.back.content);
    const signature = JSON.stringify([semanticFront, semanticBack, review.direction]);
    const ordinal = ordinals.get(signature) ?? 0;
    ordinals.set(signature, ordinal + 1);
    const reviewKey = await sha256(
      JSON.stringify([
        REVIEW_KEY_VERSION,
        noteId,
        semanticFront,
        semanticBack,
        review.direction,
        ordinal,
      ]),
    );
    result.push({
      ...review,
      noteId,
      noteTitle,
      reviewKey,
      schedule: schedules[reviewKey] ?? null,
      documentOrder: orderOffset + index,
    });
  }
  return result;
}
