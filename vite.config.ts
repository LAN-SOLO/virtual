import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

// Tauri expects a fixed dev port (see src-tauri/tauri.conf.json devUrl)
export default defineConfig({
  plugins: [react()],
  // relative Pfade: die Web-Version darf auch unter einem Unterpfad liegen
  base: './',
  clearScreen: false,
  server: {
    port: 1436,
    strictPort: true,
  },
  build: {
    target: 'es2021',
    outDir: 'dist',
  },
});
