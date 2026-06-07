import { defineConfig } from 'vite';
import solidPlugin from 'vite-plugin-solid';
import tailwindcss from '@tailwindcss/vite';
import { viteSingleFile } from 'vite-plugin-singlefile';

export default defineConfig({
  plugins: [solidPlugin(), tailwindcss(), viteSingleFile()],
  build: {
    target: 'esnext',
  },
});
