import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import { HEADER_ICONS, renderHeaderIcons } from './src/ui/icons.ts';

/** Every module of the math engine and the unit registry, by path. Read here
 *  for the same reason the stylesheet is: a test asserting what the engine
 *  cannot contain has to read the files the application actually ships. */
function mathSources(): Record<string, string> {
  const sources: Record<string, string> = {};
  for (const folder of ['math', 'units']) {
    const directory = fileURLToPath(new URL(`./src/${folder}`, import.meta.url));
    for (const name of readdirSync(directory)) {
      if (name.endsWith('.ts')) {
        sources[`${folder}/${name}`] = readFileSync(`${directory}/${name}`, 'utf8');
      }
    }
  }
  return sources;
}

/** The chosen icon files, by header-button id. The only copy of each. */
function headerIcons(): Record<string, string> {
  const directory = fileURLToPath(new URL('../IconesNote-it', import.meta.url));
  const icons: Record<string, string> = {};
  for (const icon of HEADER_ICONS) {
    icons[icon.id] = readFileSync(`${directory}/${icon.asset}`, 'utf8');
  }
  return icons;
}

/**
 * The host's own minimum note width.
 *
 * Read out of the file that declares it rather than copied, because it is the
 * budget the header bar has to fit inside: at this width the menu, the six
 * quick actions, the timer and the close cross must all still be on the note.
 * A second copy of the number would keep passing after the real one moved.
 */
function minNoteWidth(): number {
  const source = readFileSync(
    fileURLToPath(new URL('../src/layer_shell.rs', import.meta.url)),
    'utf8',
  );
  const match = /pub const MIN_NOTE_WIDTH: i32 = (\d+);/.exec(source);
  if (!match) {
    throw new Error('MIN_NOTE_WIDTH is no longer declared in src/layer_shell.rs');
  }
  return Number(match[1]);
}

const indexHtml = readFileSync(fileURLToPath(new URL('./index.html', import.meta.url)), 'utf8');

export default defineConfig({
  base: './',
  plugins: [
    {
      // The icons are drawn into the page here rather than fetched by it.
      // WebKitGTK enforces the page's `default-src 'self'`, and a CSS mask or
      // a background image is an image fetch: a `data:` URL is refused and a
      // relative one would have to be a file the bundle ships and the browser
      // then asks for. Inline SVG is neither — it is simply part of the
      // document, which is why it renders on the real application.
      name: 'note-it-header-icons',
      enforce: 'pre',
      transformIndexHtml(html) {
        return renderHeaderIcons(html, headerIcons());
      },
    },
    {
      name: 'note-it-local-file-bundle',
      enforce: 'post',
      transformIndexHtml(html) {
        return html.replace(/\s+crossorigin(?=[\s>])/g, '');
      },
    },
  ],
  build: {
    outDir: 'dist',
    assetsDir: 'assets',
    sourcemap: false,
    emptyOutDir: true,
  },
  test: {
    environment: 'happy-dom',
    // Vitest stubs every CSS import, `?raw` included, so the appearance tests
    // are handed the stylesheet's real text here. They assert against the file
    // the application actually ships rather than a second copy of its values.
    provide: {
      themeCss: readFileSync(
        fileURLToPath(new URL('./src/styles/theme.css', import.meta.url)),
        'utf8',
      ),
      indexHtml,
      // The page as the application receives it: the same transform the build
      // applies, over the same file, so a test of the icons is a test of what
      // ships rather than of the markup before it was finished.
      renderedHtml: renderHeaderIcons(indexHtml, headerIcons()),
      headerIcons: headerIcons(),
      // Read so a test can prove the released files are the only ones taken
      // from the icon drop, and that every file the page uses is one of them.
      gitignore: readFileSync(fileURLToPath(new URL('../.gitignore', import.meta.url)), 'utf8'),
      // The narrowest a note can be, so the header's budget is measured
      // against the real floor the host enforces.
      minNoteWidth: minNoteWidth(),
      mathSources: mathSources(),
      // The host and the page each carry a `stored note -> visible text`
      // projection, and two implementations only *described* as equivalent
      // drift. Both test suites are held to this one corpus, so they cannot.
      visibleTextCases: readFileSync(
        fileURLToPath(new URL('../tests/visible_text_cases.json', import.meta.url)),
        'utf8',
      ),
      // The documented unit table has to be the table the application ships,
      // so the test that compares them reads the real file.
      featuresDoc: readFileSync(
        fileURLToPath(new URL('../docs/features.md', import.meta.url)),
        'utf8',
      ),
    },
  },
});
