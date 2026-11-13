<template>
  <div class="app-shell">
    <aside class="sidebar">
      <div class="brand">
        <svg width="22" height="22" viewBox="0 0 24 24" fill="none">
          <rect x="3" y="4" width="18" height="3" rx="1" fill="#f59e0b"/>
          <rect x="3" y="9" width="18" height="3" rx="1" fill="#f59e0b" opacity=".7"/>
          <rect x="3" y="14" width="18" height="3" rx="1" fill="#2a3038"/>
          <circle cx="19" cy="18.5" r="2.5" fill="#f59e0b"/>
        </svg>
        Memex
      </div>
      <router-link v-for="n in nav" :key="n.to" :to="n.to" class="nav-item">
        <span>{{ n.icon }}</span> {{ n.label }}
        <span class="nav-desc">{{ n.desc }}</span>
      </router-link>
      <div class="nav-spacer" />
    </aside>
    <main class="main">
      <h1 class="page-title">{{ title }}</h1>
      <router-view />
    </main>
    <!-- Background task progress bar (global, bottom-fixed) -->
    <div v-if="taskBar.visible" class="ocr-progress-bar">
      <div class="ocr-progress-inner">
        <span class="ocr-icon">{{ taskBar.icon }}</span>
        <span class="ocr-label">{{ taskBar.label }}</span>
        <span class="ocr-counts">{{ taskBar.done }}/{{ taskBar.total }}</span>
        <div class="ocr-track">
          <div class="ocr-fill" :style="{ width: taskBar.percent + '%' }"></div>
        </div>
        <span class="ocr-percent">{{ taskBar.percent }}%</span>
      </div>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, reactive } from 'vue'
import { useRoute } from 'vue-router'
import { api } from './api.js'

const route = useRoute()
const nav = [
  { to: '/search', icon: '🔍', label: '全局搜索', desc: '全文检索知识库文件' },
  { to: '/classify', icon: '🏷️', label: '多维分类', desc: '生成分类 Prompt、浏览标签' },
  { to: '/stats', icon: '📊', label: '统计面板', desc: '索引概况、重复清理、可信度' },
  { to: '/memories', icon: '🧠', label: '智能体记忆', desc: '智能体存储的记忆与笔记' },
  { to: '/settings', icon: '⚙️', label: '设置', desc: '文件源、过滤规则、MCP 配置' }
]
const title = computed(() => route.meta.title ?? '')

// ---- Global background task progress (bottom bar) ----
const taskBar = reactive({
  visible: false,
  icon: '🔍',
  label: '',
  done: 0,
  total: 0,
  percent: 0,
})
let progressTimer = null

async function pollProgress() {
  try {
    const p = await api.index.progress()
    const phase = p.phase || 0
    const ocrTotal = p.ocr_total || 0
    const ocrDone = p.ocr_done || 0
    const contentTotal = p.content_total || 0
    const contentDone = p.content_done || 0
    const embedInProgress = p.embed_in_progress || false
    const embedTotal = p.embed_total || 0
    const embedDone = p.embed_done || 0

    // Phase 3: embedding (highest priority — runs after indexing)
    if (embedInProgress) {
      taskBar.icon = '🧠'
      taskBar.label = '语义向量生成'
      taskBar.done = embedDone
      taskBar.total = embedTotal
      taskBar.percent = p.embed_percent || 0
      taskBar.visible = true
    } else if (phase === 2 && ocrTotal > 0) {
      // OCR in progress — show OCR-specific bar
      taskBar.icon = '🔍'
      taskBar.label = 'OCR 图片识别'
      taskBar.done = ocrDone
      taskBar.total = ocrTotal
      taskBar.percent = p.ocr_percent || 0
      taskBar.visible = true
    } else if (phase === 2 && contentTotal > 0) {
      // Content indexing (non-image files) — show content bar
      taskBar.icon = '📄'
      taskBar.label = '内容索引'
      taskBar.done = contentDone
      taskBar.total = contentTotal
      taskBar.percent = p.content_percent || 0
      taskBar.visible = true
    } else if (phase === 1) {
      // Title scanning phase
      taskBar.icon = '📁'
      taskBar.label = '文件扫描'
      taskBar.done = p.done || 0
      taskBar.total = p.total || 0
      taskBar.percent = p.percent || 0
      taskBar.visible = true
    } else {
      taskBar.visible = false
    }
  } catch (_) {
    // serve not running — silently hide
    taskBar.visible = false
  }
}

onMounted(() => {
  progressTimer = setInterval(pollProgress, 1500)
  pollProgress()
})
onUnmounted(() => {
  if (progressTimer) { clearInterval(progressTimer); progressTimer = null }
})
</script>

<style scoped>
.ocr-progress-bar {
  position: fixed;
  bottom: 0;
  left: 0;
  right: 0;
  height: 36px;
  background: var(--bg-elevated, #1e2228);
  border-top: 1px solid var(--border, #333);
  display: flex;
  align-items: center;
  padding: 0 16px;
  z-index: 1000;
  font-size: 12px;
}
.ocr-progress-inner {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  max-width: 1200px;
  margin: 0 auto;
}
.ocr-icon { font-size: 14px; }
.ocr-label { color: var(--text-secondary, #aaa); white-space: nowrap; }
.ocr-counts {
  font-variant-numeric: tabular-nums;
  color: var(--text-primary, #e0e0e0);
  white-space: nowrap;
}
.ocr-track {
  flex: 1;
  height: 6px;
  background: var(--border, #333);
  border-radius: 3px;
  overflow: hidden;
  min-width: 100px;
}
.ocr-fill {
  height: 100%;
  background: var(--accent, #f59e0b);
  transition: width 0.4s ease;
}
.ocr-percent {
  font-variant-numeric: tabular-nums;
  color: var(--text-secondary, #aaa);
  width: 36px;
  text-align: right;
}
</style>
