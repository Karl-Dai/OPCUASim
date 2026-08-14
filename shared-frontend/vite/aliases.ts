import { createRequire } from 'node:module'
import { fileURLToPath, URL } from 'node:url'

// Vite alias map shared by both frontends. `@shared` points at this directory's
// parent, `@app` at the host project's src. Bare third-party imports (vue,
// @tauri-apps/api/*) are resolved against the host project's node_modules so
// files in shared-frontend — which has no node_modules of its own — can use
// them without bundler errors.
export function buildSharedAliases(hostImportMetaUrl: string) {
  const require = createRequire(hostImportMetaUrl)
  return {
    '@app': fileURLToPath(new URL('./src', hostImportMetaUrl)),
    '@shared': fileURLToPath(new URL('../shared-frontend', hostImportMetaUrl)),
    vue: require.resolve('vue'),
    '@tauri-apps/api/core': require.resolve('@tauri-apps/api/core'),
    '@tauri-apps/api/event': require.resolve('@tauri-apps/api/event'),
    '@tauri-apps/api/app': require.resolve('@tauri-apps/api/app'),
  }
}
