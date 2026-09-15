import { beforeEach, describe, expect, inject, it } from 'vitest';
import type { HostToWebviewMessage, WebviewToHostMessage } from '../src/bridge/types.ts';
import './support/stylesheet.ts';

/**
 * How many `metadata_changed` messages one metadata change produces.
 *
 * `tests/metadata.test.ts` covers the panel itself, but it hands the panel a
 * stubbed `save`, so it cannot see what `main.ts` does with the call — and what
 * it did was send the change twice. `deferDocumentEdit` was read as a boolean
 * test: the handler passed the send as the deferred action *and* repeated it
 * underneath, so with the document free the barrier ran the action, answered
 * `false`, and the fall-through sent the very same change a second time.
 *
 * So this drives the real path instead: the shipped `index.html`, `main.ts`'s
 * own wiring, the note menu, the metadata panel, and the panel's own add-tag
 * form. The property is a count, because that is what went wrong.
 */

const NOTE_ID = '11111111-2222-4333-8444-555555555555';
const REQUEST_ID = 'aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee';

const INDEX_HTML = inject('indexHtml');

/** The `<body>` of the shipped page, without its module `<script>` tags. */
function pageBody(): string {
  const body = /<body[^>]*>([\s\S]*)<\/body>/i.exec(INDEX_HTML);
  if (!body) throw new Error('index.html has no <body>');
  return body[1].replace(/<script[\s\S]*?<\/script>/gi, '');
}

function loadNote(content: string): HostToWebviewMessage {
  return {
    type: 'load_note',
    payload: {
      id: NOTE_ID,
      content,
      color: 'yellow',
      paperType: 'blank',
      paperIntensity: 'normal',
      fontSize: 15,
      metadata: { tags: [], properties: [] },
      collapsed: false,
      createdAt: '2026-09-01T10:00:00Z',
      updatedAt: '2026-09-01T10:05:00Z',
      zoomPercent: 100,
      layerMode: 'overlay',
      theme: 'system',
      uiScalePercent: 100,
      timer: null,
      captureDelimiter: 'blankLine',
      generation: 0,
    },
  };
}

async function startPage(content: string) {
  document.documentElement.innerHTML = `<head></head><body>${pageBody()}</body>`;

  const sent: WebviewToHostMessage[] = [];
  (window as unknown as { webkit: unknown }).webkit = {
    messageHandlers: {
      noteItHost: {
        postMessage: (raw: string) => sent.push(JSON.parse(raw) as WebviewToHostMessage),
      },
    },
  };

  await import('../src/main.ts');
  document.dispatchEvent(new Event('DOMContentLoaded'));

  const handle = (window as unknown as { handleHostMessage?: (m: unknown) => void })
    .handleHostMessage;
  if (!handle) throw new Error('the page installed no host-message handler');
  handle(loadNote(content));

  return { sent, host: handle };
}

function click(element: Element): void {
  element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
}

/** Opens the metadata panel the way a reader does: the menu, then its entry. */
function openMetadataPanel(): void {
  const menu = document.getElementById('btn-menu');
  if (!menu) throw new Error('the shipped page has no #btn-menu');
  click(menu);

  const entry = [...document.querySelectorAll('.note-menu-item')].find(
    (item) => item.textContent?.trim().startsWith('Metadados'),
  );
  if (!entry) throw new Error('the note menu has no Metadados entry');
  click(entry);
}

/** Adds a tag through the panel's own form — one action by one reader. */
function addTag(value: string): void {
  const form = document.querySelector('.note-metadata .note-metadata-add') as
    | HTMLFormElement
    | null;
  if (!form) throw new Error('the metadata panel has no add-tag form');
  const input = form.querySelector('input') as HTMLInputElement | null;
  if (!input) throw new Error('the add-tag form has no input');
  input.value = value;
  form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }));
}

function metadataMessages(sent: WebviewToHostMessage[]) {
  return sent.filter((message) => message.type === 'metadata_changed');
}

describe('a metadata change is sent once', () => {
  beforeEach(() => {
    document.documentElement.innerHTML = '<head></head><body></body>';
  });

  it('sends exactly one metadata_changed when the document is free', async () => {
    const { sent } = await startPage('corpo da nota');

    openMetadataPanel();
    sent.length = 0;
    addTag('Cardiologia');

    const changes = metadataMessages(sent);
    expect(changes, 'one change by the reader must reach the host once').toHaveLength(1);
    expect(changes[0]).toMatchObject({
      type: 'metadata_changed',
      payload: { id: NOTE_ID, generation: 0, tags: ['Cardiologia'] },
    });
    expect(
      (changes[0] as { payload: { content: string } }).payload.content,
    ).toContain('corpo da nota');
  });

  /**
   * The held path, which must not regress in the other direction: a change made
   * while an external write holds the document waits for the commit rather than
   * carrying the pre-commit body back with the tag — and then goes exactly once.
   */
  it('holds the change for an external write and sends it once on release', async () => {
    const { sent, host } = await startPage('antes');

    openMetadataPanel();

    host({
      type: 'begin_external_write',
      payload: { requestId: REQUEST_ID, generation: 0 },
    });

    sent.length = 0;
    addTag('Urgência');
    expect(
      metadataMessages(sent),
      'the change raced the commit instead of waiting',
    ).toHaveLength(0);

    host({
      type: 'apply_external_document',
      payload: {
        id: NOTE_ID,
        requestId: REQUEST_ID,
        generation: 1,
        content: 'depois',
        metadata: { tags: [], properties: [] },
        createdAt: '2026-09-01T10:00:00Z',
        updatedAt: '2026-09-01T10:10:00Z',
      },
    });

    const changes = metadataMessages(sent);
    expect(changes, 'the change was lost, or was sent more than once').toHaveLength(1);
    const payload = (changes[0] as {
      payload: { content: string; generation: number; tags: string[] };
    }).payload;
    expect(payload.generation, 'the change quoted a run the host would refuse').toBe(1);
    expect(payload.content).toContain('depois');
    expect(payload.tags).toEqual(['Urgência']);
  });
});
