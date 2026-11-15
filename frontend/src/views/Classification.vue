<template>
  <div class="col">
    <HelpTour :steps="tourSteps" :before-start="() => { expanded = true }" />
    <!-- Prompt Generator -->
    <div class="card" :class="{collapsed: !expanded}">
      <!-- Header (click to toggle) -->
      <div class="row between gen-header" id="cls-gen-header" @click="expanded = !expanded" :title="expanded ? '点击收起' : '点击展开'">
        <div class="row" style="gap:8px">
          <span class="card-title" style="margin:0">分类 Prompt 生成器</span>
          <span class="muted" style="font-size:12px">生成访谈式分类提示词，Agent 会先抽样侦察、提问确认、试点打标，经你确认后才全量执行</span>
        </div>
        <span class="chev" :class="{rot: expanded}">▸</span>
      </div>

      <!-- Body (only when expanded) -->
      <template v-if="expanded">
        <!-- Dimensions editor (optional, user-specified) -->
        <div class="dim-block auto-hint" id="cls-dim-hint">
          <span>下方可指定维度和标签（留空则由 Agent 访谈中提出候选方案）。已指定的维度作为先验，Agent 围绕它细化而不会从零提问。</span>
        </div>
        <div v-for="(dim, i) in genDims" :key="i" class="dim-block">
            <div class="row" style="gap:8px">
              <input v-model="dim.name" class="input dim-name" placeholder="维度名称" />
              <button class="btn ghost sm" @click="removeDim(i)">移除</button>
            </div>
            <div class="row tag-row">
              <span v-for="(tag, j) in dim.tags" :key="j" class="tag on dim-tag" @click="removeTag(i, j)" :title="'点击移除'">
                {{ tag }} <span style="opacity:.6;margin-left:2px">×</span>
              </span>
              <input
                v-model="dim.newTag"
                class="input tag-input"
                placeholder="添加标签后回车"
                @keyup.enter="addTag(i)"
              />
            </div>
          </div>
          <button class="btn ghost sm" style="width:100%;text-align:center;margin-top:4px" @click="addDim">+ 添加维度</button>

        <!-- Generate & Copy -->
        <div class="row" id="cls-gen-actions" style="margin-top:14px">
          <button class="btn" @click="generatePrompt">生成提示词</button>
          <button class="btn ghost" @click="copyPrompt" :disabled="!prompt">📋 复制</button>
          <span v-if="copied" class="muted" style="font-size:12px">已复制 ✓</span>
        </div>
        <textarea
          v-if="prompt"
          v-model="prompt"
          readonly
          rows="14"
          class="input mono prompt-text"
          style="margin-top:12px;resize:vertical;font-size:12px;line-height:1.6"
        />
      </template>
    </div>

    <!-- Master-detail tag browser -->
    <div class="card tag-browser" id="cls-tag-browser">
      <!-- Dimension tabs -->
      <div class="dim-tabs">
        <button
          v-for="d in dims"
          :key="d"
          class="dim-tab"
          :class="{active: selectedDim === d}"
          @click="selectDim(d)"
        >{{ d }}</button>
      </div>

      <div class="tag-body" v-if="selectedDim">
        <!-- Left: tag list -->
        <div class="tag-list">
          <div
            v-for="t in tagSummary"
            :key="t.value"
            class="tag-item"
            :class="{active: selectedTag === t.value}"
            @click="selectTag(t.value)"
          >
            <span class="tag-item-name">{{ t.value }}</span>
            <span class="tag-item-count">{{ t.count }}</span>
          </div>
          <div v-if="!tagSummary.length" class="muted" style="padding:20px;text-align:center">无标签</div>
        </div>

        <!-- Right: file panel (list + fixed pagination) -->
        <div class="file-panel">
          <div class="file-list" ref="fileListRef">
            <div v-if="selectedTag" class="file-list-head">
              <span class="muted" style="font-size:12px">{{ tagFileTotal }} 个文件 · {{ selectedDim }} / {{ selectedTag }}</span>
            </div>
            <div v-for="f in tagFiles" :key="f.file_id" class="file-row" :title="f.path" @dblclick="openFile(f.path)" @contextmenu.prevent="onFileCtx($event, f)">
              <span class="ext-badge" :class="extClass(f.ext)">{{ f.ext }}</span>
              <span class="file-name">{{ f.filename }}</span>
              <span class="file-path">{{ f.path }}</span>
              <span class="file-row-actions">
                <button v-if="isPreviewable(f)" class="hit-action-btn" @click.stop="previewFile(f)" title="预览">👁</button>
                <button v-if="vecReady" class="hit-action-btn" @click.stop="findSimilarFile(f)" title="查找相似文档">🔗</button>
              </span>
            </div>
            <div v-if="selectedTag && !tagFiles.length" class="muted" style="padding:20px;text-align:center">无文件</div>
            <div v-if="!selectedTag" class="muted" style="padding:40px 20px;text-align:center">← 点击左侧标签查看关联文件</div>
          </div>
          <!-- Pagination (fixed at bottom of panel) -->
          <div v-if="selectedTag" class="pager">
            <button class="btn ghost sm" :disabled="tagFilePage <= 1" @click="changePage(-1)">‹ 上一页</button>
            <span class="muted" style="font-size:12px">{{ tagFilePage }} / {{ tagFilePages }} 页 · 共 {{ tagFileTotal }} 个文件</span>
            <button class="btn ghost sm" :disabled="tagFilePage >= tagFilePages" @click="changePage(1)">下一页 ›</button>
          </div>
        </div>
      </div>
    </div>
    <!-- File context menu -->
    <div v-if="fileMenu" class="ctx-backdrop" @click="fileMenu = null" @contextmenu.prevent="fileMenu = null"></div>
    <div v-if="fileMenu" class="ctx-menu" :style="{ left: fileMenu.x + 'px', top: fileMenu.y + 'px' }">
      <div class="ctx-item" v-if="isPreviewable(fileMenu.f)" @click="previewFile(fileMenu.f)">👁 预览</div>
      <div class="ctx-item" v-if="vecReady" @click="findSimilarFile(fileMenu.f)">🔗 查找相似文档</div>
      <div class="ctx-sep" v-if="isPreviewable(fileMenu.f) || vecReady"></div>
      <div class="ctx-item" @click="ctxOpenFolder">📂 打开所在文件夹</div>
      <div class="ctx-item" @click="ctxCopyPath">📋 复制文件路径</div>
      <div class="ctx-item" @click="ctxCopyName">📋 复制文件名</div>
      <div class="ctx-item" @click="ctxCopyFolder">📋 复制文件夹路径</div>
    </div>

    <MarkdownPreview
      :path="previewPath"
      :file_id="previewFileId"
      @close="previewPath = ''"
    />
  </div>
</template>

<script setup>
import { ref, onMounted, nextTick } from 'vue'
import { api } from '../api.js'
import HelpTour from '../components/HelpTour.vue'
import MarkdownPreview from '../components/MarkdownPreview.vue'

// ── Prompt generator state ──
const genDims = ref([
  { name: '文档类型', tags: ['方案', '报告', '规范', '代码'], newTag: '' },
  { name: '项目阶段', tags: ['前期', '设计', '实施', '验收'], newTag: '' },
  { name: '技术领域', tags: ['物联网', '交通', '数据', '安全'], newTag: '' },
])
const prompt = ref('')
const copied = ref(false)
const expanded = ref(false)

function addDim() { genDims.value.push({ name: '', tags: [], newTag: '' }) }
function removeDim(i) { genDims.value.splice(i, 1) }
function addTag(i) {
  const d = genDims.value[i]
  const v = d.newTag.trim()
  if (v && !d.tags.includes(v)) d.tags.push(v)
  d.newTag = ''
}
function removeTag(i, j) { genDims.value[i].tags.splice(j, 1) }

// ── Master-detail tag browser state ──
const dims = ref([])
const selectedDim = ref('')
const tagSummary = ref([])
const selectedTag = ref('')
const tagFiles = ref([])
const tagFilePage = ref(1)
const tagFilePages = ref(1)
const tagFileTotal = ref(0)
const fileListRef = ref(null)

// ── Help tour steps (shared HelpTour component uses CSS selectors) ──
const tourSteps = [
  { selector: '#cls-gen-header', title: '分类 Prompt 生成器', text: '生成访谈式分类提示词，复制后粘贴到智能体对话。Agent 会先抽样侦察、提出候选方案并向你提问、试点打标 20-30 个，经你确认后才全量执行。点击标题栏可展开/收起。' },
  { selector: '#cls-dim-hint', title: '维度设置（可选先验）', text: '可选填写维度名称和标签（如：文档类型→方案/报告）。已填的维度作为先验，Agent 访谈时围绕它细化而不会从零提问；留空则 Agent 提出多套候选方案供你选。' },
  { selector: '#cls-gen-actions', title: '生成与复制', text: '点击「生成提示词」→「复制」，粘贴到智能体。Agent 通过 MCP 工具 memex_search/read/tag/report_confidence 完成侦察→访谈→试点→全量四阶段流程。' },
  { selector: '#cls-tag-browser', title: '分类结果浏览', text: 'Agent 完成打标后，这里按维度 → 标签 → 文件三级浏览。点击维度标签页 → 选择标签 → 查看关联文件，双击文件可直接打开。' },
]

// ── File preview & similar docs (reuse MarkdownPreview) ──
const previewPath = ref('')
const previewFileId = ref(null)
const vecReady = ref(false)

const PREVIEWABLE_EXTS = ['md', 'markdown', 'txt', 'csv', 'json', 'html', 'htm', 'xml', 'log', 'yml', 'yaml', 'toml', 'ini', 'conf']
function isPreviewable(f) {
  const ext = (f.ext || '').replace(/^\./, '').toLowerCase()
  return PREVIEWABLE_EXTS.includes(ext)
}
function previewFile(f) {
  fileMenu.value = null
  previewPath.value = f.path
  previewFileId.value = f.file_id
}
async function findSimilarFile(f) {
  // Reuse the preview modal, which has a built-in "相似文档" button
  fileMenu.value = null
  previewPath.value = f.path
  previewFileId.value = f.file_id
}
async function checkVectors() {
  try {
    const s = await api.vectorStatus()
    vecReady.value = !!s.has_vectors
  } catch (_) { vecReady.value = false }
}

// ── File context menu ──
const fileMenu = ref(null)
function onFileCtx(e, f) {
  fileMenu.value = { x: e.clientX, y: e.clientY, path: f.path, filename: f.filename, f }
}
async function ctxCopy(text) {
  try { await navigator.clipboard.writeText(text) }
  catch (_) {
    const ta = document.createElement('textarea')
    ta.value = text; document.body.appendChild(ta); ta.select()
    document.execCommand('copy'); document.body.removeChild(ta)
  }
}
function ctxOpenFolder() {
  const p = fileMenu.value?.path
  fileMenu.value = null
  if (p) api.openFolder(p).catch(() => {})
}
function ctxCopyPath() {
  ctxCopy(fileMenu.value?.path || '')
  fileMenu.value = null
}
function ctxCopyName() {
  ctxCopy(fileMenu.value?.filename || '')
  fileMenu.value = null
}
function ctxCopyFolder() {
  const p = fileMenu.value?.path || ''
  const idx = Math.max(p.lastIndexOf('\\'), p.lastIndexOf('/'))
  ctxCopy(idx >= 0 ? p.substring(0, idx) : '')
  fileMenu.value = null
}

async function selectDim(d) {
  selectedDim.value = d
  selectedTag.value = ''
  tagFiles.value = []
  tagFilePage.value = 1
  tagFilePages.value = 1
  tagFileTotal.value = 0
  try {
    const r = await api.tags.summary(d)
    tagSummary.value = r.tags || []
    // Auto-select first tag
    if (tagSummary.value.length) {
      await selectTag(tagSummary.value[0].value)
    }
  } catch (e) { console.error(e); tagSummary.value = [] }
}

async function selectTag(tag) {
  selectedTag.value = tag
  tagFilePage.value = 1
  await loadTagFiles()
}

async function loadTagFiles() {
  if (!selectedDim.value || !selectedTag.value) return
  try {
    const r = await api.tags.filesByTag(selectedDim.value, selectedTag.value, tagFilePage.value, 50)
    tagFiles.value = r.files || []
    tagFilePages.value = r.pages || 1
    tagFileTotal.value = r.total || 0
  } catch (e) { console.error(e); tagFiles.value = [] }
}

async function changePage(delta) {
  const np = tagFilePage.value + delta
  if (np < 1 || np > tagFilePages.value) return
  tagFilePage.value = np
  await loadTagFiles()
  // Scroll file list back to top
  await nextTick()
  if (fileListRef.value) fileListRef.value.scrollTop = 0
}

async function openFile(path) {
  try { await api.openFile(path) } catch (_) {}
}

function extClass(name) {
  const e = (name || '').split('.').pop().toLowerCase()
  if (['md', 'markdown', 'txt'].includes(e)) return 'ext-md'
  if (['docx', 'doc'].includes(e)) return 'ext-doc'
  if (['pdf'].includes(e)) return 'ext-pdf'
  if (['xlsx', 'xls'].includes(e)) return 'ext-sheet'
  if (['html', 'htm', 'js'].includes(e)) return 'ext-code'
  return 'ext-default'
}

function generatePrompt() {
  const sections = genDims.value
    .filter(d => d.name.trim())
    .map((d, i) => {
      const tagList = d.tags.length ? d.tags.join('、') : '（未指定，Agent 补充建议）'
      return `### ${i + 1}. ${d.name.trim()}\n标签：${tagList}`
    })
    .join('\n\n')

  const hasDims = !!sections
  const dimSection = hasDims
    ? `\n## 用户已指定的维度（先验，访谈时围绕它细化，不必再从零提问）\n\n${sections}\n`
    : ''

  prompt.value = `请帮我对 Memex 知识库中的文件进行分类归档打标。

**这不是一次性执行的指令，而是一个访谈式流程。分类方案必须经过我的确认才能全量执行。**${dimSection}
## 阶段 0 — 抽样侦察（先做）

1. 用 memex_search 搜索若干主题关键词，用 memex_stats 了解知识库概况
2. 抽样阅读 15-30 个文件（覆盖不同扩展名、不同文件夹），形成内容地图
3. 后续所有提问必须引用你真实读过的文件，不许凭空泛泛而问

## 阶段 1 — 候选方案 + 结构化提问

提出 2-3 套候选分类体系，每套包含：维度名、标签、适用场景、来自抽样文件的真实例证。
然后按优先级向我提问（一次最多 5 个，标注哪些可跳过）：

1. **用途**（必问）：这套分类主要拿来做什么——搜索过滤 / 归档导航 / 汇报统计？用途决定维度设计
2. **边界**（必问）：拿抽样中 2-3 个你拿不准的模糊文件问我"这个你会归哪类？"，用真实文件问，不用抽象描述
3. **粒度**（可跳过）：粗几大类还是细多级？
4. **多值**（可跳过）：一个文件在同一维度下允许打多个标签吗？

我答一部分就推进一部分，不必等全部答完。

## 阶段 2 — 试点打标（小步验证）

1. 选 20-30 个代表性文件（含模糊文件）先打标
2. 以表格形式列给我过目：文件名 | 维度 | 标签 | 一句话理由
3. 我说"对"才继续全量；说"不对"只改方案，试点标签推倒重来
4. 我对试点的反馈是最高优先级信息，直接修正你对真实意图的理解

## 阶段 3 — 全量 + 低置信度裁决

1. 全量打标（库大时分批，每批 200 个左右）
2. 置信度 < 0.85 的文件单独列表：文件名 | 你的犹豫原因
3. 把这批"拿不准"交我裁决——这批文件就是理解偏差所在，逐个给答案
4. 用户已指定的维度名和标签值严格使用指定名称，新增需说明
5. 完成后用 memex_report_confidence 上报

## 注意

- 发现标签同义、歧义、层级混乱时，指出并给修改建议
- 只为确认的文件打标，不确定就放入待裁决清单
- 每个文件在每个维度下最多一个标签，除非我在阶段 1 明确允许多值`
}

async function copyPrompt() {
  if (!prompt.value) return
  try {
    await navigator.clipboard.writeText(prompt.value)
    copied.value = true
    setTimeout(() => (copied.value = false), 2000)
  } catch (e) {
    const ta = document.createElement('textarea')
    ta.value = prompt.value
    document.body.appendChild(ta)
    ta.select()
    document.execCommand('copy')
    document.body.removeChild(ta)
    copied.value = true
    setTimeout(() => (copied.value = false), 2000)
  }
}

onMounted(async () => {
  checkVectors()
  try {
    const d = await api.tags.dims()
    dims.value = d.dims || []
    if (dims.value.length) await selectDim(dims.value[0])
  } catch (e) { console.error(e) }
})
</script>

<style scoped>
/* ── collapsible card header ── */
.gen-header {
  cursor: pointer;
  user-select: none;
  padding: 4px 0;
}
.gen-header:hover .card-title { color: var(--accent); }
.card.collapsed { padding-bottom: 12px; }
.card.collapsed .gen-header { margin-bottom: 0; }
.chev {
  font-size: 14px;
  color: var(--muted);
  transition: transform .2s ease;
  flex-shrink: 0;
}
.chev.rot { transform: rotate(90deg); }
/* auto mode concise hint */
.auto-hint {
  text-align: center;
  font-size: 13px;
  color: var(--fg);
  padding: 12px;
  margin-bottom: 0;
}
/* ── mode toggle ── */
.mode-toggle {
  display: flex;
  gap: 4px;
  margin-bottom: 12px;
  background: var(--bg);
  border-radius: 8px;
  padding: 4px;
}
.mode-btn {
  flex: 1;
  padding: 8px 12px;
  border: none;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 600;
  background: transparent;
  color: var(--muted);
  cursor: pointer;
  transition: all .15s;
}
.mode-btn:hover { color: var(--fg) }
.mode-btn.active {
  background: var(--accent);
  color: #1a1208;
}
/* ── dim block ── */
.dim-block {
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px;
  margin-bottom: 8px;
}
.dim-block .dim-name {
  max-width: 200px;
  font-weight: 600;
  border-color: var(--accent-dim);
}

/* ── tag chips inside dim block ── */
.tag-row {
  flex-wrap: wrap;
  gap: 6px;
  margin-top: 8px;
}
.dim-tag {
  cursor: pointer;
  transition: opacity .15s;
}
.dim-tag:hover { opacity: .7 }
.tag-input {
  width: 140px;
  flex: 1;
  min-width: 100px;
  font-size: 12px;
  padding: 5px 10px;
}

/* ── prompt textarea ── */
.prompt-text {
  background: var(--bg);
  border-color: var(--accent-dim);
  color: var(--fg);
  font-family: 'JetBrains Mono', 'Consolas', monospace;
}

/* ── tag browser (master-detail, fills viewport) ── */
.col { display: flex; flex-direction: column; height: calc(100vh - 40px); overflow: hidden; }
.col > .card:first-child { flex-shrink: 0; }
.tag-browser { padding: 0; overflow: hidden; flex: 1; display: flex; flex-direction: column; min-height: 0; }
.dim-tabs {
  display: flex;
  gap: 0;
  border-bottom: 1px solid var(--border);
  padding: 0 12px;
  flex-shrink: 0;
}
.dim-tab {
  padding: 12px 16px;
  border: none;
  background: transparent;
  color: var(--muted);
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  transition: all .15s;
  white-space: nowrap;
}
.dim-tab:hover { color: var(--fg); }
.dim-tab.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
}
.tag-body {
  display: flex;
  flex: 1;
  overflow: hidden;
  min-height: 0;
}
/* Left: tag list */
.tag-list {
  width: 200px;
  flex-shrink: 0;
  border-right: 1px solid var(--border);
  overflow-y: auto;
  padding: 4px 0;
}
.tag-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 14px;
  cursor: pointer;
  font-size: 13px;
  transition: background .12s;
}
.tag-item:hover { background: var(--bg); }
.tag-item.active {
  background: rgba(245,158,11,.12);
  color: var(--accent);
  font-weight: 600;
}
.tag-item-name { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.tag-item-count {
  font-size: 11px;
  color: var(--muted);
  background: rgba(255,255,255,0.06);
  padding: 1px 7px;
  border-radius: 10px;
  flex-shrink: 0;
  margin-left: 8px;
}
.tag-item.active .tag-item-count { color: var(--accent); background: rgba(245,158,11,.15); }
/* Right: file panel (container for list + pager) */
.file-panel {
  flex: 1;
  display: flex;
  flex-direction: column;
  overflow: hidden;
  min-width: 0;
  min-height: 0;
}
.file-list {
  flex: 1;
  overflow-y: auto;
  padding: 8px 14px;
}
.file-list-head {
  padding: 4px 0 8px;
  border-bottom: 1px solid var(--border);
  margin-bottom: 4px;
}
.file-row {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 7px 4px;
  border-bottom: 1px solid var(--border);
  font-size: 13px;
  cursor: pointer;
  transition: background .12s;
}
.file-row:hover { background: var(--bg); }
.file-row:last-child { border-bottom: none; }
.file-row-actions { margin-left: auto; display: none; gap: 2px; flex-shrink: 0; }
.file-row:hover .file-row-actions { display: inline-flex; }
.hit-action-btn {
  background: none; border: none; cursor: pointer;
  font-size: 13px; padding: 2px 5px; border-radius: 4px;
  opacity: .75;
}
.hit-action-btn:hover { opacity: 1; background: var(--bg-card); }
.ctx-sep { height: 1px; background: var(--border); margin: 4px 0; }
.file-name {
  color: var(--fg);
  font-weight: 500;
  flex-shrink: 0;
  max-width: 300px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.file-path {
  color: var(--muted);
  font-size: 11px;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* Pagination (fixed at bottom of file panel) */
.pager {
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 12px;
  padding: 10px 14px;
  flex-shrink: 0;
  border-top: 1px solid var(--border);
  background: var(--bg-card);
}

/* ── ext badges ── */
.ext-badge {
  display: inline-block;
  padding: 1px 6px;
  border-radius: 3px;
  font-size: 10px;
  font-family: var(--mono);
  text-transform: uppercase;
}
.ext-md { background: #1e3a5f; color: #60a5fa }
.ext-doc { background: #3b2f5f; color: #a78bfa }
.ext-pdf { background: #5f1e1e; color: #f87171 }
.ext-sheet { background: #1e5f2f; color: #4ade80 }
.ext-code { background: #5f4a1e; color: #fbbf24 }
.ext-default { background: #3f3f46; color: #a1a1aa }

/* ── File context menu ── */
.ctx-backdrop { position: fixed; inset: 0; z-index: 90; }
.ctx-menu {
  position: fixed; z-index: 100; min-width: 180px;
  background: #1e2024; border: 1px solid #2a2d33; border-radius: 6px;
  box-shadow: 0 8px 24px rgba(0,0,0,.5); padding: 4px 0;
}
.ctx-item { padding: 7px 14px; font-size: 13px; cursor: pointer; color: #e6e8eb; white-space: nowrap; }
.ctx-item:hover { background: rgba(245,158,11,.12); }
</style>
