<template>
  <div class="col">
    <div class="card">
      <div class="row between">
        <input v-model="newPath" class="input" placeholder="D:\资料\技术文档" @keyup.enter="add" style="flex:1" />
        <button class="btn ghost sm" @click="browse" :disabled="busy">浏览…</button>
        <button class="btn" @click="add">添加源</button>
      </div>
    </div>
    <div v-if="status" class="status" :class="statusType">{{ status }}</div>
    <div v-if="progress" class="progress-box">
      <div class="progress-text">
        <span>已处理 {{ progress.done }}/{{ progress.total }} 文件</span>
        <span class="muted cur-file" v-if="progress.current">· {{ shortName(progress.current) }}</span>
      </div>
      <div class="progress-track">
        <div class="progress-fill" :style="{ width: progress.percent + '%' }"></div>
      </div>
    </div>
    <div class="card">
      <div class="row between" style="margin-bottom:12px">
        <span class="card-title" style="margin:0">已注册文件源 ({{ folders.length }})</span>
        <div class="row" style="gap:8px">
          <button class="btn ghost sm" @click="reindex" :disabled="busy">重新索引</button>
          <button class="btn ghost sm" @click="reload">刷新</button>
        </div>
      </div>
      <table v-if="folders.length">
        <thead><tr><th>#</th><th>路径</th><th>添加时间</th><th></th></tr></thead>
        <tbody>
          <tr v-for="f in folders" :key="f.id">
            <td class="mono muted">{{ f.id }}</td>
            <td>{{ f.path }}</td>
            <td class="muted mono">{{ fmt(f.added_at) }}</td>
            <td><button class="btn danger sm" @click="remove(f.id)">移除</button></td>
          </tr>
        </tbody>
      </table>
      <div v-else class="empty">{{ msg }}</div>
    </div>
  </div>
</template>

<script setup>
import { ref, onMounted } from 'vue'
import { api } from '../api.js'

const folders = ref([])
const newPath = ref('')
const msg = ref('加载中…')
const status = ref('')
const statusType = ref('info')
const busy = ref(false)
const progress = ref(null)

function setOk(text) { status.value = text; statusType.value = 'success' }
function setErr(text) { status.value = text; statusType.value = 'error' }

// 添加/重索引后调用:解析文件 → 建索引,并把结果反馈给用户
async function doIndex(prefix) {
  busy.value = true
  status.value = prefix + '索引中…'
  statusType.value = 'info'

  // 轮询进度，让 UI 实时反馈
  const timer = setInterval(async () => {
    try { progress.value = await api.index.progress() }
    catch (_) { /* ignore */ }
  }, 400)

  try {
    await api.index.run()
    // 索引是后台异步的，轮询直到完成
    await new Promise((resolve) => {
      const poll = setInterval(async () => {
        try {
          const p = await api.index.progress()
          progress.value = p
          if (!p.phase || p.phase === 0) {
            clearInterval(poll)
            resolve()
          }
        } catch (_) {
          clearInterval(poll)
          resolve()
        }
      }, 1000)
    })
    clearInterval(timer)
    progress.value = null
    // 从 cmd_info 获取最终统计
    const info = await api.info()
    setOk(`已索引 ${info.files} 个文件 / ${info.chunks} 个分块`)
    await reload()
  } catch (e) {
    clearInterval(timer)
    progress.value = null
    setErr(e.message)
  } finally { busy.value = false }
}

function shortName(path) {
  if (!path) return ''
  const i = path.lastIndexOf('/')
  return i >= 0 ? path.slice(i + 1) : path
}

// added_at 是 epoch 秒(字符串) → yyyy-MM-dd HH:mm:ss
function fmt(ts) {
  const n = Number(ts)
  if (!n) return ts
  const d = new Date(n * 1000)
  const p = (x) => String(x).padStart(2, '0')
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`
}

async function reload() {
  try {
    folders.value = await api.folders.list()
    if (!folders.value.length) msg.value = '尚未添加任何文件夹源'
  } catch (e) { msg.value = e.message }
}
async function add() {
  const p = newPath.value.trim()
  if (!p) return
  busy.value = true
  status.value = '正在添加文件夹…'
  statusType.value = 'info'
  try {
    await api.folders.add(p)
    newPath.value = ''
    await doIndex('添加完成,')
  } catch (e) {
    setErr(e.message)
    busy.value = false
  }
}
async function browse() {
  try {
    const r = await api.pickFolder()
    if (r.path) newPath.value = r.path
  } catch (e) {
    setErr(e.message)
  }
}
async function remove(id) {
  if (!confirm('移除文件源将同步删除关联的索引数据，剩余文件源将自动重新索引。确认移除？')) return
  busy.value = true
  status.value = '正在移除并重新索引…'
  statusType.value = 'info'
  try {
    await api.folders.remove(id)
    // Poll reindex progress until complete
    await new Promise((resolve) => {
      const timer = setInterval(async () => {
        try {
          const p = await api.index.progress()
          if (p.phase === 0 || !p.phase) {
            clearInterval(timer)
            resolve()
          }
        } catch { clearInterval(timer); resolve() }
      }, 800)
    })
    await reload()
    const info = await api.info()
    setOk(`已移除文件源，剩余 ${info.files} 个文件已重新索引`)
  } catch (e) {
    setErr(e.message)
  } finally {
    busy.value = false
  }
}
async function reindex() {
  if (busy.value || !folders.value.length) return
  await doIndex('重新')
}
onMounted(reload)
</script>

<style scoped>
.status {
  padding: 8px 12px;
  border-radius: 6px;
  font-size: 13px;
  margin-bottom: 12px;
  border: 1px solid rgba(245, 158, 11, 0.4);
  background: rgba(245, 158, 11, 0.08);
  color: #e6e8eb;
}
.status.success {
  border-color: rgba(34, 197, 94, 0.4);
  background: rgba(34, 197, 94, 0.08);
}
.status.error {
  border-color: rgba(239, 68, 68, 0.4);
  background: rgba(239, 68, 68, 0.08);
  color: #fca5a5;
}
.progress-box {
  margin: 0 0 12px;
  padding: 10px 12px;
  border: 1px solid var(--border, #2a2e35);
  border-radius: 6px;
  background: var(--card, #111215);
}
.progress-text {
  font-size: 13px;
  margin-bottom: 6px;
  display: flex;
  gap: 6px;
  align-items: baseline;
}
.cur-file {
  font-size: 12px;
  color: var(--muted-foreground, #8a9099);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 400px;
}
.progress-track {
  height: 6px;
  background: var(--border, #2a2e35);
  border-radius: 3px;
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  background: #f59e0b;
  border-radius: 3px;
  transition: width 0.3s ease;
}
</style>
