import type { Schema } from '@tiptap/pm/model';
import type { ReviewItem } from '../flashcards/extract.ts';

export type StudyRating = 'difficult' | 'medium' | 'easy';

export interface StudyCardState {
  level: number;
  due_at: string;
  last_reviewed_at: string;
  review_count: number;
  last_rating: StudyRating;
}

export interface StudyDay {
  reviews: number;
  difficult: number;
  medium: number;
  easy: number;
}

export interface StudyState {
  version: number;
  algorithm: 'ladder-v1';
  cards: Record<string, StudyCardState>;
  days: Record<string, StudyDay>;
}

export interface StudyCatalogNote {
  id: string;
  content: string;
}

export interface GlobalReviewItem extends ReviewItem {
  readonly noteId: string;
  readonly noteTitle: string;
  readonly reviewKey: string;
  readonly schedule: StudyCardState | null;
  readonly documentOrder: number;
}

export interface GlobalCatalog {
  readonly items: readonly GlobalReviewItem[];
  readonly sourceCards: number;
  readonly notesWithCards: number;
  readonly schema: Schema;
}

export function emptyStudyState(): StudyState {
  return { version: 1, algorithm: 'ladder-v1', cards: {}, days: {} };
}
