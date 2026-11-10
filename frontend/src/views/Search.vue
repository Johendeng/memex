<template>
  <div class="col">
    <HelpTour :steps="tourSteps" />
    <div class="card search-bar">
      <div class="row">
        <div class="search-input-wrap">
          <input v-model="q" class="input" placeholder="输入关键词或只言片语,回车搜索…" @keyup.enter="run" @focus="showHistory = true" @blur="hideHistoryLater" />
          <div v-if="showHistory && history.length" class="history-dropdown" @mousedown.prevent="">
            <div class="history-header">搜索历史 <span class="history-clear" @click="clearHistory">清空</span></div>
            <div v-for="(item, i) in history" :key="i" class="history-item" @click="useHistory(item)">
              <span class="history-icon">🕐</span>
              <span class="history-text">{{ item }}</span>
            </div>
          </div>
        </div>
        <button class="btn" @click="run" :disabled="loading">{{ loading ? '搜索中…' : '搜索' }}</button>
      </div>
      <div class="search-opts">
        <label class="toggle" :class="{ on: hybrid }" @click="toggleHybrid" :title="hybrid ? '混合检索：BM25 关键词 + bge-small-zh 语义向量 + RRF 融合' : '点击启用混合检索（语义+关键词）'">
          <span class="toggle-dot"></span>
          <span class="toggle-label">语义检索</span>
        </label>
        <span class="vector-status" v-if="hybrid && vecReady" title="向量索引就绪">✓ {{ vecCount }} 条向量</span>
        <button class="embed-btn" v-if="hybrid && !vecReady && !embedding" @click="doEmbed">生成向量索引</button>
        <span class="embedding-hint" v-if="embedding">正在生成向量…</span>
        <span class="opt-sep"></span>
        <select v-model="extFilter" class="tag-select" @change="run" title="按文件类型过滤">
          <option value="">全部类型</option>
          <option value="md">Markdown</option>
          <option value="pdf">PDF</option>
          <option value="docx">Word</option>
          <option value="xlsx">Excel</option>
          <option value="pptx">PPT</option>
        <option value="png">IMG</option>
          <option value="txt">TXT</option>
        </select>
        <select v-model="filterDim" class="tag-select" @change="onFilterDim">
          <option value="">全部维度</option>
          <option v-for="d in filterDims" :key="d" :value="d">{{ d }}</option>
        </select>
        <select v-model="filterTag" class="tag-select" @change="run" :disabled="!filterDim">
          <option value="">全部标签</option>
          <option v-for="t in filterTags" :key="t.value" :value="t.value">{{ t.value }} ({{ t.count }})</option>
        </select>
        <button v-if="filterDim || extFilter" class="btn ghost sm" @click="clearAllFilters">清除过滤</button>
      </div>
      <div class="search-info" v-if="hits.length">{{ hits.length }} 条结果 · {{ hybrid ? '混合检索' : '关键词检索' }}<span v-if="extFilter"> · 类型:{{ extFilter }}</span><span v-if="filterDim && filterTag"> · {{ filterDim }}/{{ filterTag }}</span></div>
    </div>

    <!-- Find Similar results -->
    <div v-if="similarMode && similarResults.length" class="card similar-card">
      <div class="similar-header">
        <span class="similar-label">相似文档</span>
        <span class="similar-origin" :title="similarOrigin">{{ similarOriginName }}</span>
        <button class="btn ghost sm" @click="exitSimilar">返回搜索</button>
      </div>
      <div v-for="(h, i) in similarResults" :key="'sim' + i" class="hit" :title="fullPath(h)" @dblclick="openHit(h)">
        <div class="hit-head">
          <span class="ext-badge" :class="'ext-' + h.ext">{{ h.ext }}</span>
          <span class="hit-name" :title="fullPath(h)">{{ fullPath(h) }}</span>
          <span class="hit-score" :title="'相似度 ' + h.rank.toFixed(3)">{{ Math.round(h.rank * 100) }}%</span>
        </div>
        <div class="hit-snippet" v-html="escapeAndTruncate(h.snippet)"></div>
      </div>
    </div>

    <!-- Normal search results -->
    <div v-if="!similarMode && hits.length" class="card">
      <div v-for="(h, i) in hits" :key="i" class="hit" :title="'双击打开文件'" @contextmenu.prevent="onCtx($event, h)" @dblclick="openHit(h)">
        <div class="hit-head">
          <span class="ext-badge" :class="'ext-' + h.ext">{{ h.ext }}</span>
          <span class="hit-name" :title="fullPath(h)">{{ fullPath(h) }}</span>
          <div class="hit-actions">
            <button v-if="isPreviewable(h)" class="hit-action-btn" @click.stop="previewHit(h)" title="预览">👁</button>
            <button v-if="vecReady" class="hit-action-btn" @click.stop="findSimilar(h)" title="查找相似文档">🔗</button>
            <span class="hit-score" :title="'原始评分 ' + h.rank.toFixed(3)">{{ normScore(h) }}%</span>
          </div>
          <span class="hit-meta">chunk {{ h.chunk_seq }}</span>
        </div>
        <div class="hit-snippet" v-html="h.snippet"></div>
      </div>
    </div>
    <div v-else-if="!similarMode && msg" class="empty">{{ msg }}</div>

    <div v-if="menu" class="ctx-backdrop" @click="menu = null" @contextmenu.prevent="menu = null"></div>
    <div v-if="menu" class="ctx-menu" :style="{ left: menu.x + 'px', top: menu.y + 'px' }">
      <div class="ctx-item" v-if="isPreviewable(menu.hit)" @click="previewHit(menu.hit)">预览</div>
      <div class="ctx-item" v-if="vecReady" @click="findSimilar(menu.hit)">查找相似文档</div>
      <div class="ctx-sep"></div>
      <div class="ctx-item" @click="openFile">打开文件</div>
      <div class="ctx-item" @click="openFolder">打开文件所在文件夹</div>
      <div class="ctx-sep"></div>
      <div class="ctx-item" @click="copyPath">复制文件地址</div>
      <div class="ctx-item" @click="copyFolderPath">复制文件所在文件夹</div>
    </div>

    <MarkdownPreview
      :path="previewPath"
      :file_id="previewFileId"
      @close="closePreview"
      @preview="onPreviewFromSimilar"
    />
  </div>
</template>

<script setup>
import { ref, computed, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'
import HelpTour from '../components/HelpTour.vue'
import MarkdownPreview from '../components/MarkdownPreview.vue'

const tourSteps = [
  { selector: '.search-bar .input', title: '搜索框', text: '输入关键词或文件中的只言片语，回车或点击「搜索」即可检索知识库。支持 BM25 关键词匹配，高亮显示命中的片段。' },
  { selector: '.search-bar .toggle', title: '语义检索', text: '开启后使用 bge-small-zh 语义向量 + BM25 关键词混合检索（RRF 融合）。适合查找"意思相近但用词不同"的内容。首次需生成向量索引。' },
  { selector: '.search-bar .tag-select', title: '过滤', text: '按文件类型、分类维度和标签缩小搜索范围。文件类型支持 Markdown/PDF/Word/Excel/TXT；标签按维度+值过滤。' },
  { selector: '.hit', title: '搜索结果', text: '每条结果显示文件路径、匹配分数、chunk 序号和高亮片段。双击直接打开文件，点击👁预览MD，点击🔗查找相似文档，右键有更多操作。' },
]

const q = ref('')
const hits = ref([])
const msg = ref('')
const loading = ref(false)
const menu = ref(null)
const hybrid = ref(false)
const vecReady = ref(false)
const vecCount = ref(0)
const embedding = ref(false)
const filterDims = ref([])
const filterTags = ref([])
const filterDim = ref('')
const filterTag = ref('')
const extFilter = ref('')
const showHistory = ref(false)
const history = ref([])
let hideTimer = null
const similarMode = ref(false)
const similarResults = ref([])
const similarOrigin = ref('')
const previewPath = ref('')
const previewFileId = ref(0)

const maxScore = computed(() => Math.max(...hits.value.map(h => h.rank), 1))
function normScore(h) { return Math.round(h.rank / maxScore.value * 100) }

const PREVIEWABLE_EXTS = ['md', 'markdown', 'txt', 'csv', 'json', 'html', 'htm', 'xml', 'log', 'yml', 'yaml', 'toml', 'ini', 'conf']
function isPreviewable(h) {
  const ext = (h.ext || '').replace(/^\./, '').toLowerCase()
  return PREVIEWABLE_EXTS.includes(ext)
}

const similarOriginName = computed(() => {
  if (!similarOrigin.value) return ''
  const parts = similarOrigin.value.replace(/\\/g, '/').split('/')
  return parts[parts.length - 1] || similarOrigin.value
})

function fullPath(h) {
  const fp = (h.folder_path || '').replace(/[\\\/]+$/, '')
  const rp = (h.rel_path || '').replace(/\//g, '\\')
  return fp + '\\' + rp
}

function escapeAndTruncate(s) {
  if (!s) return ''
  const escaped = s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
  return escaped.length > 300 ? escaped.slice(0, 300) + '…' : escaped
}

function onCtx(e, h) {
  menu.value = { x: e.clientX, y: e.clientY, path: fullPath(h), hit: h }
}

async function copyPath() {
  const p = menu.value?.path || ''
  menu.value = null
  try { await navigator.clipboard.writeText(p) }
  catch (_) {
    const ta = document.createElement('textarea')
    ta.value = p; document.body.appendChild(ta); ta.select()
    document.execCommand('copy'); document.body.removeChild(ta)
  }
}

async function openFile() {
  const p = menu.value?.path || ''
  menu.value = null
  try { await api.openFile(p) }
  catch (e) { msg.value = e.message }
}

async function openFolder() {
  const p = menu.value?.path || ''
  menu.value = null
  try { await api.openFolder(p) }
  catch (e) { msg.value = e.message }
}

function getFolderPath(p) {
  const normalized = p.replace(/\//g, '\\')
  const idx = normalized.lastIndexOf('\\')
  return idx >= 0 ? normalized.slice(0, idx) : ''
}

async function copyFolderPath() {
  const p = getFolderPath(menu.value?.path || '')
  menu.value = null
  try { await navigator.clipboard.writeText(p) }
  catch (_) {
    const ta = document.createElement('textarea')
    ta.value = p; document.body.appendChild(ta); ta.select()
    document.execCommand('copy'); document.body.removeChild(ta)
  }
}

async function openHit(h) {
  try { await api.openFile(fullPath(h)) }
  catch (e) { msg.value = e.message }
}

function loadHistory() {
  try {
    const raw = localStorage.getItem('memex_search_history')
    history.value = raw ? JSON.parse(raw) : []
  } catch (_) { history.value = [] }
}

function saveHistory(query) {
  const trimmed = query.trim()
  if (!trimmed) return
  let h = history.value.filter(x => x !== trimmed)
  h.unshift(trimmed)
  h = h.slice(0, 10)
  history.value = h
  try { localStorage.setItem('memex_search_history', JSON.stringify(h)) } catch (_) {}
}

function useHistory(item) {
  q.value = item
  showHistory.value = false
  run()
}

function clearHistory() {
  history.value = []
  try { localStorage.removeItem('memex_search_history') } catch (_) {}
  showHistory.value = false
}

function hideHistoryLater() {
  hideTimer = setTimeout(() => { showHistory.value = false }, 200)
}

function previewHit(h) {
  previewPath.value = fullPath(h)
  previewFileId.value = h.file_id
}

function closePreview() {
  previewPath.value = ''
  previewFileId.value = 0
}

function onPreviewFromSimilar(s) {
  previewPath.value = (s.folder_path || '').replace(/[\\\/]+$/, '') + '\\' + (s.rel_path || '').replace(/\//g, '\\')
  previewFileId.value = s.file_id
}

async function findSimilar(h) {
  menu.value = null
  similarMode.value = true
  similarResults.value = []
  similarOrigin.value = fullPath(h)
  msg.value = ''
  try {
    const r = await api.findSimilar(h.file_id, 10)
    if (r.error) {
      msg.value = r.error
      similarResults.value = []
    } else {
      similarResults.value = r.hits || []
      if (!similarResults.value.length) msg.value = '未找到相似文档'
    }
  } catch (e) { msg.value = e.message }
}

function exitSimilar() {
  similarMode.value = false
  similarResults.value = []
  similarOrigin.value = ''
  msg.value = ''
}

async function run() {
  if (!q.value.trim()) return
  showHistory.value = false
  loading.value = true
  hits.value = []
  msg.value = ''
  similarMode.value = false
  similarResults.value = []
  try {
    const tagFilter = {}
    if (filterDim.value && filterTag.value) {
      tagFilter.dim = filterDim.value
      tagFilter.tag = filterTag.value
    }
    const filters = {}
    if (extFilter.value) filters.ext_filter = extFilter.value
    hits.value = await api.search(q.value.trim(), 20, hybrid.value, tagFilter, filters)
    if (!hits.value.length) {
      if (hybrid.value && !vecReady.value) msg.value = '无结果。请先生成向量索引（点击上方「生成向量索引」）'
      else if (filterDim.value && filterTag.value) msg.value = '无结果（当前过滤范围：' + filterDim.value + '/' + filterTag.value + '）'
      else if (extFilter.value) msg.value = '无结果（当前类型过滤：' + extFilter.value + '）'
      else msg.value = '无结果'
    }
    saveHistory(q.value.trim())
  } catch (e) { msg.value = e.message }
  finally { loading.value = false }
}

async function loadFilterDims() {
  try {
    const d = await api.tags.dims()
    filterDims.value = d.dims || []
  } catch (_) {}
}

async function onFilterDim() {
  filterTag.value = ''
  filterTags.value = []
  if (!filterDim.value) return
  try {
    const r = await api.tags.summary(filterDim.value)
    filterTags.value = r.tags || []
  } catch (_) {}
}

function clearAllFilters() {
  filterDim.value = ''
  filterTag.value = ''
  filterTags.value = []
  extFilter.value = ''
}

function onKey(e) {
  if (e.key === 'Escape') {
    menu.value = null
    showHistory.value = false
    if (previewPath.value) closePreview()
  }
}

onMounted(() => {
  window.addEventListener('keydown', onKey)
  checkVectors()
  loadFilterDims()
  loadHistory()
})

onUnmounted(() => {
  window.removeEventListener('keydown', onKey)
  if (hideTimer) clearTimeout(hideTimer)
})

async function checkVectors() {
  try {
    const s = await api.vectorStatus()
    vecReady.value = s.has_vectors
    vecCount.value = s.count || 0
  } catch (_) { vecReady.value = false }
}

function toggleHybrid() { hybrid.value = !hybrid.value }

async function doEmbed() {
  embedding.value = true
  msg.value = ''
  try {
    await api.embed()
    await checkVectors()
    msg.value = `向量索引已生成（${vecCount.value} 条）`
  } catch (e) {
    msg.value = '向量生成失败：' + e.message
  } finally { embedding.value = false }
}
</script>

<style scoped>
.ctx-backdrop { position: fixed; inset: 0; z-index: 90; }
.ctx-menu {
  position: fixed; z-index: 100; min-width: 160px;
  background: #1e2024; border: 1px solid #2a2d33; border-radius: 6px;
  box-shadow: 0 8px 24px rgba(0,0,0,0.5); padding: 4px 0;
}
.ctx-item { padding: 7px 14px; font-size: 13px; cursor: pointer; color: #e6e8eb; }
.ctx-item:hover { background: rgba(245,158,11,0.12); }
.ctx-sep { height: 1px; background: #2a2d33; margin: 4px 0; }

.search-bar .input { width: 100%; }
.search-bar .btn { flex-shrink: 0; white-space: nowrap; }
.search-info { font-size: 12px; color: var(--muted); margin-top: 8px; }

.search-input-wrap {
  flex: 1;
  position: relative;
}

.history-dropdown {
  position: absolute;
  top: 100%;
  left: 0; right: 0;
  background: #1e2024;
  border: 1px solid #2a2d33;
  border-radius: 0 0 6px 6px;
  border-top: none;
  box-shadow: 0 8px 16px rgba(0,0,0,0.4);
  z-index: 50;
  max-height: 240px;
  overflow-y: auto;
}
.history-header {
  font-size: 11px;
  color: var(--muted);
  padding: 6px 12px;
  border-bottom: 1px solid var(--border);
  display: flex;
  justify-content: space-between;
  align-items: center;
}
.history-clear {
  cursor: pointer;
  color: var(--accent);
}
.history-clear:hover { text-decoration: underline; }
.history-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 12px;
  cursor: pointer;
  font-size: 12px;
}
.history-item:hover { background: var(--bg-card); }
.history-icon { opacity: .5; }
.history-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.search-opts {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 8px;
  flex-wrap: wrap;
}
.toggle {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  cursor: pointer;
  user-select: none;
  font-size: 12px;
  color: var(--muted);
}
.toggle-dot {
  width: 28px;
  height: 16px;
  border-radius: 8px;
  background: #2a2d33;
  position: relative;
  transition: background .15s;
}
.toggle-dot::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 12px;
  height: 12px;
  border-radius: 50%;
  background: #6b7280;
  transition: transform .15s, background .15s;
}
.toggle.on { color: var(--accent); }
.toggle.on .toggle-dot { background: rgba(245,158,11,.25); }
.toggle.on .toggle-dot::after { transform: translateX(12px); background: var(--accent); }
.toggle-label { font-weight: 500; }
.vector-status {
  font-size: 11px;
  color: #4ade80;
  font-family: 'JetBrains Mono', 'Consolas', monospace;
}
.embed-btn {
  font-size: 11px;
  padding: 2px 8px;
  border: 1px solid var(--accent);
  color: var(--accent);
  background: transparent;
  border-radius: 4px;
  cursor: pointer;
}
.embed-btn:hover { background: rgba(245,158,11,.12); }
.embedding-hint {
  font-size: 11px;
  color: var(--muted);
}
.opt-sep {
  width: 1px;
  height: 20px;
  background: var(--border);
  margin: 0 4px;
}
.tag-select {
  font-size: 12px;
  padding: 4px 8px;
  border: 1px solid var(--border);
  border-radius: 4px;
  background: var(--bg);
  color: var(--fg);
  cursor: pointer;
  max-width: 140px;
}
.tag-select:disabled { opacity: .5; cursor: not-allowed; }
.tag-select:focus { border-color: var(--accent); outline: none; }

.hit {
  padding: 12px 14px;
  border-radius: 6px;
  transition: background .12s;
  cursor: pointer;
}
.hit + .hit { border-top: 1px solid var(--border); }
.hit:hover { background: var(--bg-elev); }
.hit-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 5px;
}
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
}
.ext-md, .ext-markdown { background: rgba(96,165,250,.14); color: #60a5fa; }
.ext-docx { background: rgba(96,165,250,.14); color: #60a5fa; }
.ext-xlsx { background: rgba(74,222,128,.14); color: #4ade80; }
.ext-pptx { background: rgba(251,146,60,.14); color: #fb923c; }
.ext-png, .ext-jpg, .ext-jpeg, .ext-bmp, .ext-tiff, .ext-tif, .ext-webp { background: rgba(34,197,94,.14); color: #22c55e; }
.ext-pdf { background: rgba(248,113,113,.14); color: #f87171; }
.ext-txt, .ext-csv, .ext-json { background: rgba(139,145,153,.14); color: var(--muted); }
.hit-name {
  font-size: 14px;
  font-weight: 500;
  color: var(--fg);
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.hit-actions {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-shrink: 0;
}
.hit-action-btn {
  background: none;
  border: none;
  color: var(--muted);
  cursor: pointer;
  padding: 2px 6px;
  border-radius: 4px;
  font-size: 14px;
}
.hit-action-btn:hover {
  color: var(--accent);
  background: rgba(245,158,11,.12);
}
.hit-meta {
  font-size: 11px;
  color: var(--muted);
  font-family: 'JetBrains Mono', 'Consolas', monospace;
  flex-shrink: 0;
}
.hit-score {
  font-size: 11px;
  font-weight: 600;
  font-family: 'JetBrains Mono', 'Consolas', monospace;
  color: var(--accent);
  background: rgba(245,158,11,.12);
  padding: 1px 6px;
  border-radius: 3px;
  flex-shrink: 0;
  min-width: 32px;
  text-align: center;
}
.hit-snippet {
  color: var(--muted);
  font-size: 12px;
  line-height: 1.6;
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
  padding-left: 42px;
}
.hit-snippet b {
  background: rgba(245,158,11,.25);
  color: var(--accent);
  padding: 0 2px;
  border-radius: 2px;
  font-weight: 600;
}

.similar-card {
  border: 1px solid var(--accent-dim);
}
.similar-header {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 10px;
  padding-bottom: 8px;
  border-bottom: 1px solid var(--border);
}
.similar-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--accent);
}
.similar-origin {
  font-size: 12px;
  color: var(--muted);
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
