import { afterEach, describe, expect, it, vi } from 'vitest';
import { buildGlobalCatalog } from '../src/study/catalog.ts';
import { emptyStudyState, type GlobalCatalog, type StudyState } from '../src/study/types.ts';
import { StudyHub } from '../src/ui/studyHub.ts';

const A = '11111111-1111-4111-8111-111111111111';
const B = '22222222-2222-4222-8222-222222222222';
const NOW = new Date('2026-08-31T12:00:00Z');

afterEach(() => document.body.replaceChildren());

async function fixture(): Promise<{ catalog: GlobalCatalog; study: StudyState }> {
  const notes = [
    { id: A, content: '# Anatomia\n\nCoração :: Bomba' },
    { id: B, content: '# Farmacologia\n\nMetformina ::: Biguanida' },
  ];
  const initial = await buildGlobalCatalog(notes, emptyStudyState(), null, document);
  const study = emptyStudyState();
  study.cards[initial.items[0].reviewKey] = {
    level: 1,
    due_at: '2026-08-20T12:00:00Z',
    last_reviewed_at: '2026-08-19T12:00:00Z',
    review_count: 1,
    last_rating: 'medium',
  };
  study.cards[initial.items[2].reviewKey] = {
    level: 2,
    due_at: '2026-09-03T12:00:00Z',
    last_reviewed_at: '2026-08-31T11:00:00Z',
    review_count: 2,
    last_rating: 'easy',
  };
  study.days['2026-08-30'] = { reviews: 1, difficult: 1, medium: 0, easy: 0 };
  study.days['2026-08-31'] = { reviews: 2, difficult: 0, medium: 1, easy: 1 };
  return { catalog: await buildGlobalCatalog(notes, study, null, document), study };
}

function mount() {
  const host = document.createElement('div');
  const invoker = document.createElement('button');
  document.body.append(invoker, host);
  Object.defineProperty(invoker, 'offsetParent', { configurable: true, get: () => document.body });
  const handlers = {
    onRequestCatalog: vi.fn(),
    onStart: vi.fn(),
    onClose: vi.fn(),
  };
  const hub = new StudyHub({ mount: host, handlers, document, now: () => NOW });
  return { hub, handlers, invoker };
}

function stat(hub: StudyHub, label: string): string {
  const found = Array.from(hub.element().querySelectorAll('.note-study-stat')).find(
    (item) => item.querySelector('span')?.textContent === label,
  );
  return found?.querySelector('strong')?.textContent ?? '';
}

describe('the Study Hub', () => {
  it('opens as a focused dialog, loads by generation, and restores its invoker', () => {
    const { hub, handlers, invoker } = mount();
    hub.openHub(A, invoker);
    expect(hub.isOpen()).toBe(true);
    expect(hub.element().hidden).toBe(false);
    expect(hub.element().getAttribute('role')).toBe('dialog');
    expect(hub.element().getAttribute('aria-label')).toBe('Central de estudos');
    expect(document.activeElement).toBe(hub.element());
    expect(hub.element().querySelector('.note-study-hub-status')?.textContent).toContain('Carregando');
    expect(handlers.onRequestCatalog).toHaveBeenCalledWith(1);

    hub.close();
    expect(hub.element().hidden).toBe(true);
    expect(document.activeElement).toBe(invoker);
  });

  it('drops stale catalog replies and fails closed without constructing an empty state', async () => {
    const { hub, invoker } = mount();
    const { catalog, study } = await fixture();
    hub.openHub(A, invoker);
    hub.element().querySelector<HTMLButtonElement>('.note-study-hub-refresh')!.click();
    expect(hub.currentRequestId()).toBe(2);
    expect(hub.showCatalog(1, catalog, study)).toBe(false);
    expect(hub.showError(2, 'Histórico incompatível.')).toBe(true);
    expect(hub.element().querySelector('.note-study-hub-status')?.textContent).toBe(
      'Histórico incompatível.',
    );
    expect(hub.element().querySelector<HTMLButtonElement>('.note-study-start')?.disabled).toBe(true);
  });

  it('shows the requested minimum statistics, fixed heatmap, and accessible cells', async () => {
    const { hub, invoker } = mount();
    const { catalog, study } = await fixture();
    hub.openHub(A, invoker);
    expect(hub.showCatalog(1, catalog, study)).toBe(true);

    expect(stat(hub, 'Para revisar')).toBe('1');
    expect(stat(hub, 'Novas revisões')).toBe('1');
    // One basic source plus one reversible source: two cards, three
    // independently studyable directions.
    expect(stat(hub, 'Cartões')).toBe('2');
    expect(stat(hub, 'Revisões')).toBe('3');
    expect(stat(hub, 'Notas')).toBe('2');
    expect(stat(hub, 'Revisões hoje')).toBe('2');
    expect(stat(hub, 'Sequência atual')).toBe('2');
    expect(stat(hub, 'Maior sequência')).toBe('2');

    const cells = hub.element().querySelectorAll('.note-study-heat-cell');
    expect(cells).toHaveLength(365);
    const today = cells[cells.length - 1];
    expect(today.getAttribute('role')).toBe('img');
    expect(today.getAttribute('aria-label')).toContain('2 revisões');
    expect(today.getAttribute('title')).toBe(today.getAttribute('aria-label'));
    expect(today.getAttribute('data-level')).toBe('1');
  });

  it('filters Review Now, All, and Current Note and starts the exact snapshot', async () => {
    const { hub, handlers, invoker } = mount();
    const { catalog, study } = await fixture();
    hub.openHub(A, invoker);
    hub.showCatalog(1, catalog, study);

    expect(hub.element().querySelectorAll('.note-study-global-item')).toHaveLength(2);
    expect(hub.element().querySelector('.note-study-global-meta')?.textContent).toContain('Anatomia');
    expect(hub.element().querySelector('.note-study-global-meta')?.textContent).toContain(
      'Revisar agora',
    );

    hub.element().querySelector<HTMLButtonElement>('.note-study-filter-all')!.click();
    expect(hub.element().querySelectorAll('.note-study-global-item')).toHaveLength(3);
    expect(hub.element().textContent).toContain('em 3 dias');

    hub.element().querySelector<HTMLButtonElement>('.note-study-filter-current')!.click();
    expect(hub.element().querySelectorAll('.note-study-global-item')).toHaveLength(1);
    hub.element().querySelector<HTMLButtonElement>('.note-study-start')!.click();
    expect(handlers.onStart).toHaveBeenCalledTimes(1);
    expect(handlers.onStart.mock.calls[0][0]).toHaveLength(1);
    expect(handlers.onStart.mock.calls[0][1]).toBe(1);
  });

  it('shows an explicit empty state and owns Escape only while open', async () => {
    const { hub, invoker } = mount();
    hub.openHub(A, invoker);
    const empty = await buildGlobalCatalog([], emptyStudyState(), null, document);
    hub.showCatalog(1, empty, emptyStudyState());
    expect(hub.element().querySelector('.note-study-hub-status')?.textContent).toBe(
      'Nenhum flashcard nas notas.',
    );
    expect(hub.element().querySelector<HTMLButtonElement>('.note-study-start')?.disabled).toBe(true);

    hub.element().dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    expect(hub.isOpen()).toBe(false);
  });
});
