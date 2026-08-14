import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'
import { buildSharedAliases } from '../shared-frontend/vite/aliases.ts'

export default defineConfig({
  plugins: [vue()],
  resolve: { alias: buildSharedAliases(import.meta.url) },
})
