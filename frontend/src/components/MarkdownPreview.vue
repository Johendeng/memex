<template>
  <transition name="modal">
    <div v-if="path" class="modal-mask" @click.self="$emit('close')">
      <div class="modal-shell">
        <!-- header -->
        <div class="md-header">
          <div class="md-title">
            <span class="ext-badge" :class="'ext-' + ext">{{ ext }}</span>
            <span class="md-name" :title="path">{{ filename }}</span>
          </div>
          <div class="md-actions">
            <template v-if="mode === 'view'">
              <button v-if="canEdit" class="btn sm" @click="startEdit">✏️ 编辑</button>
              <button class="btn ghost sm" @click="findSimilar" :disabled="findingSimilar" title="查找语义相似的文档">{{ findingSimilar ? '查找中…' : '🔍 相似文档' }}</button>
            </template>
            <template v-else>
              <label class="live-toggle" title="编辑时实时预览">
                <input type="checkbox" v-model="livePreview"> 预览
              </label>
              <button class="btn ghost sm" @click="cancelEdit" :disabled="saving">取消</button>
              <button class="btn sm" @click="save" :disabled="saving">{{ saving ? '保存中…' : '💾 保存' }}</button>
            </template>
            <span v-if="msg" class="md-msg" :class="{ err: msgType === 'err' }">{{ msg }}</span>
            <button class="md-close" @click="$emit('close')">✕</button>
          </div>
        </div>

        <!-- toolbar (edit mode only) -->
        <div v-if="mode === 'edit'" class="md-toolbar">
          <div class="tb-group">
            <button class="tb-btn" title="标题 1 (Ctrl+1)" @mousedown.prevent @click="mdHeading(1)"><b>H1</b></button>
            <button class="tb-btn" title="标题 2 (Ctrl+2)" @mousedown.prevent @click="mdHeading(2)"><b>H2</b></button>
            <button class="tb-btn" title="标题 3 (Ctrl+3)" @mousedown.prevent @click="mdHeading(3)"><b>H3</b></button>
          </div>
          <span class="tb-sep"></span>
          <div class="tb-group">
            <button class="tb-btn" title="加粗 (Ctrl+B)" @mousedown.prevent @click="mdWrap('**','**')"><b>B</b></button>
            <button class="tb-btn" title="斜体 (Ctrl+I)" @mousedown.prevent @click="mdWrap('*','*')"><i>I</i></button>
            <button class="tb-btn" title="删除线" @mousedown.prevent @click="mdWrap('~~','~~')"><s>S</s></button>
            <button class="tb-btn mono" title="行内代码" @mousedown.prevent @click="mdWrap('`','`')">&lt;&gt;</button>
          </div>
          <span class="tb-sep"></span>
          <div class="tb-group">
            <button class="tb-btn" title="无序列表" @mousedown.prevent @click="mdLinePrefix('- ')">•≡</button>
            <button class="tb-btn" title="有序列表" @mousedown.prevent @click="mdLinePrefix('1. ')">1.</button>
            <button class="tb-btn" title="任务列表" @mousedown.prevent @click="mdLinePrefix('- [ ] ')">☑</button>
            <button class="tb-btn" title="引用" @mousedown.prevent @click="mdLinePrefix('> ')">❝</button>
          </div>
          <span class="tb-sep"></span>
          <div class="tb-group">
            <button class="tb-btn" title="链接 (Ctrl+K)" @mousedown.prevent @click="mdLink()">🔗</button>
            <button class="tb-btn" title="图片" @mousedown.prevent @click="mdImage()">🖼</button>
            <button class="tb-btn mono" title="代码块" @mousedown.prevent @click="mdCodeBlock()">{ }</button>
            <button class="tb-btn" title="表格" @mousedown.prevent @click="mdTable()">⊞</button>
            <button class="tb-btn" title="分隔线" @mousedown.prevent @click="mdInsertText('\n\n---\n\n')">—</button>
          </div>
        </div>

        <!-- body -->
        <div v-if="loading" class="md-loading">加载中…</div>
        <div v-else class="md-body-row">
          <!-- view mode -->
          <div v-if="mode === 'view'" class="md-body markdown-body" v-html="rendered"></div>
          <!-- edit mode -->
          <template v-else>
            <textarea
              ref="editorRef"
              v-model="editContent"
              class="md-editor"
              spellcheck="false"
              @keydown.esc="cancelEdit"
              @keydown.ctrl.s.prevent="save"
              @keydown.meta.s.prevent="save"
              @keydown.ctrl.b.prevent="mdWrap('**','**')"
              @keydown.ctrl.i.prevent="mdWrap('*','*')"
              @keydown.ctrl.k.prevent="mdLink()"
              @keydown.ctrl.1.prevent="mdHeading(1)"
              @keydown.ctrl.2.prevent="mdHeading(2)"
              @keydown.ctrl.3.prevent="mdHeading(3)"
            ></textarea>
            <div v-if="livePreview" class="md-split-preview markdown-body" v-html="editRendered"></div>
          </template>
        </div>

        <!-- status bar -->
        <div v-if="mode === 'edit'" class="md-statusbar">
          <span>{{ editContent.length }} 字符 · {{ lineCount }} 行</span>
          <span class="status-hint">Ctrl+S 保存 · Esc 取消</span>
        </div>

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
    </div>
  </transition>
</template>

<script setup>
import { ref, watch, computed, nextTick } from 'vue'
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
const livePreview = ref(true)
const editorRef = ref(null)

const EDITABLE_EXTS = ['md', 'markdown', 'txt', 'csv', 'json', 'html', 'htm', 'xml', 'log', 'yml', 'yaml', 'toml', 'ini', 'conf']
const canEdit = computed(() => EDITABLE_EXTS.includes(ext.value))
const filename = computed(() => {
  if (!props.path) return ''
  const parts = props.path.replace(/\\/g, '/').split('/')
  return parts[parts.length - 1] || ''
})
const isMarkdown = computed(() => ext.value === 'md' || ext.value === 'markdown')
const lineCount = computed(() => editContent.value.split('\n').length)

function renderMd(text) {
  try {
    return marked.parse(text || '', { breaks: true, gfm: true })
  } catch (e) {
    return '<pre>' + escapeHtml(text) + '</pre>'
  }
}
const rendered = computed(() => {
  if (!content.value) return '<p class="md-empty">无内容</p>'
  if (isMarkdown.value) return renderMd(content.value)
  return '<pre>' + escapeHtml(content.value) + '</pre>'
})
const editRendered = computed(() => isMarkdown.value ? renderMd(editContent.value) : '<pre>' + escapeHtml(editContent.value) + '</pre>')

function escapeHtml(s) {
  return String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
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

// ---------------- editor helpers ----------------

/// Wrap/unwrap markdown markers around the current selection (bold, italic, code…)
function mdWrap(before, after) {
  const ta = editorRef.value
  if (!ta) return
  const { selectionStart: s, selectionEnd: e } = ta
  const sel = editContent.value.slice(s, e)
  // toggle off: same marker sits immediately around the selection
  const beforeText = editContent.value.slice(Math.max(0, s - before.length), s)
  const afterText = editContent.value.slice(e, e + after.length)
  if (sel.length > 0 && beforeText === before && afterText === after) {
    editContent.value = editContent.value.slice(0, s - before.length) + sel +
      editContent.value.slice(e + after.length)
    nextTick(() => ta.setSelectionRange(s - before.length, e - before.length))
    return
  }
  const out = before + sel + after
  editContent.value = editContent.value.slice(0, s) + out + editContent.value.slice(e)
  nextTick(() => {
    ta.focus()
    if (sel.length) ta.setSelectionRange(s + before.length, s + before.length + sel.length)
    else ta.setSelectionRange(s + before.length, s + before.length)
  })
}

/// Prefix / unprefix every line in the selection (lists, quotes…)
function mdLinePrefix(prefix) {
  const ta = editorRef.value
  if (!ta) return
  const { selectionStart: s, selectionEnd: e } = ta
  const text = editContent.value
  const ls = text.lastIndexOf('\n', s - 1) + 1
  const nl = text.indexOf('\n', e)
  const le = nl === -1 ? text.length : nl
  const lines = text.slice(ls, le).split('\n')
  const all = lines.every(l => l.startsWith(prefix))
  const out = lines.map(l => all ? l.slice(prefix.length) : prefix + l).join('\n')
  editContent.value = text.slice(0, ls) + out + text.slice(le)
  nextTick(() => {
    ta.focus()
    ta.setSelectionRange(ls, ls + out.length)
  })
}

/// Set heading level on the current line (replaces an existing marker)
function mdHeading(level) {
  const ta = editorRef.value
  if (!ta) return
  const { selectionStart: s } = ta
  const text = editContent.value
  const ls = text.lastIndexOf('\n', s - 1) + 1
  const nl = text.indexOf('\n', s)
  const le = nl === -1 ? text.length : nl
  let line = text.slice(ls, le)
  const marker = '#'.repeat(level) + ' '
  line = line.replace(/^#{1,6}\s*/, '')
  editContent.value = text.slice(0, ls) + marker + line + text.slice(le)
  nextTick(() => {
    ta.focus()
    ta.setSelectionRange(ls + marker.length, ls + marker.length + line.length)
  })
}

function mdLink() {
  const ta = editorRef.value
  if (!ta) return
  const { selectionStart: s, selectionEnd: e } = ta
  const sel = editContent.value.slice(s, e)
  const label = sel || '链接文字'
  const url = sel.startsWith('http') ? sel : 'https://'
  const out = `[${label}](${url})`
  editContent.value = editContent.value.slice(0, s) + out + editContent.value.slice(e)
  // select the url so typing replaces it
  const urlPos = s + label.length + 3
  nextTick(() => { ta.focus(); ta.setSelectionRange(urlPos, urlPos + url.length) })
}

function mdImage() {
  const ta = editorRef.value
  if (!ta) return
  const s = ta.selectionStart
  const out = `![图片描述](https://)`
  editContent.value = editContent.value.slice(0, s) + out + editContent.value.slice(ta.selectionEnd)
  nextTick(() => { ta.focus(); ta.setSelectionRange(s + 2, s + 6) })
}

function mdCodeBlock() {
  const ta = editorRef.value
  if (!ta) return
  const { selectionStart: s, selectionEnd: e } = ta
  const sel = editContent.value.slice(s, e)
  const body = sel || '// code'
  const out = '\n```\n' + body + '\n```\n'
  editContent.value = editContent.value.slice(0, s) + out + editContent.value.slice(e)
  nextTick(() => { ta.focus(); ta.setSelectionRange(s + 4, s + 4) })
}

function mdTable() {
  const ta = editorRef.value
  if (!ta) return
  const s = ta.selectionStart
  const out = '\n| 列1 | 列2 | 列3 |\n| --- | --- | --- |\n| 内容 | 内容 | 内容 |\n'
  editContent.value = editContent.value.slice(0, s) + out + editContent.value.slice(ta.selectionEnd)
  nextTick(() => { ta.focus(); ta.setSelectionRange(s + 5, s + 7) })
}

function mdInsertText(text) {
  const ta = editorRef.value
  if (!ta) return
  const s = ta.selectionStart
  editContent.value = editContent.value.slice(0, s) + text + editContent.value.slice(ta.selectionEnd)
  nextTick(() => { ta.focus(); ta.setSelectionRange(s + text.length, s + text.length) })
}

// ---------------- view/edit switch & actions ----------------

function startEdit() {
  editContent.value = content.value
  mode.value = 'edit'
  msg.value = ''
  nextTick(() => editorRef.value?.focus())
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
.modal-mask {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.55);
  z-index: 200;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
}
.modal-shell {
  width: min(1180px, 96vw);
  height: min(860px, 92vh);
  background: var(--bg-elev);
  border: 1px solid var(--border);
  border-radius: 10px;
  display: flex;
  flex-direction: column;
  box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
  overflow: hidden;
}
.md-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 10px 16px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
  gap: 8px;
}
.md-title {
  display: flex;
  align-items: center;
  gap: 8px;
  overflow: hidden;
  min-width: 0;
}
.md-name {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.md-actions {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-shrink: 0;
}
.live-toggle {
  display: flex;
  align-items: center;
  gap: 4px;
  font-size: 12px;
  color: var(--muted);
  cursor: pointer;
  user-select: none;
  margin-right: 4px;
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

/* ---- Word-like toolbar ---- */
.md-toolbar {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 2px;
  padding: 6px 12px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-card);
  flex-shrink: 0;
}
.tb-group { display: flex; align-items: center; gap: 2px; }
.tb-sep {
  width: 1px;
  height: 18px;
  background: var(--border);
  margin: 0 6px;
}
.tb-btn {
  min-width: 26px;
  height: 26px;
  padding: 0 7px;
  background: transparent;
  border: 1px solid transparent;
  border-radius: 5px;
  color: var(--fg);
  font-size: 12px;
  cursor: pointer;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  line-height: 1;
}
.tb-btn:hover {
  background: var(--bg-elev);
  border-color: var(--border);
}
.tb-btn:active { transform: scale(0.94); }
.tb-btn.mono { font-family: 'JetBrains Mono', 'Consolas', monospace; font-size: 11px; }

/* ---- body ---- */
.md-body-row {
  flex: 1;
  display: flex;
  min-height: 0;
}
.md-body {
  flex: 1;
  overflow-y: auto;
  padding: 24px 36px;
  font-size: 13.5px;
  line-height: 1.75;
  color: var(--fg);
}
.md-loading {
  padding: 60px;
  text-align: center;
  color: var(--muted);
}
.md-editor {
  flex: 1;
  background: var(--bg);
  border: none;
  border-right: 1px solid var(--border);
  color: var(--fg);
  font-family: 'JetBrains Mono', 'Consolas', monospace;
  font-size: 13px;
  line-height: 1.7;
  padding: 16px 20px;
  resize: none;
  outline: none;
  min-width: 0;
}
.md-split-preview {
  flex: 1;
  overflow-y: auto;
  padding: 16px 24px;
  font-size: 13px;
  line-height: 1.7;
  min-width: 0;
}
.md-statusbar {
  display: flex;
  justify-content: space-between;
  padding: 4px 16px;
  border-top: 1px solid var(--border);
  font-size: 11px;
  color: var(--muted);
  flex-shrink: 0;
  font-family: 'JetBrains Mono', 'Consolas', monospace;
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
  padding: 10px 16px;
  max-height: 180px;
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

/* Modal transition */
.modal-enter-active, .modal-leave-active {
  transition: opacity .18s ease;
}
.modal-enter-active .modal-shell, .modal-leave-active .modal-shell {
  transition: transform .18s ease;
}
.modal-enter-from, .modal-leave-to { opacity: 0; }
.modal-enter-from .modal-shell, .modal-leave-to .modal-shell { transform: scale(0.97); }

@media (max-width: 900px) {
  .md-split-preview { display: none; }
}
</style>

<!-- Markdown rendered content styling (global, not scoped) -->
<style>
.markdown-body h1 { font-size: 22px; font-weight: 700; margin: 18px 0 10px; color: var(--fg); border-bottom: 1px solid var(--border); padding-bottom: 6px; }
.markdown-body h2 { font-size: 18px; font-weight: 600; margin: 16px 0 8px; color: var(--fg); }
.markdown-body h3 { font-size: 15px; font-weight: 600; margin: 12px 0 6px; color: var(--fg); }
.markdown-body h4, .markdown-body h5, .markdown-body h6 { font-size: 14px; font-weight: 600; margin: 10px 0 6px; color: var(--fg); }
.markdown-body p { margin: 8px 0; }
.markdown-body a { color: var(--accent); text-decoration: underline; }
.markdown-body ul, .markdown-body ol { margin: 8px 0; padding-left: 24px; }
.markdown-body li { margin: 4px 0; }
.markdown-body li input[type="checkbox"] { margin-right: 6px; }
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
