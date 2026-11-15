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
            <span v-if="msg" class="md-msg" :class="{ err: msgType === 'err' }">{{ msg }}</span>
            <button class="md-close" @click="$emit('close')">✕</button>
          </div>
        </div>

        <!-- Tab bar: 预览 / 编辑 / 相似文档 -->
        <div class="md-tabs">
          <button class="md-tab" :class="{ active: activeTab === 'preview' }" @click="switchTab('preview')">👁 预览</button>
          <button v-if="canEdit" class="md-tab" :class="{ active: activeTab === 'edit' }" @click="switchTab('edit')">✏️ 编辑</button>
          <button class="md-tab" :class="{ active: activeTab === 'similar' }" @click="switchTab('similar')">
            🔗 相似文档<template v-if="similarHits.length">（{{ similarHits.length }}）</template>
          </button>
          <span v-if="mode === 'edit'" class="md-tab-actions">
            <label class="live-toggle" title="编辑时实时预览">
              <input type="checkbox" v-model="livePreview"> 预览
            </label>
            <button class="btn ghost sm" @click="cancelEdit" :disabled="saving">取消</button>
            <button class="btn sm" @click="save" :disabled="saving">{{ saving ? '保存中…' : '💾 保存' }}</button>
          </span>
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
        <div v-else-if="activeTab === 'preview'" class="md-body-row">
          <!-- TOC sidebar (markdown with headings only) -->
          <div v-if="toc.length" v-show="tocOpen" class="md-toc">
            <div class="toc-head">
              <span>目录</span>
              <button class="toc-collapse" @click="tocOpen = false" title="收起目录">«</button>
            </div>
            <div class="toc-list">
              <div v-for="t in toc" :key="t.idx" class="toc-item" :class="'toc-h' + t.level"
                @click="tocScroll(t)">{{ t.title }}</div>
            </div>
          </div>
          <button v-else-if="!tocOpen" class="toc-expand" @click="tocOpen = true" title="展开目录">»</button>
          <div v-if="content" ref="bodyRef" class="md-body markdown-body" v-html="rendered"></div>
          <div v-else class="md-empty-state">
            <div class="md-empty-icon">{{ extIcon }}</div>
            <div class="md-empty-text">{{ emptyText }}</div>
            <button class="btn ghost sm" @click="openExternal">📂 打开原文件</button>
          </div>
        </div>
        <!-- edit mode -->
        <div v-else-if="activeTab === 'edit'" class="md-body-row">
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
            @paste="onPaste"
          ></textarea>
          <div v-if="livePreview" class="md-split-preview markdown-body" v-html="editRendered"></div>
        </div>
        <!-- similar docs tab -->
        <div v-else-if="activeTab === 'similar'" class="md-body-row">
          <div class="similar-list">
            <div v-if="findingSimilar" class="md-loading">查找相似文档中…</div>
            <template v-else>
              <div v-if="!similarHits.length" class="sim-empty">
                <span>暂无相似文档</span>
                <button class="btn ghost sm" @click="findSimilar">🔍 重新查找</button>
              </div>
              <div
                v-for="(s, i) in similarHits"
                :key="s.file_id"
                class="sim-hit"
                :title="simPath(s)"
                @click="previewFromSim(s)"
                @dblclick="openSimFile(s)"
                @contextmenu.prevent="onSimCtx($event, s)"
              >
                <span class="ext-badge" :class="'ext-' + s.ext">{{ s.ext }}</span>
                <div class="sim-info">
                  <div class="sim-line1">
                    <span class="sim-name">{{ simName(s) }}</span>
                    <span class="sim-dir">{{ simDir(s) }}</span>
                    <span class="sim-score">{{ Math.round(s.rank * 100) }}%</span>
                  </div>
                  <div class="sim-snippet" v-html="simSnippet(s)"></div>
                </div>
                <span class="sim-actions">
                  <button class="hit-action-btn" @click.stop="previewFromSim(s)" title="预览"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/></svg></button>
                  <button class="hit-action-btn" @click.stop="openSimFile(s)" title="打开文件"><svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z"/></svg></button>
                </span>
              </div>
            </template>
          </div>
        </div>

        <!-- status bar -->
        <div v-if="activeTab === 'edit'" class="md-statusbar">
          <span>{{ editContent.length }} 字符 · {{ lineCount }} 行</span>
          <span class="status-enc" :title="'文件编码：' + fileEncoding">enc: {{ fileEncoding }}</span>
          <span class="status-hint">Ctrl+S 保存 · Esc 取消</span>
        </div>
      </div>
    </div>

    <!-- similar-file context menu -->
    <div v-if="simMenu" class="sim-ctx-backdrop" @click="simMenu = null" @contextmenu.prevent="simMenu = null"></div>
    <div v-if="simMenu" class="sim-ctx-menu" :style="{ left: simMenu.x + 'px', top: simMenu.y + 'px' }">
      <div class="ctx-item" @click="simMenuPreview">👁 预览</div>
      <div class="ctx-item" @click="simMenuOpen">📂 打开文件</div>
      <div class="ctx-item" @click="simMenuOpenFolder">📁 打开所在文件夹</div>
      <div class="ctx-sep"></div>
      <div class="ctx-item" @click="simMenuCopyPath">📋 复制文件路径</div>
      <div class="ctx-item" @click="simMenuCopyName">📋 复制文件名</div>
      <div class="ctx-item" @click="simMenuCopyFolder">📋 复制文件夹路径</div>
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
  // Optional search query: after loading, scroll to & flash the first match.
  query: { type: String, default: '' },
  // Auto-run find-similar when opened (for non-previewable files).
  autoSimilar: { type: Boolean, default: false },
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

const EXT_ICONS = {
  pdf: '📄', doc: '📄', docx: '📄', xls: '📊', xlsx: '📊', ppt: '📽', pptx: '📽',
  png: '🖼', jpg: '🖼', jpeg: '🖼', gif: '🖼', bmp: '🖼', zip: '🗜', rar: '🗜',
}
const extIcon = computed(() => EXT_ICONS[ext.value] || '📦')
const emptyText = computed(() => {
  const e = ext.value || '文件'
  return `此类型（${e.toUpperCase()}）暂不支持预览，可打开原文件或查看下方相似文档`
})

function openExternal() {
  if (props.path) api.openFile(props.path).catch(() => {})
}

// ---------------- tabs: preview / edit / similar ----------------
const activeTab = ref('preview')
const fileEncoding = ref('utf-8')
// one-shot flag: set when the modal switches to another similar file via the
// preview button/menu, so watch(path) keeps the preview tab instead of jumping
// back to the similar tab (autoSimilar)
const simPreviewJump = ref(false)

function switchTab(t) {
  if (t === 'edit') startEdit()
  else if (t === 'preview') cancelEdit()
  activeTab.value = t
  if (t === 'similar' && !similarHits.value.length && !findingSimilar.value) {
    findSimilar()
  }
}

// ---------------- similar-doc helpers ----------------
function simPath(s) {
  return (s.folder_path || '').replace(/[\\/]+$/, '') + '\\' + (s.rel_path || '').replace(/\//g, '\\')
}
function simName(s) {
  return (s.rel_path || '').split('/').pop() || s.rel_path
}
function simDir(s) {
  const parts = (s.rel_path || '').split('/')
  parts.pop()
  return parts.join('/')
}
function simSnippet(s) {
  const raw = String(s.snippet || '').replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  return raw.length > 160 ? raw.slice(0, 160) + '…' : raw
}
function openSimFile(s) {
  api.openFile(simPath(s)).catch(() => {})
}

// ---- similar-file context menu ----
const simMenu = ref(null)
function onSimCtx(e, s) {
  simMenu.value = { x: e.clientX, y: e.clientY, s }
}
async function simCopy(text) {
  try { await navigator.clipboard.writeText(text) }
  catch (_) {
    const ta = document.createElement('textarea')
    ta.value = text; document.body.appendChild(ta); ta.select()
    document.execCommand('copy'); document.body.removeChild(ta)
  }
}
function simMenuPreview() { const s = simMenu.value?.s; simMenu.value = null; if (s) previewFromSim(s) }
function previewFromSim(s) {
  simPreviewJump.value = true
  emit('preview', s)
}
function simMenuOpen() { const s = simMenu.value?.s; simMenu.value = null; if (s) openSimFile(s) }
function simMenuOpenFolder() {
  const p = simPath(simMenu.value?.s || {})
  simMenu.value = null
  api.openFolder(p).catch(() => {})
}
function simMenuCopyPath() { const s = simMenu.value?.s; simMenu.value = null; if (s) simCopy(simPath(s)) }
function simMenuCopyName() { const s = simMenu.value?.s; simMenu.value = null; if (s) simCopy(simName(s)) }
function simMenuCopyFolder() {
  const p = simPath(simMenu.value?.s || {})
  const idx = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'))
  simMenu.value = null
  simCopy(idx >= 0 ? p.substring(0, idx) : '')
}

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
  simMenu.value = null
  // one-shot: when the user clicks a similar-file preview button, stay on the
  // preview tab instead of auto-switching to the similar tab again
  const fromSimClick = simPreviewJump.value
  simPreviewJump.value = false
  activeTab.value = fromSimClick ? 'preview' : 'preview'
  loading.value = true
  try {
    const r = await api.readFile(p)
    if (r.ok) {
      content.value = r.content || ''
      ext.value = r.ext || ''
      fileEncoding.value = r.encoding || 'utf-8'
      toc.value = isMarkdown.value ? extractToc(content.value) : []
      tagHeadings()
      scrollToHit()
    } else {
      content.value = ''
      ext.value = r.ext || ''
      fileEncoding.value = 'utf-8'
      msg.value = r.error || '读取失败'
      msgType.value = 'err'
    }
  } catch (e) {
    msg.value = e.message
    msgType.value = 'err'
  } finally {
    loading.value = false
    if (p && props.autoSimilar && props.file_id && !fromSimClick) {
      activeTab.value = 'similar'
      findSimilar()
    }
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

// ---------------- paste image ----------------

/// Paste an image into the editor: upload via cmd_save_image, insert a
/// relative `![](assets/…)` link at the caret. Text pastes pass through.
async function onPaste(e) {
  const items = e.clipboardData?.items
  if (!items) return
  for (const it of items) {
    if (it.kind === 'file' && it.type.startsWith('image/')) {
      e.preventDefault()
      const file = it.getAsFile()
      if (!file) return
      const reader = new FileReader()
      reader.onload = async () => {
        try {
          const r = await api.saveImage(props.path, reader.result, it.type)
          if (r.ok && r.rel_path) mdInsertText(`![图片](${r.rel_path})`)
          else { msg.value = r.error || '图片保存失败'; msgType.value = 'err' }
        } catch (err) {
          msg.value = '图片保存失败: ' + err.message
          msgType.value = 'err'
        }
      }
      reader.readAsDataURL(file)
      return
    }
  }
}

// ---------------- TOC (table of contents) ----------------

const toc = ref([])
const tocOpen = ref(true)
const bodyRef = ref(null)

/// Extract H1-H3 headings from the raw markdown (view mode outline).
function extractToc(text) {
  const out = []
  const lines = (text || '').split('\n')
  let inCode = false
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i]
    if (/^\s*(```|~~~)/.test(l)) inCode = !inCode
    if (inCode) continue
    const m = /^(#{1,3})\s+(.*)$/.exec(l)
    if (m) {
      const title = m[2].replace(/[#\s]+$/, '').trim()
      if (title) out.push({ level: m[1].length, title, idx: out.length })
    }
  }
  return out
}

/// After render, tag each heading in the rendered DOM with an id so TOC
/// clicks can scroll to it. Only run for markdown; non-md has no headings.
function tagHeadings() {
  nextTick(() => {
    const root = bodyRef.value
    if (!root) return
    const hs = root.querySelectorAll('h1, h2, h3')
    hs.forEach((h, i) => { h.id = 'md-toc-' + i })
  })
}

function tocScroll(item) {
  const root = bodyRef.value
  if (!root) return
  const hs = root.querySelectorAll('h1, h2, h3')
  const el = hs[item.idx]
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' })
    el.classList.add('toc-flash')
    setTimeout(() => el.classList.remove('toc-flash'), 1200)
  }
}

// ---------------- search hit locate ----------------

/// Scroll to & flash the first occurrence of props.query in the rendered
/// document (view mode). Falls back silently when there is no query.
function scrollToHit() {
  if (!props.query) return
  nextTick(() => {
    const root = bodyRef.value
    if (!root) return
    const query = props.query.toLowerCase()
    // Headings first, then paragraphs/list items.
    const nodes = [...root.querySelectorAll('h1,h2,h3,h4,p,li,td,pre')]
    for (const el of nodes) {
      if ((el.textContent || '').toLowerCase().includes(query)) {
        el.scrollIntoView({ behavior: 'smooth', block: 'center' })
        el.classList.add('toc-flash')
        setTimeout(() => el.classList.remove('toc-flash'), 1600)
        return
      }
    }
  })
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
    await api.saveFile(props.path, editContent.value, fileEncoding.value)
    content.value = editContent.value
    toc.value = isMarkdown.value ? extractToc(content.value) : []
    tagHeadings()
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
/* TOC sidebar */
.md-toc {
  width: 220px;
  flex-shrink: 0;
  border-right: 1px solid var(--border);
  overflow-y: auto;
  padding: 10px 8px;
}
.toc-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 11px;
  font-weight: 700;
  color: var(--muted);
  text-transform: uppercase;
  letter-spacing: 1px;
  padding: 2px 6px 8px;
}
.toc-collapse {
  background: none;
  border: none;
  color: var(--muted);
  cursor: pointer;
  font-size: 13px;
  padding: 0 4px;
  border-radius: 4px;
}
.toc-collapse:hover { color: var(--fg); background: var(--bg-card); }
.toc-list { display: flex; flex-direction: column; gap: 1px; }
.toc-item {
  font-size: 12px;
  color: var(--muted);
  padding: 3px 8px;
  border-radius: 4px;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.toc-item:hover { color: var(--fg); background: var(--bg-card); }
.toc-h1 { font-weight: 700; color: var(--fg); }
.toc-h2 { padding-left: 18px; }
.toc-h3 { padding-left: 30px; font-size: 11px; }
.toc-expand {
  width: 20px;
  flex-shrink: 0;
  border: none;
  border-right: 1px solid var(--border);
  background: var(--bg-card);
  color: var(--muted);
  cursor: pointer;
  font-size: 13px;
}
.toc-expand:hover { color: var(--fg); background: var(--bg-elev); }
/* flash highlight when scrolling to TOC/search hit */
.toc-flash {
  animation: toc-flash-anim 1.4s ease-out;
  border-radius: 4px;
}
@keyframes toc-flash-anim {
  0% { background: rgba(96,165,250,.35); box-shadow: 0 0 0 4px rgba(96,165,250,.18); }
  100% { background: transparent; box-shadow: 0 0 0 0 transparent; }
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
.status-enc { color: var(--accent); }

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

/* Tab bar */
.md-tabs {
  display: flex;
  align-items: center;
  gap: 2px;
  padding: 0 12px;
  border-bottom: 1px solid var(--border);
  flex-shrink: 0;
}
.md-tab {
  background: none;
  border: none;
  border-bottom: 2px solid transparent;
  color: var(--muted);
  font-size: 12.5px;
  font-family: inherit;
  padding: 8px 12px;
  cursor: pointer;
}
.md-tab:hover { color: var(--fg); }
.md-tab.active { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }
.md-tab-actions {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 4px 0;
}

/* Empty state (non-previewable file types) */
.md-empty-state {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 32px;
}
.md-empty-icon { font-size: 44px; opacity: .5; }
.md-empty-text { font-size: 13px; color: var(--muted); }

/* Similar docs tab */
.similar-list {
  flex: 1;
  overflow-y: auto;
  padding: 8px 12px;
}
.sim-empty {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 48px 16px;
  color: var(--muted);
  font-size: 13px;
}
.sim-hit {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  padding: 8px 10px;
  border-radius: 6px;
  cursor: pointer;
}
.sim-hit:hover { background: var(--bg-card); }
.sim-info { flex: 1; min-width: 0; }
.sim-line1 { display: flex; align-items: center; gap: 8px; }
.sim-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--fg);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sim-dir {
  font-size: 11px;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}
.sim-score {
  font-size: 11px;
  color: var(--accent);
  font-family: 'JetBrains Mono', 'Consolas', monospace;
  flex-shrink: 0;
}
.sim-snippet {
  font-size: 11.5px;
  color: var(--muted);
  margin-top: 3px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.sim-actions {
  display: none;
  gap: 2px;
  flex-shrink: 0;
  align-self: center;
}
.sim-hit:hover .sim-actions { display: inline-flex; }
.hit-action-btn {
  background: none; border: none; cursor: pointer;
  color: var(--muted);
  display: inline-flex; align-items: center; justify-content: center;
  font-size: 13px; padding: 3px 5px; border-radius: 4px;
}
.hit-action-btn:hover { color: var(--accent); background: var(--bg-card); }
.hit-action-btn svg { display: block; }

/* Similar-file context menu */
.sim-ctx-backdrop { position: fixed; inset: 0; z-index: 90; }
.sim-ctx-menu {
  position: fixed; z-index: 100; min-width: 170px;
  background: #1e2024; border: 1px solid #2a2d33; border-radius: 6px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.5); padding: 4px 0;
}
.ctx-item { padding: 7px 14px; font-size: 13px; cursor: pointer; color: #e6e8eb; }
.ctx-item:hover { background: #2a2d33; }
.ctx-sep { height: 1px; background: #2a2d33; margin: 4px 0; }

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
