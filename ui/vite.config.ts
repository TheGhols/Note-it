import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';
import { QUICK_ACTIONS, renderQuickActionIcons } from './src/ui/icons.ts';

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

/** The six chosen icon files, by quick-action id. The only copy of each. */
function quickActionIcons(): Record<string, string> {
  const directory = fileURLToPath(new URL('../IconesNote-it', import.meta.url));
  const icons: Record<string, string> = {};
  for (const action of QUICK_ACTIONS) {
    icons[action.id] = readFileSync(`${directory}/${action.asset}`, 'utf8');
  }
  return icons;
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
      name: 'note-it-quick-action-icons',
      enforce: 'pre',
      transformIndexHtml(html) {
        return renderQuickActionIcons(html, quickActionIcons());
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
      renderedHtml: renderQuickActionIcons(indexHtml, quickActionIcons()),
      quickActionIcons: quickActionIcons(),
      // Read so a test can prove the six files are the only ones released from
      // the icon drop, and that every file the page uses is one of them.
      gitignore: readFileSync(fileURLToPath(new URL('../.gitignore', import.meta.url)), 'utf8'),
      mathSources: mathSources(),
      // The documented unit table has to be the table the application ships,
      // so the test that compares them reads the real file.
      featuresDoc: readFileSync(
        fileURLToPath(new URL('../docs/features.md', import.meta.url)),
        'utf8',
      ),
    },
  },
});
