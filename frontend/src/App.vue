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
      <div class="nav-item muted" style="cursor:default">v1.0.0</div>
    </aside>
    <main class="main">
      <h1 class="page-title">{{ title }}</h1>
      <router-view />
    </main>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { useRoute } from 'vue-router'
const route = useRoute()
const nav = [
  { to: '/search', icon: '🔍', label: '全局搜索', desc: '全文检索知识库文件' },
  { to: '/classify', icon: '🏷️', label: '多维分类', desc: '生成分类 Prompt、浏览标签' },
  { to: '/stats', icon: '📊', label: '统计面板', desc: '索引概况、重复清理、可信度' },
  { to: '/settings', icon: '⚙️', label: '设置', desc: '文件源、过滤规则、MCP 配置' }
]
const title = computed(() => route.meta.title ?? '')
</script>
