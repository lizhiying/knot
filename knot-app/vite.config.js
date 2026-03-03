import { defineConfig } from 'vite'
import { svelte } from '@sveltejs/vite-plugin-svelte'
import path from 'path'

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  resolve: {
    alias: {
      $lib: path.resolve('./src/lib')
    }
  },
  server: {
    port: 14420,
    strictPort: true,
    watch: {
      // 忽略 Finder 产生的 .DS_Store、二进制文件和 src-tauri 目录
      ignored: ['**/.DS_Store', '**/bin/**', '**/*.dylib', '**/src-tauri/**', '**/target/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: ['es2021', 'chrome100', 'safari13'],
    minify: !process.env.TAURI_DEBUG ? 'esbuild' : false,
    sourcemap: !!process.env.TAURI_DEBUG,
  },
})
