import { createApp } from 'vue'
import { createRouter, createWebHashHistory } from 'vue-router'
import App from './App.vue'
import './style.css'

const routes = [
  { path: '/', redirect: '/search' },
  { path: '/search', name: 'search', component: () => import('./views/Search.vue'), meta: { title: '全局搜索' } },
  { path: '/classify', name: 'classify', component: () => import('./views/Classification.vue'), meta: { title: '多维分类' } },
  { path: '/stats', name: 'stats', component: () => import('./views/Stats.vue'), meta: { title: '统计面板' } },
  { path: '/memories', name: 'memories', component: () => import('./views/Memories.vue'), meta: { title: '智能体记忆' } },
  { path: '/settings', name: 'settings', component: () => import('./views/Settings.vue'), meta: { title: '设置' } }
]

const router = createRouter({
  history: createWebHashHistory(),
  routes
})

createApp(App).use(router).mount('#app')
