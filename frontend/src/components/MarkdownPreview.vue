<template>
  <transition name="slide">
    <div v-if="path" class="md-drawer">
      <div class="md-header">
        <div class="md-title">
          <span class="ext-badge" :class="'ext-' + ext">{{ ext }}</span>
          <span class="md-name" :title="path">{{ filename }}</span>
        </div>
        <div class="md-actions">
          <button v-if="mode === 'view' && canEdit" class="btn ghost sm" @click="startEdit">编辑</button>
          <button v-if="mode === 'edit'" class="btn sm" @click="save" :disabled="saving">{{ saving ? '保存中…' : '保存' }}</button>
          <button v-if="mode === 'edit'" class="btn ghost sm" @click="cancelEdit">取消</button>
          <button v-if="mode === 'view'" class="btn ghost sm" @click="findSimilar" :disabled="findingSimilar" title="查找语义相似的文档">{{ findingSimilar ? '查找中…' : '相似文档' }}</button>
          <span v-if="msg" class="md-msg" :class="{ err: msgType === 'err' }">{{ msg }}</span>
          <button class="md-close" @click="$emit('close')">✕</button>
        </div>
      </div>
      <div v-if="loading" class="md-loading">加载中…</div>
      <template v-else>
        <div v-if="mode === 'view'" class="md-body markdown-body" v-html="rendered"></div>
        <textarea v-else v-model="editContent" class="md-editor" spellcheck="false" @keydown.esc="cancelEdit"></textarea>
      </template>
      <!-- Find Similar results panel -->
      <div v-if="similarHits.length" class="similar-panel">
        <div class="similar-title">相似文档（{{ similarHits.length }}）</div>
        <div v-for="(s, i) in similarHits" :key="i" class="similar-hit" @click="$emit('preview', s)">
          <span class="ext-badge" :class="'ext-' + s.ext">{{ s.ext }}</span>
          <span class="similar-name">{{ s.rel_path?.split('/').pop() || s.rel_path }}</span>
          <span class="similar-score">{{ Math.round(s.rank * 100) }}%</span>
        </div>
      </div>
    </div>
  </transition>
</template>

<script setup>
import { ref, watch, computed } from 'vue'
import { marked } from 'marked'
import { api } from '../api.js'

const props = defineProps({
  path: { type: String, default: '' },
  file_id: { type: Number, default: 0 },
})
const emit = defineEmits(['close', 'preview'])

const loading = ref(false)
const mode = ref('view')
const content = ref('')
const editContent = ref('')
const ext = ref('')
const msg = ref('')
const msgType = ref('')
const saving = ref(false)
const findingSimilar = ref(false)
const similarHits = ref([])

const EDITABLE_EXTS = ['md', 'markdown', 'txt', 'csv', 'json', 'html', 'htm', 'xml', 'log', 'yml', 'yaml', 'toml', 'ini', 'conf']
const canEdit = computed(() => EDITABLE_EXTS.includes(ext.value))
const filename = computed(() => {
  if (!props.path) return ''
  const parts = props.path.replace(/\\/g, '/').split('/')
  return parts[parts.length - 1] || ''
})
const rendered = computed(() => {
  if (!content.value) return '<p class="md-empty">无内容</p>'
  try {
    if (ext.value === 'md' || ext.value === 'markdown') {
      return marked.parse(content.value, { breaks: true, gfm: true })
    }
    // For other text files, render in <pre>
    return '<pre>' + escapeHtml(content.value) + '</pre>'
  } catch (e) {
    return '<pre>' + escapeHtml(content.value) + '</pre>'
  }
})

function escapeHtml(s) {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
}

watch(() => props.path, async (p) => {
  if (!p) { content.value = ''; similarHits.value = []; return }
  mode.value = 'view'
  similarHits.value = []
  msg.value = ''
  loading.value = true
  try {
    const r = await api.readFile(p)
    if (r.ok) {
      content.value = r.content || ''
      ext.value = r.ext || ''
    } else {
      content.value = ''
      ext.value = r.ext || ''
      msg.value = r.error || '读取失败'
      msgType.value = 'err'
    }
  } catch (e) {
    msg.value = e.message
    msgType.value = 'err'
  } finally {
    loading.value = false
  }
}, { immediate: true })

function startEdit() {
  editContent.value = content.value
  mode.value = 'edit'
  msg.value = ''
}

function cancelEdit() {
  mode.value = 'view'
  editContent.value = ''
  msg.value = ''
}

async function save() {
  saving.value = true
  msg.value = ''
  try {
    await api.saveFile(props.path, editContent.value)
    content.value = editContent.value
    mode.value = 'view'
    msg.value = '已保存'
    msgType.value = 'ok'
    setTimeout(() => { msg.value = '' }, 2000)
  } catch (e) {
    msg.value = '保存失败: ' + e.message
    msgType.value = 'err'
  } finally {
    saving.value = false
  }
}

async function findSimilar() {
  if (!props.file_id) {
    msg.value = '需要向量索引支持，请先在搜索页生成向量索引'
    msgType.value = 'err'
    return
  }
  findingSimilar.value = true
  msg.value = ''
  try {
    const r = await api.findSimilar(props.file_id, 10)
    if (r.error) {
      msg.value = r.error
      msgType.value = 'err'
    } else if (r.hits && r.hits.length) {
      similarHits.value = r.hits
    } else {
      msg.value = '未找到相似文档'
      msgType.value = 'err'
    }
  } catch (e) {
    msg.value = e.message
    msgType.value = 'err'
  } finally {
    findingSimilar.value = false
  }
}
</script>

<style scoped>
.md-drawer {
  position: fixed;
  top: 0; right: 0;
  width: 520px;
  height: 100%;
  background: var(--bg-elev);
  border-left: 1px solid var(--border);
  z-index: 200;
  display: flex;
  flex-direction: column;
  box-shadow: -8px 0 24px rgba(0,0,0,0.3);
}
.md-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 14px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.md-title {
  display: flex;
  align-items: center;
  gap: 8px;
  overflow: hidden;
}
.md-name {
  font-size: 13px;
  font-weight: 500;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 260px;
}
.md-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}
.md-msg {
  font-size: 11px;
  color: var(--ok);
}
.md-msg.err { color: var(--err); }
.md-close {
  background: none;
  border: none;
  color: var(--muted);
  font-size: 16px;
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
}
.md-close:hover { color: var(--fg); background: var(--bg-card); }
.md-loading {
  padding: 40px;
  text-align: center;
  color: var(--muted);
}
.md-body {
  flex: 1;
  overflow-y: auto;
  padding: 16px 20px;
  font-size: 13px;
  line-height: 1.7;
  color: var(--fg);
}
.md-editor {
  flex: 1;
  background: var(--bg);
  border: none;
  color: var(--fg);
  font-family: 'JetBrains Mono', 'Consolas', monospace;
  font-size: 13px;
  line-height: 1.7;
  padding: 16px 20px;
  resize: none;
  outline: none;
}
.md-empty { color: var(--muted); font-style: italic; }

/* ext badges reuse same styles as Search */
.ext-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 34px;
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 10px;
  font-weight: 700;
  text-transform: uppercase;
  letter-spacing: .5px;
  font-family: 'JetBrains Mono', 'Consolas', monospace;
  flex-shrink: 0;
}
.ext-md, .ext-markdown { background: rgba(96,165,250,.14); color: #60a5fa; }
.ext-txt, .ext-csv, .ext-json { background: rgba(139,145,153,.14); color: var(--muted); }

/* Similar results panel */
.similar-panel {
  border-top: 1px solid var(--border);
  padding: 10px 14px;
  max-height: 200px;
  overflow-y: auto;
  flex-shrink: 0;
}
.similar-title {
  font-size: 11px;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: .5px;
  margin-bottom: 6px;
}
.similar-hit {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 5px 8px;
  border-radius: 4px;
  cursor: pointer;
}
.similar-hit:hover { background: var(--bg-card); }
.similar-name {
  font-size: 12px;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.similar-score {
  font-size: 11px;
  color: var(--accent);
  font-family: 'JetBrains Mono', 'Consolas', monospace;
  flex-shrink: 0;
}

/* Slide transition */
.slide-enter-active, .slide-leave-active {
  transition: transform .2s ease;
}
.slide-enter-from, .slide-leave-to {
  transform: translateX(100%);
}
</style>

<!-- Markdown rendered content styling (global, not scoped) -->
<style>
.markdown-body h1 { font-size: 20px; font-weight: 700; margin: 16px 0 10px; color: var(--fg); border-bottom: 1px solid var(--border); padding-bottom: 6px; }
.markdown-body h2 { font-size: 17px; font-weight: 600; margin: 14px 0 8px; color: var(--fg); }
.markdown-body h3 { font-size: 15px; font-weight: 600; margin: 12px 0 6px; color: var(--fg); }
.markdown-body h4, .markdown-body h5, .markdown-body h6 { font-size: 14px; font-weight: 600; margin: 10px 0 6px; color: var(--fg); }
.markdown-body p { margin: 8px 0; }
.markdown-body a { color: var(--accent); text-decoration: underline; }
.markdown-body ul, .markdown-body ol { margin: 8px 0; padding-left: 24px; }
.markdown-body li { margin: 4px 0; }
.markdown-body code {
  font-family: 'JetBrains Mono', 'Consolas', monospace;
  font-size: 12px;
  background: var(--bg-card);
  padding: 1px 4px;
  border-radius: 3px;
}
.markdown-body pre {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
  padding: 12px;
  overflow-x: auto;
  margin: 10px 0;
}
.markdown-body pre code {
  background: none;
  padding: 0;
  font-size: 12px;
}
.markdown-body blockquote {
  border-left: 3px solid var(--accent);
  margin: 10px 0;
  padding: 4px 12px;
  color: var(--muted);
}
.markdown-body table {
  border-collapse: collapse;
  margin: 10px 0;
  font-size: 12px;
}
.markdown-body th, .markdown-body td {
  border: 1px solid var(--border);
  padding: 5px 10px;
}
.markdown-body th { background: var(--bg-card); font-weight: 600; }
.markdown-body img { max-width: 100%; }
.markdown-body hr { border: none; border-top: 1px solid var(--border); margin: 16px 0; }
.markdown-body strong { font-weight: 600; }
.markdown-body del { color: var(--muted); }
</style>
