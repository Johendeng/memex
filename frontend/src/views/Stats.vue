<template>
  <div class="col">
    <HelpTour :steps="tourSteps" />
    <!-- overview cards -->
    <div class="stats-grid" id="stats-overview">
      <div class="stat-card" v-for="c in cards" :key="c.label">
        <div class="stat-num">{{ c.value }}</div>
        <div class="stat-label">{{ c.label }}</div>
      </div>
    </div>

    <!-- duplicate cleanup -->
    <div class="card" id="stats-dedup">
      <span class="card-title">重复文件清理</span>
      <div class="row between" style="margin-top:8px;align-items:center">
        <span class="muted" style="font-size:12px">{{ dupStatus }}</span>
        <button class="btn" :disabled="dupScanning" @click="scanDups">
          {{ dupScanning ? '扫描中…' : '扫描重复' }}
        </button>
      </div>

      <div v-if="dupResult" style="margin-top:12px">
        <div class="dup-summary" v-if="dupResult.total_groups > 0">
          <span class="dup-badge">{{ dupResult.total_groups }} 组重复</span>
          <span class="dup-badge">{{ dupResult.total_dup_files }} 个冗余副本</span>
          <span class="dup-badge savings">可释放 {{ formatBytes(dupResult.total_savings_bytes) }}</span>
        </div>
        <div v-else class="muted" style="padding:12px 0;font-size:13px">未发现重复文件 ✅</div>

        <div class="dup-list" v-if="dupResult.groups.length">
          <div class="dup-group" v-for="g in dupResult.groups" :key="g.hash">
            <div class="dup-group-header">
              <label class="dup-check"><input type="checkbox" v-model="g.selected" /></label>
              <span class="dup-filename" :title="g.file_name">{{ g.file_name }}</span>
              <span class="dup-count">{{ g.copies.length }} 份</span>
              <span class="dup-savings">{{ formatBytes(g.savings_bytes) }}</span>
              <button class="btn-sm" @click="toggleGroup(g)">{{ g.expanded ? '收起' : '展开' }}</button>
            </div>
            <div class="dup-paths" v-if="g.expanded">
              <div class="dup-path" v-for="(c, i) in g.copies" :key="c.file_id">
                <span class="dup-path-keep" v-if="i === 0">✅ 保留</span>
                <span class="dup-path-del" v-else>🗑 删除</span>
                <span class="mono dup-path-text">{{ c.path }}</span>
                <span class="dup-path-depth">深度 {{ c.depth }}</span>
              </div>
            </div>
          </div>
        </div>

        <div class="dup-actions" v-if="dupResult.groups.length">
          <label class="dup-check-all">
            <input type="checkbox" :checked="allSelected" @change="toggleAll($event.target.checked)" /> 全选
          </label>
          <span class="muted" style="font-size:12px">
            已选 {{ selectedCount }} 组 · 可释放 {{ formatBytes(selectedSavings) }}
          </span>
          <button class="btn danger" :disabled="selectedCount === 0 || dupCleaning" @click="cleanDups">
            {{ dupCleaning ? '清理中…' : '确认清理' }}
          </button>
        </div>
      </div>

      <div v-if="cleanResult" class="dup-clean-result">
        <span v-if="cleanResult.deleted_files > 0" class="conf-badge conf-high">
          已删除 {{ cleanResult.deleted_files }} 个副本 · 释放 {{ formatBytes(cleanResult.freed_bytes) }} · {{ cleanResult.tagged_files }} 个文件已标记为高频
        </span>
        <span v-else class="conf-badge conf-low">没有文件被删除</span>
        <div v-if="cleanResult.errors.length" class="dup-errors">
          <div v-for="e in cleanResult.errors" :key="e" class="dup-error">{{ e }}</div>
        </div>
      </div>
    </div>

    <!-- file type distribution -->
    <div class="card" v-if="info">
      <span class="card-title">文件类型分布</span>
      <div class="ext-list">
        <div class="ext-row" v-for="[ext, count] in extEntries" :key="ext">
          <span class="ext-name mono">{{ ext }}</span>
          <div class="ext-bar-bg">
            <div class="ext-bar" :style="{ width: barWidth(count) + '%' }"></div>
          </div>
          <span class="ext-count">{{ count }}</span>
        </div>
      </div>
    </div>

    <!-- tag dimension statistics -->
    <div class="card" v-if="tagDims.length">
      <span class="card-title">标签维度统计</span>
      <div class="dim-stat" v-for="dim in tagDims" :key="dim.name">
        <div class="dim-stat-head">
          <span class="dim-stat-name">{{ dim.name }}</span>
          <span class="dim-stat-total">{{ dim.total }} 个文件 · {{ dim.tags.length }} 个标签</span>
        </div>
        <div class="dim-stat-bars">
          <div class="dim-stat-bar-row" v-for="t in dim.tags" :key="t.value">
            <span class="dim-stat-tag-name" :title="t.value">{{ t.value }}</span>
            <div class="dim-stat-bar-bg">
              <div class="dim-stat-bar" :style="{ width: barPct(dim, t) + '%' }"></div>
            </div>
            <span class="dim-stat-count">{{ t.count }}</span>
          </div>
        </div>
      </div>
    </div>

    <!-- confidence records (merged from Verified page) -->
    <div class="card" id="stats-confidence">
      <div class="row between" style="margin-bottom:12px">
        <span class="card-title" style="margin:0">可信度记录</span>
        <button class="btn ghost sm" @click="loadConfidence">刷新</button>
      </div>

      <div v-if="confLoading" class="muted" style="padding:20px 0">加载中…</div>

      <div v-else-if="!confRecords.length" class="empty-state">
        <p style="font-size:13px;margin-bottom:6px">暂无可信度记录</p>
        <p class="muted" style="font-size:12px">Agent 解析文件后可通过 MCP 上报可信度分数</p>
      </div>

      <div v-else>
        <div class="conf-stats row" style="gap:24px;margin-bottom:12px">
          <div class="stat" v-for="s in confStatItems" :key="s.lbl">
            <span class="num" :class="s.cls">{{ s.val }}</span>
            <span class="lbl">{{ s.lbl }}</span>
          </div>
        </div>
        <div class="conf-detail-row" v-for="(r, i) in confSorted" :key="i">
          <span class="ext-badge" :class="extClass(r.filename)">{{ ext(r.filename) }}</span>
          <div class="conf-detail-main">
            <div class="conf-detail-name" @click="openFile(r.path)" :title="r.path">
              {{ r.filename }}
              <span class="muted" style="font-size:11px;margin-left:8px">{{ r.path }}</span>
            </div>
            <div class="conf-detail-meta">
              <span class="muted" style="font-size:11px">{{ fmtTime(r.at) }}</span>
              <span v-if="r.note" class="muted" style="font-size:11px;margin-left:8px">· {{ r.note }}</span>
            </div>
          </div>
          <div class="conf-bar-wrap">
            <div class="conf-bar">
              <div class="conf-fill" :class="confBarClass(r)" :style="{ width: Math.round(r.score * 100) + '%' }"></div>
            </div>
            <span class="conf-pct" :class="confBarClass(r)">{{ Math.round(r.score * 100) }}%</span>
          </div>
          <span class="status-badge" :class="'st-' + r.status">{{ statusLabel(r.status) }}</span>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { api } from '../api.js'
import HelpTour from '../components/HelpTour.vue'

const tourSteps = [
  { selector: '#stats-overview', title: '索引概况', text: '展示知识库核心指标：文件数、chunk 数、bigram 数、文件夹数、总大小、向量数。一眼掌握索引规模。' },
  { selector: '#stats-dedup', title: '重复文件清理', text: '扫描内容完全相同的重复文件（SHA-256）。展开查看副本路径，勾选后确认清理，自动保留路径最浅的一份，删除其余副本并标记为高频文件。' },
  { selector: '#stats-confidence', title: '可信度记录', text: 'Agent 解析文件后通过 MCP 上报可信度分数。此处展示已验证/低于阈值/待复核的统计，以及每条记录的进度条、状态和文件信息。点击文件名可直接打开。' },
]

const info = ref(null)
const tagDims = ref([])
// confidence (merged from Verified)
const confRecords = ref([])
const confStats = ref(null)
const confLoading = ref(true)

const confSorted = computed(() =>
  [...confRecords.value].sort((a, b) => a.score - b.score)
)

const confStatItems = computed(() => {
  const s = confStats.value || {}
  const total = s.total_files || 0
  const avg = s.verified > 0 && confRecords.value.length
    ? (confRecords.value.reduce((a, r) => a + r.score, 0) / confRecords.value.length * 100).toFixed(0) + '%'
    : '—'
  return [
    { lbl: '已验证', val: s.verified ?? 0, cls: 'ok' },
    { lbl: '低于阈值', val: s.below ?? 0, cls: s.below > 0 ? 'warn' : 'ok' },
    { lbl: '待复核', val: (s.reverify ?? 0) + (s.unverified ?? 0), cls: '' },
    { lbl: '平均可信度', val: avg, cls: '' },
  ]
})

async function loadConfidence() {
  confLoading.value = true
  try {
    const r = await api.confidence.list()
    confRecords.value = r.records || []
    confStats.value = r.stats || null
  } catch (e) { console.error(e) }
  finally { confLoading.value = false }
}

function statusLabel(s) {
  return { verified: '已验证', below: '低于阈值', reverify: '需复核' }[s] || s
}
function confBarClass(r) {
  if (r.status === 'verified' || r.score >= 0.8) return 'good'
  if (r.score >= 0.5) return 'mid'
  return 'bad'
}
function ext(name) {
  if (!name) return 'file'
  const i = name.lastIndexOf('.')
  return i >= 0 ? name.slice(i + 1).toLowerCase() : 'txt'
}
function extClass(name) {
  const e = ext(name)
  if (['md', 'markdown', 'txt'].includes(e)) return 'ext-md'
  if (['docx', 'doc'].includes(e)) return 'ext-doc'
  if (['pdf'].includes(e)) return 'ext-pdf'
  if (['xlsx', 'xls'].includes(e)) return 'ext-sheet'
  return 'ext-default'
}
function fmtTime(epoch) {
  if (!epoch) return ''
  const d = new Date(epoch * 1000)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}
async function openFile(path) {
  if (!path) return
  try { await api.openFile(path) } catch (_) {}
}

// ---- dedup state ----
const dupResult = ref(null)
const dupScanning = ref(false)
const dupCleaning = ref(false)
const cleanResult = ref(null)

const dupStatus = computed(() => {
  if (dupScanning.value) return '正在扫描文件哈希…'
  if (dupResult.value) {
    const r = dupResult.value
    if (r.total_groups === 0) return '扫描完成，无重复文件'
    return `扫描完成：${r.total_groups} 组重复，可释放 ${formatBytes(r.total_savings_bytes)}`
  }
  return '点击扫描检测重复文件'
})

const selectedGroups = computed(() =>
  (dupResult.value?.groups || []).filter(g => g.selected)
)
const selectedCount = computed(() => selectedGroups.value.length)
const selectedSavings = computed(() =>
  selectedGroups.value.reduce((sum, g) => sum + g.savings_bytes, 0)
)
const allSelected = computed(() =>
  dupResult.value?.groups?.length > 0 &&
  selectedGroups.value.length === dupResult.value.groups.length
)

function toggleGroup(g) { g.expanded = !g.expanded }

function toggleAll(checked) {
  if (dupResult.value) dupResult.value.groups.forEach(g => g.selected = checked)
}

async function scanDups() {
  dupScanning.value = true
  cleanResult.value = null
  try {
    const r = await api.dedup.scan()
    r.groups.forEach(g => { g.selected = false; g.expanded = false })
    dupResult.value = r
  } catch (e) {
    alert('扫描失败: ' + e.message)
  } finally {
    dupScanning.value = false
  }
}

async function cleanDups() {
  const hashes = selectedGroups.value.map(g => g.hash)
  if (!confirm(`确认删除 ${hashes.length} 组重复文件？\n将释放 ${formatBytes(selectedSavings.value)} 磁盘空间。\n此操作不可恢复！`)) return
  dupCleaning.value = true
  try {
    const r = await api.dedup.clean(hashes)
    cleanResult.value = r
    await scanDups()
    try { info.value = await api.info() } catch (_) {}
  } catch (e) {
    alert('清理失败: ' + e.message)
  } finally {
    dupCleaning.value = false
  }
}

function formatBytes(bytes) {
  if (!bytes || bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  let i = 0, val = bytes
  while (val >= 1024 && i < units.length - 1) { val /= 1024; i++ }
  return val.toFixed(i === 0 ? 0 : 1) + ' ' + units[i]
}

const cards = computed(() => {
  if (!info.value) return []
  const i = info.value
  const sizeMB = i.total_bytes ? (i.total_bytes / 1048576).toFixed(1) + ' MB' : '—'
  const vecInfo = i.has_vectors ? `${i.vector_count}` : '未生成'
  return [
    { label: '文件', value: i.files },
    { label: 'chunk', value: i.chunks },
    { label: 'bigram', value: i.bigrams },
    { label: '文件夹', value: i.folders },
    { label: '总大小', value: sizeMB },
    { label: '向量', value: vecInfo },
  ]
})

const extEntries = computed(() => {
  if (!info.value?.ext_dist) return []
  return Object.entries(info.value.ext_dist).sort((a, b) => b[1] - a[1])
})

function barWidth(count) {
  const max = Math.max(...extEntries.value.map(e => e[1]))
  return max > 0 ? Math.round(count / max * 100) : 0
}

// ---- tag dimension statistics ----
async function loadTagStats() {
  try {
    const d = await api.tags.dims()
    const dims = d.dims || []
    const results = await Promise.all(dims.map(async dim => {
      const r = await api.tags.summary(dim)
      const tags = (r.tags || []).sort((a, b) => b.count - a.count)
      const total = tags.reduce((s, t) => s + t.count, 0)
      return { name: dim, tags, total }
    }))
    tagDims.value = results
  } catch (e) { console.error(e) }
}

function barPct(dim, t) {
  const max = Math.max(...dim.tags.map(x => x.count), 1)
  return Math.round(t.count / max * 100)
}

onMounted(async () => {
  try { info.value = await api.info() } catch (_) {}
  await loadTagStats()
  await loadConfidence()
})
</script>

<style scoped>
.stats-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
  gap: 12px;
  margin-bottom: 16px;
}
.stat-card {
  background: var(--card-bg, #1a1f26);
  border: 1px solid var(--border, #2a3038);
  border-radius: 10px;
  padding: 16px 12px;
  text-align: center;
}
.stat-num {
  font-size: 24px;
  font-weight: 700;
  color: #f59e0b;
}
.stat-label {
  font-size: 11px;
  color: #888;
  margin-top: 4px;
}
.ext-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 12px;
}
.ext-row {
  display: flex;
  align-items: center;
  gap: 10px;
  font-size: 12px;
}
.ext-name {
  width: 60px;
  text-align: right;
}
.ext-bar-bg {
  flex: 1;
  height: 18px;
  background: rgba(255,255,255,0.05);
  border-radius: 4px;
  overflow: hidden;
}
.ext-bar {
  height: 100%;
  background: linear-gradient(90deg, #f59e0b, #f59e0b88);
  border-radius: 4px;
  transition: width 0.3s;
}
.ext-count {
  width: 30px;
  font-weight: 600;
}
/* ---- confidence records (merged from Verified) ---- */
.empty-state { padding: 24px 0; text-align: center; }
.conf-stats .stat { text-align: center; }
.conf-stats .num { font-size: 22px; font-weight: 700; display: block; }
.conf-stats .num.ok { color: #4ade80; }
.conf-stats .num.warn { color: #fbbf24; }
.conf-stats .lbl { font-size: 11px; color: #888; }
.conf-detail-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 0;
  border-bottom: 1px solid var(--border);
}
.conf-detail-row:last-child { border-bottom: none }
.conf-detail-main { flex: 1; min-width: 0 }
.conf-detail-name {
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.conf-detail-name:hover { color: var(--accent) }
.conf-detail-meta { font-size: 11px; margin-top: 2px }
.conf-bar-wrap { display: flex; align-items: center; gap: 8px; min-width: 140px; }
.conf-bar { flex: 1; height: 6px; background: rgba(255,255,255,0.08); border-radius: 3px; overflow: hidden; }
.conf-fill { height: 100%; border-radius: 3px; transition: width 0.3s; }
.conf-fill.good { background: var(--accent) }
.conf-fill.mid { background: #fbbf24 }
.conf-fill.bad { background: #ef4444 }
.conf-pct { font-size: 12px; font-family: var(--mono); min-width: 36px; text-align: right }
.conf-pct.good { color: var(--accent) }
.conf-pct.mid { color: #fbbf24 }
.conf-pct.bad { color: #ef4444 }
.status-badge { font-size: 11px; padding: 2px 8px; border-radius: 4px; min-width: 60px; text-align: center; }
.st-verified { background: rgba(59,130,246,0.15); color: #60a5fa }
.st-below { background: rgba(239,68,68,0.15); color: #f87171 }
.st-reverify { background: rgba(245,158,11,0.15); color: #fbbf24 }
.ext-badge { display: inline-block; padding: 1px 6px; border-radius: 3px; font-size: 10px; font-family: var(--mono); text-transform: uppercase; }
.ext-md { background: #1e3a5f; color: #60a5fa }
.ext-doc { background: #3b2f5f; color: #a78bfa }
.ext-pdf { background: #5f1e1e; color: #f87171 }
.ext-sheet { background: #1e5f2f; color: #4ade80 }
.ext-default { background: #3f3f46; color: #a1a1aa }

/* ---- dedup styles ---- */
.dup-summary { display: flex; gap: 8px; flex-wrap: wrap; margin-bottom: 12px; }
.dup-badge { padding: 4px 10px; border-radius: 6px; background: rgba(255,255,255,0.06); font-size: 12px; font-weight: 600; }
.dup-badge.savings { background: #f59e0b22; color: #fbbf24; }
.dup-list { max-height: 400px; overflow-y: auto; border: 1px solid #2a3038; border-radius: 8px; }
.dup-group { border-bottom: 1px solid #2a3038; }
.dup-group:last-child { border-bottom: none; }
.dup-group-header { display: flex; align-items: center; gap: 8px; padding: 8px 12px; font-size: 12px; }
.dup-check input { cursor: pointer; }
.dup-filename { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; }
.dup-count { color: #fbbf24; font-weight: 600; white-space: nowrap; }
.dup-savings { color: #4ade80; white-space: nowrap; }
.btn-sm { padding: 2px 8px; font-size: 11px; border: 1px solid #2a3038; border-radius: 4px; background: transparent; color: #aaa; cursor: pointer; }
.btn-sm:hover { background: rgba(255,255,255,0.05); }
.dup-paths { padding: 4px 12px 8px 32px; }
.dup-path { display: flex; align-items: center; gap: 8px; padding: 2px 0; font-size: 11px; }
.dup-path-keep { color: #4ade80; white-space: nowrap; font-weight: 600; }
.dup-path-del { color: #f87171; white-space: nowrap; font-weight: 600; }
.dup-path-text { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: #888; }
.dup-path-depth { color: #555; white-space: nowrap; }
.dup-actions { display: flex; align-items: center; gap: 12px; margin-top: 12px; padding: 8px 0; }
.dup-check-all { font-size: 12px; display: flex; align-items: center; gap: 4px; }
.dup-check-all input { cursor: pointer; }
.btn { padding: 6px 14px; border: 1px solid #2a3038; border-radius: 6px; background: rgba(255,255,255,0.04); color: #ddd; cursor: pointer; font-size: 13px; }
.btn:hover:not(:disabled) { background: rgba(255,255,255,0.08); }
.btn:disabled { opacity: 0.4; cursor: not-allowed; }
.btn.danger { border-color: #ef444455; color: #f87171; }
.btn.danger:hover:not(:disabled) { background: #ef444422; }
.dup-clean-result { margin-top: 12px; padding: 8px 0; }
.dup-errors { margin-top: 8px; }
.dup-error { color: #f87171; font-size: 11px; padding: 2px 0; }
/* ---- tag dimension statistics ---- */
.dim-stat { margin-top: 16px; }
.dim-stat:first-of-type { margin-top: 12px; }
.dim-stat-head { display: flex; align-items: baseline; gap: 8px; margin-bottom: 8px; }
.dim-stat-name { font-size: 13px; font-weight: 700; color: var(--fg); }
.dim-stat-total { font-size: 11px; color: var(--muted); }
.dim-stat-bars { display: flex; flex-direction: column; gap: 4px; }
.dim-stat-bar-row { display: flex; align-items: center; gap: 10px; font-size: 12px; }
.dim-stat-tag-name { width: 100px; text-align: right; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--fg); }
.dim-stat-bar-bg { flex: 1; height: 16px; background: rgba(255,255,255,0.05); border-radius: 4px; overflow: hidden; }
.dim-stat-bar { height: 100%; background: linear-gradient(90deg, #f59e0b, #f59e0b88); border-radius: 4px; transition: width 0.3s; }
.dim-stat-count { width: 30px; font-weight: 600; color: var(--accent); }
</style>
