import type { GlobalReviewItem, StudyDay, StudyRating, StudyState } from './types.ts';

export const INTERVAL_SECONDS = [600, 86_400, 259_200, 604_800, 1_209_600, 2_592_000, 5_184_000, 10_368_000, 20_736_000] as const;

export function nextLevel(current: number | null, rating: StudyRating): number {
  if (current === null) return rating === 'difficult' ? 0 : rating === 'medium' ? 1 : 2;
  if (rating === 'difficult') return Math.max(0, current - 1);
  return Math.min(8, current + (rating === 'medium' ? 1 : 2));
}

export function intervalLabel(level: number): string {
  if (level === 0) return '10 min';
  const days = INTERVAL_SECONDS[level] / 86_400;
  return days === 1 ? '1 dia' : `${days} dias`;
}

export type StudyStatus = 'new' | 'due' | 'future';

export function statusOf(item: GlobalReviewItem, now: Date): StudyStatus {
  if (!item.schedule) return 'new';
  return Date.parse(item.schedule.due_at) <= now.getTime() ? 'due' : 'future';
}

export function reviewNow(items: readonly GlobalReviewItem[], now: Date): GlobalReviewItem[] {
  const due = items
    .filter((item) => statusOf(item, now) === 'due')
    .sort((left, right) => Date.parse(left.schedule!.due_at) - Date.parse(right.schedule!.due_at));
  const fresh = items
    .filter((item) => statusOf(item, now) === 'new')
    .sort((left, right) => left.documentOrder - right.documentOrder);
  return [...due, ...fresh];
}

export function localDay(date: Date): string {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, '0');
  const day = String(date.getDate()).padStart(2, '0');
  return `${year}-${month}-${day}`;
}

function civilOrdinal(key: string): number {
  const [year, month, day] = key.split('-').map(Number);
  return Math.trunc(Date.UTC(year, month - 1, day) / 86_400_000);
}

function hasReviews(days: Readonly<Record<string, StudyDay>>, date: Date): boolean {
  return (days[localDay(date)]?.reviews ?? 0) > 0;
}

export function currentStreak(days: Readonly<Record<string, StudyDay>>, now: Date): number {
  const cursor = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12);
  if (!hasReviews(days, cursor)) {
    cursor.setDate(cursor.getDate() - 1);
    if (!hasReviews(days, cursor)) return 0;
  }
  let count = 0;
  while (hasReviews(days, cursor)) {
    count += 1;
    cursor.setDate(cursor.getDate() - 1);
  }
  return count;
}

export function longestStreak(days: Readonly<Record<string, StudyDay>>): number {
  const active = Object.entries(days)
    .filter(([, activity]) => activity.reviews > 0)
    .map(([key]) => key)
    .sort();
  let longest = 0;
  let run = 0;
  let previous: number | null = null;
  for (const key of active) {
    const date = civilOrdinal(key);
    const consecutive = previous !== null && date - previous === 1;
    run = consecutive ? run + 1 : 1;
    longest = Math.max(longest, run);
    previous = date;
  }
  return longest;
}

export function heatLevel(reviews: number): 0 | 1 | 2 | 3 | 4 {
  if (reviews === 0) return 0;
  if (reviews < 5) return 1;
  if (reviews < 10) return 2;
  if (reviews < 20) return 3;
  return 4;
}

export function heatmapDays(state: StudyState, now: Date): Array<{ key: string; reviews: number; level: 0 | 1 | 2 | 3 | 4 }> {
  const result = [];
  const date = new Date(now.getFullYear(), now.getMonth(), now.getDate(), 12);
  date.setDate(date.getDate() - 364);
  for (let index = 0; index < 365; index += 1) {
    const key = localDay(date);
    const reviews = state.days[key]?.reviews ?? 0;
    result.push({ key, reviews, level: heatLevel(reviews) });
    date.setDate(date.getDate() + 1);
  }
  return result;
}
