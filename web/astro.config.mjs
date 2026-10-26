import { defineConfig } from 'astro/config';

// Halaman statis; backend Rust menyajikan dist/ dan /api di port yang sama.
// Saat `npm run dev`, /api diteruskan ke backend di :8080.
export default defineConfig({
  output: 'static',
  devToolbar: { enabled: false },
  server: { port: 4321 },
  vite: {
    server: { proxy: { '/api': 'http://127.0.0.1:8080' } },
  },
});
