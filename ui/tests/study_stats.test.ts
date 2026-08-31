import { describe, expect, it } from 'vitest';
import {
  currentStreak,
  heatLevel,
  heatmapDays,
  intervalLabel,
  localDay,
  longestStreak,
  nextLevel,
  reviewNow,
  statusOf,
} from '../src/study/stats.ts';
import { emptyStudyState, type GlobalReviewItem, type StudyCardState } from '../src/study/types.ts';

function schedule(due_at: string, level = 1): StudyCardState {
  return {
    level,
    due_at,
    last_reviewed_at: '2026-08-01T12:00:00Z',
    review_count: 1,
    last_rating: 'medium',
  };
}

function item(order: number, value: StudyCardState | null): GlobalReviewItem {
  return { documentOrder: order, schedule: value } as GlobalReviewItem;
}

describe('Ladder-v1 previews', () => {
  it('uses the exact new-card and existing-card steps', () => {
    expect(['difficult', 'medium', 'easy'].map((rating) => nextLevel(null, rating as never))).toEqual([
      0,
      1,
      2,
    ]);
    expect(['difficult', 'medium', 'easy'].map((rating) => nextLevel(1, rating as never))).toEqual([
      0,
      2,
      3,
    ]);
    expect(nextLevel(0, 'difficult')).toBe(0);
    expect(nextLevel(8, 'medium')).toBe(8);
    expect(nextLevel(8, 'easy')).toBe(8);
  });

  it('names every central interval without floating point', () => {
    expect(Array.from({ length: 9 }, (_, level) => intervalLabel(level))).toEqual([
      '10 min',
      '1 dia',
      '3 dias',
      '7 dias',
      '14 dias',
      '30 dias',
      '60 dias',
      '120 dias',
      '240 dias',
    ]);
  });
});

describe('catalog scheduling', () => {
  const now = new Date('2026-08-31T12:00:00Z');

  it('distinguishes new, due, and future cards', () => {
    expect(statusOf(item(0, null), now)).toBe('new');
    expect(statusOf(item(0, schedule('2026-08-31T12:00:00Z')), now)).toBe('due');
    expect(statusOf(item(0, schedule('2026-09-01T12:00:00Z')), now)).toBe('future');
  });

  it('puts the most overdue first, then new cards in document order', () => {
    const future = item(0, schedule('2026-09-02T12:00:00Z'));
    const newerDue = item(4, schedule('2026-08-30T12:00:00Z'));
    const oldestDue = item(3, schedule('2026-08-20T12:00:00Z'));
    const secondNew = item(2, null);
    const firstNew = item(1, null);
    expect(reviewNow([future, newerDue, oldestDue, secondNew, firstNew], now)).toEqual([
      oldestDue,
      newerDue,
      firstNew,
      secondNew,
    ]);
  });
});

describe('local daily activity projections', () => {
  it('uses the local civil date on both sides of local midnight', () => {
    const before = new Date(2026, 7, 30, 23, 59, 59);
    const after = new Date(2026, 7, 31, 0, 0, 1);
    expect(localDay(before)).toBe('2026-08-30');
    expect(localDay(after)).toBe('2026-08-31');
  });

  it('implements every current-streak boundary', () => {
    const now = new Date(2026, 7, 31, 12);
    expect(currentStreak({}, now)).toBe(0);
    expect(currentStreak({ '2026-08-31': { reviews: 2 } as never }, now)).toBe(1);
    expect(currentStreak({ '2026-08-30': { reviews: 3 } as never }, now)).toBe(1);
    expect(
      currentStreak(
        {
          '2026-08-29': { reviews: 1 } as never,
          '2026-08-30': { reviews: 5 } as never,
          '2026-08-31': { reviews: 2 } as never,
        },
        now,
      ),
    ).toBe(3);
    expect(currentStreak({ '2026-08-29': { reviews: 8 } as never }, now)).toBe(0);
  });

  it('finds the longest civil-day run across gaps and DST-length days', () => {
    expect(
      longestStreak({
        '2026-03-07': { reviews: 1 } as never,
        '2026-03-08': { reviews: 9 } as never,
        '2026-03-09': { reviews: 2 } as never,
        '2026-03-12': { reviews: 1 } as never,
        '2026-03-13': { reviews: 1 } as never,
      }),
    ).toBe(3);
  });

  it('uses fixed heat levels and exactly 365 accessible-day inputs', () => {
    expect([0, 1, 4, 5, 9, 10, 19, 20, 200].map(heatLevel)).toEqual([
      0,
      1,
      1,
      2,
      2,
      3,
      3,
      4,
      4,
    ]);
    const state = emptyStudyState();
    state.days['2026-08-31'] = {
      reviews: 20,
      difficult: 2,
      medium: 8,
      easy: 10,
    };
    const days = heatmapDays(state, new Date(2026, 7, 31, 12));
    expect(days).toHaveLength(365);
    expect(days.at(-1)).toEqual({ key: '2026-08-31', reviews: 20, level: 4 });
  });
});
