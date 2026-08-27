import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'vite';

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
    },
  },
});
