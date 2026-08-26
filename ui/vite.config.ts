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
  },
});
