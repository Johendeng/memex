<template>
  <div class="col">
    <!-- Stats -->
    <div class="card">
      <div class="row" style="gap:32px;margin-bottom:12px">
        <div class="stat" v-for="s in statItems" :key="s.lbl">
          <span class="num" :class="s.cls">{{ s.val }}</span>
          <span class="lbl">{{ s.lbl }}</span>
        </div>
      </div>
    </div>

    <!-- Records -->
    <div class="card">
      <div class="row between" style="margin-bottom:12px">
        <span class="card-title" style="margin:0">可信度记录</span>
        <button class="btn ghost sm" @click="load">刷新</button>
      </div>

      <div v-if="loading" class="muted" style="padding:20px 0">加载中…</div>

      <div v-else-if="records.length === 0" class="empty-state">
        <p style="font-size:13px;margin-bottom:6px">暂无可信度记录</p>
        <p class="muted" style="font-size:12px">Agent 解析文件后可通过 MCP 上报可信度分数</p>
      </div>

      <div v-else>
        <div class="conf-row" v-for="(r, i) in sorted" :key="i">
          <span class="ext-badge" :class="extClass(r.filename)">{{ ext(r.filename) }}</span>
          <div class="conf-main">
            <div class="conf-name" @click="open(r.path)" :title="r.path">
              {{ r.filename }}
              <span class="muted" style="font-size:11px;margin-left:8px">{{ r.path }}</span>
            </div>
            <div class="conf-meta">
              <span class="muted" style="font-size:11px">{{ fmt(r.at) }}</span>
              <span v-if="r.note" class="muted" style="font-size:11px;margin-left:8px">· {{ r.note }}</span>
            </div>
          </div>
          <div class="conf-bar-wrap">
            <div class="conf-bar">
              <div class="conf-fill" :class="barClass(r)" :style="{ width: Math.round(r.score * 100) + '%' }"></div>
            </div>
            <span class="conf-pct" :class="barClass(r)">{{ Math.round(r.score * 100) }}%</span>
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

const records = ref([])
const stats = ref(null)
const loading = ref(true)

const statItems = computed(() => {
  const s = stats.value || {}
  const total = s.total_files || 0
  const avg = s.verified > 0 ? (records.value.reduce((a, r) => a + r.score, 0) / records.value.length * 100).toFixed(0) + '%' : '—'
  return [
    { lbl: '已验证', val: s.verified ?? 0, cls: 'ok' },
    { lbl: '低于阈值', val: s.below ?? 0, cls: s.below > 0 ? 'warn' : 'ok' },
    { lbl: '待复核', val: (s.reverify ?? 0) + (s.unverified ?? 0), cls: '' },
    { lbl: '平均可信度', val: avg, cls: '' },
  ]
})

const sorted = computed(() =>
  [...records.value].sort((a, b) => a.score - b.score)
)

function statusLabel(s) {
  return { verified: '已验证', below: '低于阈值', reverify: '需复核' }[s] || s
}
function barClass(r) {
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
function fmt(epoch) {
  if (!epoch) return ''
  const d = new Date(epoch * 1000)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')} ${String(d.getHours()).padStart(2, '0')}:${String(d.getMinutes()).padStart(2, '0')}`
}
async function open(path) {
  if (!path) return
  try { await api.openFile(path) } catch (_) {}
}

async function load() {
  loading.value = true
  try {
    const r = await api.confidence.list()
    records.value = r.records || []
    stats.value = r.stats || null
  } catch (e) {
    console.error(e)
  } finally {
    loading.value = false
  }
}

onMounted(load)
</script>

<style scoped>
.empty-state {
  padding: 24px 0;
  text-align: center;
}
.conf-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 0;
  border-bottom: 1px solid var(--border);
}
.conf-row:last-child { border-bottom: none }
.conf-main { flex: 1; min-width: 0 }
.conf-name {
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.conf-name:hover { color: var(--accent) }
.conf-meta { font-size: 11px; margin-top: 2px }
.conf-bar-wrap {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 140px;
}
.conf-bar {
  flex: 1;
  height: 6px;
  background: rgba(255,255,255,0.08);
  border-radius: 3px;
  overflow: hidden;
}
.conf-fill {
  height: 100%;
  border-radius: 3px;
  transition: width 0.3s;
}
.conf-fill.good { background: var(--accent) }
.conf-fill.mid { background: #fbbf24 }
.conf-fill.bad { background: #ef4444 }
.conf-pct { font-size: 12px; font-family: var(--mono); min-width: 36px; text-align: right }
.conf-pct.good { color: var(--accent) }
.conf-pct.mid { color: #fbbf24 }
.conf-pct.bad { color: #ef4444 }
.status-badge {
  font-size: 11px;
  padding: 2px 8px;
  border-radius: 4px;
  min-width: 60px;
  text-align: center;
}
.st-verified { background: rgba(59,130,246,0.15); color: #60a5fa }
.st-below { background: rgba(239,68,68,0.15); color: #f87171 }
.st-reverify { background: rgba(245,158,11,0.15); color: #fbbf24 }
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
.ext-default { background: #3f3f46; color: #a1a1aa }
</style>
