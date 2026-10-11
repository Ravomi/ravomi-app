import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';

export default defineConfig({
  plugins: [react()],
  define: { 'process.env.NODE_ENV': '"production"' },
  build: {
    outDir: 'dist-editor',
    lib: { entry: 'src/canvas-editor.tsx', name: 'HMCanvasEditor', formats: ['iife'], fileName: () => 'editor.js' },
  },
});
