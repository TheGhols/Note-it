/**
 * The header's quick actions and the icon each one is drawn with.
 *
 * The icons are real files from `IconesNote-it/`, and they are the only copy:
 * the build reads each file, normalises it here and writes the resulting
 * `<svg>` straight into `index.html`, so the shipped page carries the drawing
 * itself rather than a reference to something that has to be fetched.
 *
 * That matters because the previous attempt did not survive WebKitGTK. It
 * painted each icon as a CSS `mask-image` pointing at a `data:` URL, and the
 * page's own Content-Security-Policy is `default-src 'self'`: a mask is fetched
 * as an image, `data:` is not `'self'`, and the request was blocked with no
 * error anywhere the tests could see. Every icon came out blank on the real
 * application while the unit tests, which only read the stylesheet, passed.
 * Inline SVG is not fetched at all, which is why the hamburger and the close
 * cross have always rendered, and it is what these six use now.
 */

/** One header button: what it is called, and what it opens. */
export interface QuickAction {
  /** Identifier used by the placeholder in `index.html`. */
  readonly id: string;
  /** DOM id of the button that carries the icon. */
  readonly buttonId: string;
  /** Accessible name and tooltip. Portuguese, like the rest of the interface. */
  readonly label: string;
  /** The existing menu panel this action opens. No action has logic of its own. */
  readonly panel: 'paper' | 'textSize' | 'textColor' | 'highlight' | 'blocks' | 'search';
  /** File under `IconesNote-it/`, relative to that directory. */
  readonly asset: string;
}

/**
 * The six approved quick actions, in the order they appear in the bar.
 *
 * Every asset is single-toned once normalised: none of them relies on a
 * translucent shape sitting under an opaque one of the same colour, so the
 * whole icon is drawn in the button's own colour and the whole icon clears the
 * contrast floor on every paper. That is a real constraint on the choice, not a
 * coincidence — `tests/quick_actions.test.ts` measures it.
 */
export const QUICK_ACTIONS: readonly QuickAction[] = [
  {
    id: 'note-color',
    buttonId: 'btn-note-color',
    label: 'Cor da nota',
    panel: 'paper',
    asset: 'bucket-svgrepo-com.svg',
  },
  {
    id: 'text-size',
    buttonId: 'btn-text-size',
    label: 'Tamanho do texto',
    panel: 'textSize',
    asset: 'larger-text-svgrepo-com.svg',
  },
  {
    id: 'text-color',
    buttonId: 'btn-text-color',
    label: 'Cor do texto',
    panel: 'textColor',
    asset: 'text-svgrepo-com.svg',
  },
  {
    id: 'highlight',
    buttonId: 'btn-highlight',
    label: 'Marca-texto',
    panel: 'highlight',
    asset: 'edite-svgrepo-com.svg',
  },
  {
    id: 'blocks',
    buttonId: 'btn-blocks',
    label: 'Blocos',
    panel: 'blocks',
    asset: 'Category.svg',
  },
  {
    id: 'search',
    buttonId: 'btn-search',
    label: 'Buscar',
    panel: 'search',
    asset: 'Search.svg',
  },
];

/** Paint values that mean "no paint" and must survive untouched. */
const UNPAINTED = new Set(['none', 'currentcolor', 'transparent', 'inherit']);

/**
 * An icon file, rewritten into an inline `<svg>` the note can wear.
 *
 * Three things change, and nothing else:
 *
 * - every literal colour becomes `currentColor`, so one file serves yellow
 *   paper and black paper and both interface themes;
 * - every partial `opacity` is dropped. These are two-tone icons drawn for a
 *   28 px toolbar; at the size a note's header uses them the faint half reads
 *   as a rendering fault, and 40 % ink does not clear the contrast floor on
 *   pale paper. Full strength is both more legible and measurable;
 * - `id` attributes go. Six icons are inlined into one document, and the
 *   supplied files carry ids like `Search` and `Stroke 1` that would collide.
 *
 * The intrinsic `width`/`height` are removed from the root so the stylesheet
 * sizes the icon; the `viewBox` is what actually matters and it stays.
 */
export function normalizeIconSvg(source: string): string {
  const document = source
    .replace(/<\?xml[\s\S]*?\?>/g, '')
    .replace(/<!DOCTYPE[\s\S]*?>/gi, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .trim();

  if (!document.startsWith('<svg') || !document.endsWith('</svg>')) {
    throw new Error('an icon asset must be a single <svg> element');
  }

  const painted = document
    .replace(/(?<![-\w])(fill|stroke)="([^"]*)"/g, (whole, property: string, value: string) =>
      UNPAINTED.has(value.trim().toLowerCase()) ? whole : `${property}="currentColor"`,
    )
    .replace(/\s(?<![-\w])opacity="[^"]*"/g, '')
    .replace(/\sid="[^"]*"/g, '');

  const rootTag = /^<svg\b[^>]*>/.exec(painted);
  if (!rootTag) {
    throw new Error('an icon asset must open with an <svg> tag');
  }

  const root = `${rootTag[0]
    .replace(/\s(?:width|height)="[^"]*"/g, '')
    .replace(/>$/, '')} aria-hidden="true" focusable="false">`;

  return `${root}${painted.slice(rootTag[0].length)}`.replace(/>\s+</g, '><');
}

/**
 * Writes the six icons into the page.
 *
 * `index.html` carries an empty `<span data-quick-icon="...">` for each action
 * and no drawing at all, so the markup never holds a second copy of a path that
 * also lives in `IconesNote-it/`. The build fills them in; the test suite runs
 * the same function over the same file, so what it checks is what ships.
 *
 * Throws rather than skipping: an action with no icon is a blank button, and a
 * blank button is exactly the defect this replaces.
 */
export function renderQuickActionIcons(html: string, icons: Record<string, string>): string {
  const filled = new Set<string>();

  const rendered = html.replace(
    /(<span\b[^>]*\bdata-quick-icon="([\w-]+)"[^>]*>)\s*<\/span>/g,
    (_whole, open: string, id: string) => {
      const source = icons[id];
      if (source === undefined) {
        throw new Error(`no icon asset supplied for the quick action "${id}"`);
      }
      filled.add(id);
      return `${open}${normalizeIconSvg(source)}</span>`;
    },
  );

  const missing = QUICK_ACTIONS.filter((action) => !filled.has(action.id));
  if (missing.length > 0) {
    throw new Error(
      `the page has no icon placeholder for: ${missing.map((a) => a.id).join(', ')}`,
    );
  }

  return rendered;
}
