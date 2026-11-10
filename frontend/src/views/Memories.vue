<template>
  <div class="col">
    <HelpTour :steps="tourSteps" />
    <!-- Tab switcher -->
    <div class="tab-bar">
      <button class="tab-btn" :class="{ active: tab === 'kv' }" @click="switchTab('kv')">
        🧠 要点记忆 <span class="tab-count" v-if="kvCount">{{ kvCount }}</span>
      </button>
      <button class="tab-btn" :class="{ active: tab === 'notes' }" @click="switchTab('notes')">
        📝 详细笔记 <span class="tab-count" v-if="notesCount">{{ notesCount }}</span>
      </button>
    </div>

    <!-- ============ KV Memory tab ============ -->
    <template v-if="tab === 'kv'">
      <div class="card">
        <div class="row between" style="flex-wrap:wrap;gap:8px">
          <div class="row" style="gap:8px;flex:1;min-width:200px">
            <input class="input" v-model="kvSearch" placeholder="搜索名称或内容…" @input="filterKv" style="flex:1" />
          </div>
          <button class="btn sm" @click="openKvEditor(null)">+ 新增记忆</button>
          <button class="btn ghost sm" @click="loadKv">刷新</button>
        </div>
      </div>

      <div v-if="kvLoading" class="empty">加载中…</div>
      <div v-else-if="filteredKv.length === 0" class="empty">
        {{ kvSearch ? '没有匹配的记忆' : '暂无记忆。智能体在与你交流时记住的关键信息会显示在这里。' }}
      </div>
      <div v-else class="kv-list">
        <div class="kv-card" v-for="(m, i) in filteredKv" :key="i">
          <div class="kv-head">
            <span class="kv-key">{{ m.key }}</span>
            <span class="tag" v-if="m.namespace">{{ m.namespace }}</span>
            <span class="kv-time mono">{{ formatTime(m.at) }}</span>
            <div class="kv-actions">
              <button class="btn ghost sm" @click="openKvEditor(m)">编辑</button>
              <button class="btn danger sm" @click="deleteKv(m)">删除</button>
            </div>
          </div>
          <div class="kv-value">{{ m.value }}</div>
          <div class="kv-note muted" v-if="m.note">📎 {{ m.note }}</div>
        </div>
      </div>
    </template>

    <!-- ============ Notes tab ============ -->
    <template v-if="tab === 'notes'">
      <div class="card">
        <div class="row between" style="flex-wrap:wrap;gap:8px">
          <span class="muted" style="font-size:12px">智能体在交流过程中整理的详细笔记</span>
          <button class="btn ghost sm" @click="loadNotes">刷新</button>
        </div>
      </div>

      <div v-if="notesLoading" class="empty">加载中…</div>
      <div v-else-if="notes.length === 0" class="empty">暂无笔记。智能体整理的详细笔记会显示在这里。</div>
      <div v-else class="notes-list">
        <div class="note-card" v-for="n in notes" :key="n.name">
          <div class="note-head">
            <span class="note-title">{{ n.name }}</span>
            <span class="tag">{{ formatSize(n.size) }}</span>
            <span class="note-time mono">{{ formatTime(n.mtime) }}</span>
            <div class="note-actions">
              <button class="btn ghost sm" @click="readNote(n)">查看</button>
              <button class="btn danger sm" @click="deleteNote(n)">删除</button>
            </div>
          </div>
          <div class="note-preview">{{ n.preview }}</div>
        </div>
      </div>
    </template>

    <!-- ============ KV Editor Modal ============ -->
    <transition name="fade">
      <div v-if="kvEditing" class="modal-overlay" @click.self="closeKvEditor">
        <div class="modal-box">
          <div class="modal-head">
            <span>{{ kvEditOrig ? '编辑记忆' : '新增记忆' }}</span>
            <button class="md-close" @click="closeKvEditor">✕</button>
          </div>
          <div class="modal-body">
            <label class="modal-label">名称 <span class="muted" style="font-size:11px">用于标识这条记忆</span></label>
            <input class="input" v-model="kvForm.key" placeholder="如：用户偏好-语言" :disabled="!!kvEditOrig" />
            <label class="modal-label">分组</label>
            <input class="input" v-model="kvForm.namespace" placeholder="默认" />
            <label class="modal-label">内容</label>
            <textarea class="input" v-model="kvForm.value" rows="4" placeholder="记忆内容…"></textarea>
            <label class="modal-label">备注 <span class="muted" style="font-size:11px">可选补充说明</span></label>
            <input class="input" v-model="kvForm.note" placeholder="补充说明…" />
          </div>
          <div class="modal-foot">
            <span v-if="kvMsg" class="md-msg" :class="{ err: kvMsgType === 'err' }">{{ kvMsg }}</span>
            <button class="btn ghost sm" @click="closeKvEditor">取消</button>
            <button class="btn sm" @click="saveKv" :disabled="kvSaving">
              {{ kvSaving ? '保存中…' : '保存' }}
            </button>
          </div>
        </div>
      </div>
    </transition>

    <!-- ============ Note Viewer Drawer ============ -->
    <transition name="slide">
      <div v-if="noteViewing" class="md-drawer">
        <div class="md-header">
          <div class="md-title">
            <span class="ext-badge ext-md">md</span>
            <span class="md-name">{{ noteViewing.name }}</span>
          </div>
          <div class="md-actions">
            <button class="md-close" @click="noteViewing = null">✕</button>
          </div>
        </div>
        <div v-if="noteLoading" class="md-loading">加载中…</div>
        <div v-else class="md-body markdown-body" v-html="renderedNote"></div>
      </div>
    </transition>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { marked } from 'marked'
import { api } from '../api.js'
import HelpTour from '../components/HelpTour.vue'

const tourSteps = [
  { selector: '.tab-bar', title: '两种记忆', text: '智能体的记忆分为「要点记忆」和「详细笔记」两种。要点记忆是简短的关键信息（如你的偏好、项目名称等），详细笔记是智能体整理的长文笔记。点击标签页切换查看。' },
  { selector: '.tab-bar .tab-btn.active', title: '当前标签页', text: '高亮的标签页是你当前在查看的记忆类型。每个标签后面的数字表示该类型有多少条记忆。' },
  { selector: '.kv-list, .notes-list', title: '记忆卡片', text: '每条记忆以卡片形式展示，包含名称、内容、时间和操作按钮。你可以编辑或删除任何一条记忆，智能体写入的内容你都可以修改或清理。' },
  { selector: '.btn.sm', title: '手动添加', text: '在「要点记忆」页，你可以点击「+ 新增记忆」手动添加一条记忆。这在你想告诉智能体一些重要信息时很有用，它会记住并在后续对话中使用。' },
]

// ---- Tab state ----
const tab = ref('kv')

// ---- KV memories ----
const kvList = ref([])
const kvCount = ref(0)
const kvLoading = ref(false)
const kvSearch = ref('')
const kvEditing = ref(false)
const kvSaving = ref(false)
const kvEditOrig = ref(null)
const kvForm = ref({ key: '', value: '', namespace: 'default', note: '' })
const kvMsg = ref('')
const kvMsgType = ref('')

const filteredKv = computed(() => {
  if (!kvSearch.value) return kvList.value
  const q = kvSearch.value.toLowerCase()
  return kvList.value.filter(m =>
    m.key.toLowerCase().includes(q) ||
    m.value.toLowerCase().includes(q) ||
    (m.namespace || '').toLowerCase().includes(q)
  )
})

function filterKv() { /* computed handles it */ }

async function loadKv() {
  kvLoading.value = true
  try {
    const r = await api.memory.list()
    if (r.ok) {
      kvList.value = r.memories || []
      kvCount.value = r.count || 0
    }
  } catch (e) {
    console.error('load memories:', e)
  } finally {
    kvLoading.value = false
  }
}

function openKvEditor(m) {
  kvMsg.value = ''
  if (m) {
    kvEditOrig.value = m
    kvForm.value = { key: m.key, value: m.value, namespace: m.namespace || 'default', note: m.note || '' }
  } else {
    kvEditOrig.value = null
    kvForm.value = { key: '', value: '', namespace: 'default', note: '' }
  }
  kvEditing.value = true
}

function closeKvEditor() {
  kvEditing.value = false
  kvMsg.value = ''
}

async function saveKv() {
  if (!kvForm.value.key.trim()) {
    kvMsg.value = '名称不能为空'
    kvMsgType.value = 'err'
    return
  }
  if (!kvForm.value.value.trim()) {
    kvMsg.value = '内容不能为空'
    kvMsgType.value = 'err'
    return
  }
  kvSaving.value = true
  kvMsg.value = ''
  try {
    const r = await api.memory.save(
      kvForm.value.key.trim(),
      kvForm.value.value,
      kvForm.value.namespace.trim() || 'default',
      kvForm.value.note.trim() || null
    )
    if (r.ok) {
      await loadKv()
      closeKvEditor()
    } else {
      kvMsg.value = r.error || '保存失败'
      kvMsgType.value = 'err'
    }
  } catch (e) {
    kvMsg.value = e.message
    kvMsgType.value = 'err'
  } finally {
    kvSaving.value = false
  }
}

async function deleteKv(m) {
  if (!confirm(`确定删除记忆「${m.key}」？`)) return
  try {
    await api.memory.delete(m.key, m.namespace || '')
    await loadKv()
  } catch (e) {
    alert('删除失败: ' + e.message)
  }
}

// ---- Notes ----
const notes = ref([])
const notesCount = ref(0)
const notesLoading = ref(false)
const noteViewing = ref(null)
const noteLoading = ref(false)
const noteContent = ref('')

const renderedNote = computed(() => {
  if (!noteContent.value) return '<p class="md-empty">无内容</p>'
  try {
    return marked.parse(noteContent.value, { breaks: true, gfm: true })
  } catch {
    return '<pre>' + noteContent.value.replace(/&/g, '&amp;').replace(/</g, '&lt;') + '</pre>'
  }
})

async function loadNotes() {
  notesLoading.value = true
  try {
    const r = await api.notes.list()
    if (r.ok) {
      notes.value = r.notes || []
      notesCount.value = notes.value.length
    }
  } catch (e) {
    console.error('load notes:', e)
  } finally {
    notesLoading.value = false
  }
}

async function readNote(n) {
  noteViewing.value = n
  noteLoading.value = true
  noteContent.value = ''
  try {
    const r = await api.notes.read(n.name)
    if (r.ok) {
      noteContent.value = r.content || ''
    } else {
      noteContent.value = '读取失败: ' + (r.error || '')
    }
  } catch (e) {
    noteContent.value = '读取失败: ' + e.message
  } finally {
    noteLoading.value = false
  }
}

async function deleteNote(n) {
  if (!confirm(`确定删除笔记「${n.name}」？`)) return
  try {
    await api.notes.delete(n.name)
    if (noteViewing.value?.name === n.name) noteViewing.value = null
    await loadNotes()
  } catch (e) {
    alert('删除失败: ' + e.message)
  }
}

// ---- Utils ----
function switchTab(t) {
  tab.value = t
  if (t === 'kv' && kvList.value.length === 0) loadKv()
  if (t === 'notes' && notes.value.length === 0) loadNotes()
}

function formatTime(ts) {
  if (!ts) return ''
  const n = Number(ts)
  if (!n) return ts
  const d = new Date(n * 1000)
  const pad = (x) => String(x).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

function formatSize(bytes) {
  if (bytes < 1024) return bytes + ' B'
  if (bytes < 1024 * 1024) return (bytes / 1024).toFixed(1) + ' KB'
  return (bytes / 1024 / 1024).toFixed(1) + ' MB'
}

onMounted(() => {
  loadKv()
})
</script>

<style scoped>
/* Tab bar */
.tab-bar {
  display: flex;
  gap: 4px;
  margin-bottom: 14px;
  border-bottom: 1px solid var(--border);
  padding-bottom: 0;
}
.tab-btn {
  background: none;
  border: none;
  color: var(--muted);
  font-size: 13px;
  font-weight: 500;
  padding: 8px 16px;
  border-radius: 6px 6px 0 0;
  cursor: pointer;
  border-bottom: 2px solid transparent;
  transition: all .2s;
}
.tab-btn:hover { color: var(--fg); background: var(--bg-card); }
.tab-btn.active {
  color: var(--accent);
  border-bottom-color: var(--accent);
  background: var(--bg-card);
}
.tab-count {
  display: inline-block;
  margin-left: 4px;
  font-size: 11px;
  color: var(--muted);
  background: var(--bg);
  padding: 1px 6px;
  border-radius: 10px;
}

/* KV memory cards */
.kv-list { display: flex; flex-direction: column; gap: 8px; }
.kv-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px 14px;
  transition: border-color .2s;
}
.kv-card:hover { border-color: var(--accent-dim); }
.kv-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.kv-key {
  font-weight: 600;
  font-size: 14px;
  color: var(--accent);
}
.kv-time {
  font-size: 11px;
  color: var(--muted);
  margin-left: auto;
}
.kv-actions {
  display: flex;
  gap: 4px;
}
.kv-value {
  margin-top: 6px;
  font-size: 13px;
  line-height: 1.6;
  color: var(--fg);
  white-space: pre-wrap;
  word-break: break-word;
}
.kv-note {
  margin-top: 4px;
  font-size: 12px;
}

/* Note cards */
.notes-list { display: flex; flex-direction: column; gap: 8px; }
.note-card {
  background: var(--bg-card);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 12px 14px;
  transition: border-color .2s;
  cursor: default;
}
.note-card:hover { border-color: var(--accent-dim); }
.note-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.note-title {
  font-weight: 600;
  font-size: 14px;
  color: var(--fg);
}
.note-time {
  font-size: 11px;
  color: var(--muted);
  margin-left: auto;
}
.note-actions {
  display: flex;
  gap: 4px;
}
.note-preview {
  margin-top: 6px;
  font-size: 12px;
  color: var(--muted);
  line-height: 1.5;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}

/* Modal */
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,.5);
  z-index: 300;
  display: flex;
  align-items: center;
  justify-content: center;
}
.modal-box {
  background: var(--bg-elev);
  border: 1px solid var(--border);
  border-radius: 10px;
  width: 460px;
  max-width: 90vw;
  max-height: 80vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 12px 40px rgba(0,0,0,.4);
}
.modal-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border);
  font-weight: 600;
  font-size: 14px;
}
.modal-body {
  padding: 16px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.modal-label {
  font-size: 12px;
  color: var(--muted);
  margin-top: 4px;
}
.modal-foot {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  padding: 12px 16px;
  border-top: 1px solid var(--border);
}
textarea.input {
  resize: vertical;
  font-family: inherit;
}

/* Note viewer drawer (reuse md-drawer styles) */
.md-drawer {
  position: fixed;
  top: 0; right: 0;
  width: 560px;
  max-width: 90vw;
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
.md-title { display: flex; align-items: center; gap: 8px; overflow: hidden; }
.md-name {
  font-size: 13px; font-weight: 500;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.md-actions { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }
.md-close {
  background: none; border: none;
  color: var(--muted); font-size: 16px;
  cursor: pointer; padding: 2px 6px; border-radius: 4px;
}
.md-close:hover { color: var(--fg); background: var(--bg-card); }
.md-loading { padding: 40px; text-align: center; color: var(--muted); }
.md-body {
  flex: 1; overflow-y: auto;
  padding: 16px 20px;
  font-size: 13px; line-height: 1.7;
  color: var(--fg);
}
.md-msg { font-size: 11px; color: var(--ok); }
.md-msg.err { color: var(--err); }

.ext-badge {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 34px; padding: 1px 6px; border-radius: 4px;
  font-size: 10px; font-weight: 700; text-transform: uppercase;
  letter-spacing: .5px; font-family: 'JetBrains Mono', 'Consolas', monospace;
  flex-shrink: 0;
}
.ext-md { background: rgba(96,165,250,.14); color: #60a5fa; }

/* Transitions */
.slide-enter-active, .slide-leave-active { transition: transform .2s ease; }
.slide-enter-from, .slide-leave-to { transform: translateX(100%); }
.fade-enter-active, .fade-leave-active { transition: opacity .15s; }
.fade-enter-from, .fade-leave-to { opacity: 0; }
</style>
