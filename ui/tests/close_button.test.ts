import { beforeEach, describe, expect, inject, it } from 'vitest';
import type { HostToWebviewMessage, WebviewToHostMessage } from '../src/bridge/types.ts';
import '../tests/support/stylesheet.ts';

/**
 * The close button, driven through the wiring `main.ts` actually installs.
 *
 * This deliberately imports `main.ts` and clicks the real `#btn-close` in the
 * real `index.html` markup rather than mirroring the wiring here. A mirror
 * would have been written against the corrected shape and would have passed
 * throughout the defect it is here to catch: `saveAndClose` handed itself to
 * `deferDocumentEdit`, and because the barrier *runs* what it is given when
 * the document is free, the ordinary idle path re-entered `saveAndClose`
 * immediately and recursed until the stack gave out. The `RangeError` was
 * thrown before `sendMessage` was reached, so the host was never told
 * anything: the note stayed open and the page stalled while it unwound.
 *
 * The property under test is therefore the one the reader cares about —
 * pressing the X sends exactly one `save_and_close`, carrying the text the
 * note holds — and not which function calls which.
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

/**
 * Starts the page the way the host does and hands back what it sends.
 *
 * `main.ts` has no exports and boots on `DOMContentLoaded`, which is exactly
 * how the WebView starts it, so the test drives it the same way.
 */
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

describe('the note closes when its own X is pressed', () => {
  beforeEach(() => {
    document.documentElement.innerHTML = '<head></head><body></body>';
  });

  it('sends exactly one save_and_close, carrying the note, and does not throw', async () => {
    const { sent } = await startPage('conteúdo da nota');

    const button = document.getElementById('btn-close');
    expect(button, 'the shipped page has no #btn-close').not.toBeNull();

    sent.length = 0;
    // A stack overflow here is the defect itself: it escapes the listener
    // before anything is sent. Asserting on the messages alone would report it
    // as "nothing was sent" and hide why, so the throw is named.
    expect(() => button!.click()).not.toThrow();

    const closes = sent.filter((message) => message.type === 'save_and_close');
    expect(closes).toHaveLength(1);
    expect(closes[0]).toMatchObject({
      type: 'save_and_close',
      payload: { id: NOTE_ID },
    });
    expect((closes[0] as { payload: { content: string } }).payload.content).toContain(
      'conteúdo da nota',
    );
  });

  /**
   * The held path, which is the reason the call was written this way at all.
   *
   * A close asked for while an external write holds the document must not race
   * the commit, and must not be dropped either: it waits, and it goes the
   * moment the document is released — once, quoting the run the note is on by
   * then, not the one it was on when the X was pressed.
   */
  it('holds the close for an external write and sends it once on release', async () => {
    const { sent, host } = await startPage('antes');

    host({
      type: 'begin_external_write',
      payload: { requestId: REQUEST_ID, generation: 0 },
    });

    sent.length = 0;
    document.getElementById('btn-close')!.click();
    expect(
      sent.filter((message) => message.type === 'save_and_close'),
      'the close raced the commit instead of waiting',
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

    const closes = sent.filter((message) => message.type === 'save_and_close');
    expect(closes, 'the close was lost, or was sent more than once').toHaveLength(1);
    const payload = (closes[0] as { payload: { content: string; generation: number } })
      .payload;
    expect(payload.generation, 'the close quoted a run the host would refuse').toBe(1);
    expect(payload.content).toContain('depois');
  });
});
