import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

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

export default defineConfig({
  base: './',
  plugins: [
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
      indexHtml: readFileSync(
        fileURLToPath(new URL('./index.html', import.meta.url)),
        'utf8',
      ),
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
