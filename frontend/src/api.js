// Bridge to the Memex Rust core.
// Two transports, in priority order:
//   1. Tauri `invoke` — when running inside the desktop shell (M2+).
//   2. HTTP bridge — when opened in a plain browser/preview, fall back to the
//      local `memex serve` JSON server on 127.0.0.1:18321.
let tauri = null
try {
  tauri = window.__TAURI__ ?? null
} catch (_) {}

const HTTP_BASE = window.location.port === '5173' ? 'http://127.0.0.1:18321' : ''

async function http(cmd, args = {}) {
  const r = await fetch(`${HTTP_BASE}/api`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ cmd, args })
  })
  if (!r.ok) {
    let msg = `HTTP ${r.status}`
    try {
      const j = await r.json()
      if (j.error) msg = j.error
    } catch (_) {
      msg = await r.text()
    }
    throw new Error(msg)
  }
  return r.json()
}

async function call(cmd, args = {}) {
  if (tauri?.invoke) return tauri.invoke(cmd, args)
  return http(cmd, args)
}

export const api = {
  info: () => call('cmd_info'),
  folders: {
    list: () => call('cmd_folders_list'),
    add: (path) => call('cmd_folders_add', { path }),
    remove: (id) => call('cmd_folders_remove', { id })
  },
  index: {
    run: (mode = 'incremental') => call('cmd_index_run', { mode }),
    progress: () => call('cmd_index_progress')
  },
  reindex: () => call('cmd_index_run'),
  search: (query, limit = 20, hybrid = false, tagFilter = {}, filters = {}) => call('cmd_search', { query, limit, hybrid, ...tagFilter, ...filters }),
  readFile: (path) => call('cmd_read_file', { path }),
  saveFile: (path, content) => call('cmd_save_file', { path, content }),
  findSimilar: (fileId, limit = 10) => call('cmd_find_similar', { file_id: fileId, limit }),
  openFile: (path) => call('cmd_open_file', { path }),
  openFolder: (path) => call('cmd_open_folder', { path }),
  pickFolder: () => call('cmd_pick_folder'),
  vectorStatus: () => call('cmd_vector_status'),
  embed: () => call('cmd_embed'),
  embedProgress: () => call('cmd_embed_progress'),
  filter: {
    get: () => call('cmd_filter_get'),
    save: (rules) => call('cmd_filter_save', { rules })
  },
  tags: {
    all: () => call('cmd_tags'),
    dims: () => call('cmd_dims'),
    summary: (dim) => call('cmd_tag_summary', { dim }),
    filesByTag: (dim, tag, page = 1, pageSize = 50) => call('cmd_files_by_tag', { dim, tag, page, page_size: pageSize }),
  },
  confidence: {
    list: () => call('cmd_confidence')
  },
  dedup: {
    scan: () => call('cmd_dedup_scan'),
    clean: (hashes) => call('cmd_dedup_clean', { hashes }),
  },
  mcp: {
    prompt: () => call('cmd_mcp_prompt'),
    httpPrompt: () => call('cmd_mcp_http_prompt'),
    agentPrompt: () => call('cmd_gen_agent_prompt')
  },
  update: {
    check: () => call('cmd_check_update'),
    autoResult: () => call('cmd_auto_check_result'),
    download: (url, filename) => call('cmd_download_update', { url, filename }),
    progress: () => call('cmd_update_progress'),
    install: () => call('cmd_install_update'),
  },
  // ---- Data export/import ----
  export: {
    json: () => call('cmd_export'),
    csv: () => call('cmd_export_csv'),
    memories: () => call('cmd_export_memories'),
    importTags: (tags) => call('cmd_import_tags', { tags }),
    listNotes: () => call('cmd_list_notes'),
  },
  // ---- Agent memory management ----
  memory: {
    list: () => call('cmd_export_memories'),
    save: (key, value, namespace = 'default', note = null) => call('cmd_save_memory', { key, value, namespace, note }),
    delete: (key, namespace = '') => call('cmd_delete_memory', { key, namespace }),
  },
  notes: {
    list: () => call('cmd_list_notes'),
    read: (title) => call('cmd_read_note', { title }),
    delete: (title) => call('cmd_delete_note', { title }),
  },
}
