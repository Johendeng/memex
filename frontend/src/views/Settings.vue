<template>
  <div class="col">
    <HelpTour :steps="tourSteps" />

    <!-- Agent 介绍 Prompt — 置顶 -->
    <div class="card" id="set-agent-prompt" style="display:flex;align-items:center;justify-content:space-between;gap:12px">
      <div>
        <span class="card-title">AI 智能体接入</span>
        <p class="muted" style="font-size:12px;margin:4px 0 0">生成完整的 Agent 介绍 Prompt（能力说明 + MCP 注册 + 调用时机），一键复制给智能体。</p>
      </div>
      <button class="btn" @click="genAgentPrompt" :disabled="agentPromptLoading" style="white-space:nowrap;flex-shrink:0">
        {{ agentPromptLoading ? '生成中…' : '生成 Agent 介绍 Prompt' }}
      </button>
    </div>

    <!-- Agent Prompt 弹窗（全局） -->
    <div v-if="agentPromptModal" class="modal-overlay" @click.self="agentPromptModal = false">
      <div class="modal-box" style="max-width:800px">
        <div class="row between" style="margin-bottom:12px">
          <h3 style="margin:0">Agent 介绍 Prompt</h3>
          <div class="row" style="gap:8px">
            <button class="btn ghost" style="padding:4px 12px;font-size:12px" @click="copyAgentPrompt">
              {{ agentPromptCopied ? '已复制 ✓' : '复制到剪贴板' }}
            </button>
            <button class="btn ghost" style="padding:4px 12px;font-size:12px" @click="agentPromptModal = false">关闭</button>
          </div>
        </div>
        <textarea
          v-model="agentPromptText"
          rows="28"
          class="input mono"
          style="font-size:12px;resize:vertical;line-height:1.6"
          readonly
        ></textarea>
      </div>
    </div>

    <FolderSources />

    <div class="card" id="set-filter">
      <span class="card-title">文件过滤(全局)</span>

      <label class="muted" style="font-size:12px">包含扩展名</label>
      <div class="row" style="flex-wrap:wrap;margin:6px 0 14px">
        <span v-for="e in exts" :key="e" class="tag" :class="{on:rules.include_exts.includes(e)}"
          @click="toggle(rules.include_exts, e)" style="cursor:pointer">{{ e }}</span>
      </div>

      <label class="muted" style="font-size:12px">排除扩展名</label>
      <div class="row" style="flex-wrap:wrap;margin:6px 0 14px">
        <span v-for="e in exclExts" :key="e" class="tag" :class="{on:rules.exclude_exts.includes(e)}"
          @click="toggle(rules.exclude_exts, e)" style="cursor:pointer">{{ e }}</span>
      </div>

      <label class="muted" style="font-size:12px">大小上限:{{ (rules.max_bytes / 1048576).toFixed(0) }} MB</label>
      <input type="range" min="1" max="500" v-model.number="mb" @change="rules.max_bytes = mb * 1048576"
        style="width:100%;margin:6px 0 6px" />
      <div class="row" style="gap:6px;margin-bottom:14px">
        <button v-for="s in [50, 100, 200, 500]" :key="s" class="btn ghost sm" :style="mb === s ? 'border-color:var(--accent);color:var(--accent)' : ''" @click="mb = s; rules.max_bytes = s * 1048576">{{ s }}MB</button>
      </div>

      <label class="muted" style="font-size:12px">glob 排除规则(每行一条)</label>
      <textarea v-model="globs" rows="4" class="input mono" style="margin:6px 0 14px;font-size:12px"></textarea>

      <div class="row" style="flex-wrap:wrap;gap:18px;margin-bottom:12px">
        <label class="row" style="gap:6px"><input type="checkbox" v-model="rules.skip_empty" /> 跳过空文件</label>
        <label class="row" style="gap:6px"><input type="checkbox" v-model="rules.skip_hidden" /> 跳过隐藏文件</label>
        <label class="row" style="gap:6px"><input type="checkbox" v-model="rules.skip_system" /> 跳过系统文件</label>
      </div>

      <div class="row">
        <button class="btn" @click="save">保存规则</button>
        <button class="btn ghost" @click="reset">恢复默认</button>
        <span class="muted" v-if="msg">{{ msg }}</span>
      </div>
    </div>

    <!-- MCP 集成 -->
    <div class="card" id="set-mcp">
      <span class="card-title">MCP 集成</span>
      <p class="muted" style="font-size:12px;margin-bottom:12px;line-height:1.8">
        Memex 通过 MCP 协议向 AI 智能体开放知识库能力（13 个工具：检索/记忆/笔记/管理）。<br/>
        点击下方按钮生成仅 MCP 注册配置（stdio + HTTP），粘贴到智能体即可注册。
      </p>
      <div class="row" style="gap:8px;margin-bottom:12px">
        <button class="btn ghost" @click="genMcpPrompt">仅 MCP 注册配置</button>
      </div>
      <textarea
        v-if="mcpPrompt"
        v-model="mcpPrompt"
        rows="10"
        class="input mono"
        style="font-size:12px;resize:vertical"
        readonly
      ></textarea>
    </div>

    <!-- 本地模型 -->
    <div class="card" id="set-model">
      <span class="card-title">本地模型</span>
      <div class="row" style="gap:18px;flex-wrap:wrap;margin-bottom:12px">
        <div class="model-row">
          <span class="tag on">bge-small-zh-v1.5</span>
          <span class="muted" style="font-size:11px">{{ vecReady ? `${vecCount} 条向量 · ${vecDim} 维` : '未生成向量' }}</span>
          <button class="btn ghost" style="padding:2px 8px;font-size:11px" @click="doEmbed" :disabled="embedding">{{ embedding ? '生成中…' : '生成向量' }}</button>
        </div>
        <div class="model-row">
          <span class="tag on">ONNX Runtime</span>
          <span class="muted" style="font-size:11px">ort load-dynamic</span>
        </div>
      </div>
      <!-- Embed progress bar -->
      <div v-if="embedding" style="margin-bottom:10px">
        <div class="row" style="justify-content:space-between;font-size:12px;margin-bottom:4px">
          <span class="muted">向量生成中</span>
          <span class="muted">{{ embedProgress.done }}/{{ embedProgress.total }} ({{ embedProgress.percent }}%)</span>
        </div>
        <div class="progress-bar"><div class="progress-fill" :style="{width: embedProgress.percent + '%'}"></div></div>
      </div>
      <div v-if="embedMsg" class="row muted" style="font-size:12px;margin-bottom:8px">{{ embedMsg }}</div>
    </div>

    <!-- 索引管理 -->
    <div class="card">
      <span class="card-title">索引管理</span>
      <div class="row" style="gap:8px;align-items:center;margin-bottom:10px">
        <button class="btn" @click="reindexIncremental" :disabled="reindexing">
          {{ reindexing && phase === 1 ? '标题索引中…' : '增量索引' }}
        </button>
        <button class="btn ghost" @click="reindexFull" :disabled="reindexing">
          {{ reindexing && phase === 2 ? '内容解析中…' : '全量重建' }}
        </button>
      </div>
      <!-- Phase 1 progress -->
      <div v-if="phase === 1" style="margin-bottom:10px">
        <div class="row" style="justify-content:space-between;font-size:12px;margin-bottom:4px">
          <span class="muted">Phase 1 · 标题索引</span>
          <span class="muted">{{ progress.done }}/{{ progress.total }} ({{ progress.percent }}%)</span>
        </div>
        <div class="progress-bar"><div class="progress-fill" :style="{width: progress.percent + '%'}"></div></div>
        <div class="row muted mono" v-if="progress.current" style="font-size:11px;margin-top:4px">{{ progress.current }}</div>
      </div>
      <!-- Phase 2 progress -->
      <div v-if="phase === 2" style="margin-bottom:10px">
        <div class="row" style="justify-content:space-between;font-size:12px;margin-bottom:4px">
          <span class="muted">Phase 2 · 内容解析（后台）</span>
          <span class="muted">{{ progress.content_done }}/{{ progress.content_total }} ({{ progress.content_percent }}%)</span>
        </div>
        <div class="progress-bar"><div class="progress-fill" :style="{width: progress.content_percent + '%'}"></div></div>
        <div class="row muted mono" v-if="progress.current" style="font-size:11px;margin-top:4px">{{ progress.current }}</div>
      </div>
      <div class="row muted" v-if="reindexMsg" style="font-size:12px;margin-bottom:10px">
        {{ reindexMsg }}
      </div>
      <div class="row" style="gap:12px;font-size:12px" v-if="idxStats">
        <span class="muted">文件 <b>{{ idxStats.files }}</b></span>
        <span class="muted">chunk <b>{{ idxStats.chunks }}</b></span>
        <span class="muted">bigram <b>{{ idxStats.bigrams }}</b></span>
        <span class="muted">文件夹 <b>{{ idxStats.folders }}</b></span>
      </div>
    </div>

    <!-- 升级检测 -->
    <div class="card" id="set-update">
      <span class="card-title">升级检测</span>
      <div class="row" style="gap:8px;align-items:center;margin-bottom:8px">
        <span class="muted" style="font-size:12px">当前版本 <b>v{{ updateCurrent }}</b></span>
        <button class="btn" @click="doCheckUpdate" :disabled="updateChecking">
          {{ updateChecking ? '检测中…' : '检查更新' }}
        </button>
      </div>

      <!-- Update available banner -->
      <div v-if="updateInfo && updateInfo.has_update" style="margin-bottom:12px;padding:12px;border:1px solid var(--accent,#3b82f6);border-radius:8px">
        <div class="row" style="justify-content:space-between;margin-bottom:6px">
          <span style="font-size:14px">发现新版本 <b>v{{ updateInfo.latest }}</b></span>
          <span class="muted" style="font-size:11px">来源: {{ updateInfo.source }}</span>
        </div>
        <div v-if="updateInfo.notes" class="muted mono" style="font-size:11px;white-space:pre-wrap;max-height:120px;overflow-y:auto;margin-bottom:10px">{{ updateInfo.notes }}</div>
        <div class="row" style="gap:8px">
          <button v-if="!downloading && !updateDownloaded" class="btn" @click="doDownloadUpdate">下载并安装</button>
          <button v-if="updateDownloaded" class="btn" @click="doInstallUpdate">立即安装（将退出程序）</button>
          <span v-if="downloading" class="muted" style="font-size:12px">下载中… {{ dlProgress.percent }}%</span>
        </div>
        <!-- Download progress -->
        <div v-if="downloading" style="margin-top:8px">
          <div class="progress-bar"><div class="progress-fill" :style="{width: dlProgress.percent + '%'}"></div></div>
          <div class="muted" style="font-size:11px;margin-top:4px">{{ formatBytes(dlProgress.done) }} / {{ formatBytes(dlProgress.total) }}</div>
        </div>
      </div>

      <!-- No update / error messages -->
      <div v-if="updateMsg" class="muted" style="font-size:12px">{{ updateMsg }}</div>
    </div>

    <!-- Data Export / Import -->
    <div class="card">
      <h3>📦 数据导出 / 迁移</h3>
      <p class="muted" style="font-size:12px;margin-bottom:12px">导出知识库元数据、标签、置信度和记忆，或导入标签到新环境。</p>
      <div class="row" style="gap:8px;flex-wrap:wrap">
        <button class="btn" @click="exportJSON" :disabled="exporting">{{ exporting ? '导出中…' : '导出 JSON' }}</button>
        <button class="btn" @click="exportCSV" :disabled="exporting">导出 CSV</button>
        <button class="btn" @click="exportMemories" :disabled="exporting">导出记忆</button>
        <label class="btn ghost" style="cursor:pointer">
          导入标签
          <input type="file" accept=".json" @change="importTags" style="display:none" />
        </label>
      </div>
      <div v-if="exportMsg" class="muted" style="font-size:12px;margin-top:8px">{{ exportMsg }}</div>
    </div>
  </div>
</template>

<script setup>
import { ref, watch, onMounted, onUnmounted } from 'vue'
import { api } from '../api.js'
import FolderSources from './FolderSources.vue'
import HelpTour from '../components/HelpTour.vue'

const tourSteps = [
  { selector: '#set-filter', title: '文件过滤规则', text: '设置全局文件过滤：包含/排除扩展名、文件大小上限（预设 50/100/200/500 MB）、glob 排除规则、是否跳过空文件/隐藏文件/系统文件。保存后对新索引生效。' },
  { selector: '#set-mcp', title: 'MCP 集成', text: '点击「生成 MCP 配置 Prompt」一键生成 stdio + HTTP 两种接入方式的配置说明，包含各自优缺点和选择建议。复制后粘贴给 AI 智能体即可注册 memex_search / memex_read / memex_tag / memex_report_confidence 四个工具。' },
  { selector: '#set-model', title: '本地模型', text: '管理本地 ONNX 模型：bge-small-zh-v1.5 向量模型（用于语义检索），生成向量后可开启混合检索模式。向量维度 512，支持增量更新。' },
  { selector: '#set-update', title: '升级检测', text: '检查 Memex 新版本：启动时自动检测一次（后台静默），也可手动点击「检查更新」。发现新版本后可一键下载安装包并自动启动升级，data/ 数据目录不受影响。版本源：Gitee（主）+ GitHub（备）。' },
]

const exts = ['txt','md','markdown','html','htm','csv','json','rst','org','pdf','docx']
const exclExts = ['tmp','log','bak','exe','dll','so']
const rules = ref(defaults())
const mb = ref(50)
const globs = ref('')
const msg = ref('')
const mcpPrompt = ref('')
const mcpCopied = ref(false)
const agentPromptLoading = ref(false)
const agentPromptModal = ref(false)
const agentPromptText = ref('')
const agentPromptCopied = ref(false)

// model/embed status
const vecReady = ref(false)
const vecCount = ref(0)
const vecDim = ref(0)
const embedding = ref(false)
const embedProgress = ref({ done: 0, total: 0, percent: 0 })
const embedMsg = ref('')
let embedTimer = null

async function refreshVectorStatus() {
  try {
    const r = await api.vectorStatus()
    vecReady.value = r.has_vectors
    vecCount.value = r.count
    vecDim.value = r.dim
  } catch (_) {}
}

async function pollEmbedProgress() {
  try {
    const r = await api.embedProgress()
    if (r.in_progress) {
      embedding.value = true
      embedProgress.value = { done: r.done, total: r.total, percent: r.percent }
      embedMsg.value = r.total ? '' : '模型加载中…'
    } else {
      embedding.value = false
      if (embedTimer) { clearInterval(embedTimer); embedTimer = null }
      embedMsg.value = '向量生成完成 ✓'
      await refreshVectorStatus()
      // Auto-clear completion message after 4s
      setTimeout(() => { if (embedMsg.value === '向量生成完成 ✓') embedMsg.value = '' }, 4000)
    }
  } catch (_) {}
}

function startEmbedPolling() {
  if (embedTimer) { clearInterval(embedTimer); embedTimer = null }
  embedTimer = setInterval(pollEmbedProgress, 2000)
}

async function doEmbed() {
  embedding.value = true
  embedMsg.value = ''
  try {
    const r = await api.embed()
    if (r.ok) {
      startEmbedPolling()
    } else {
      embedding.value = false
      embedMsg.value = r.error || '生成向量失败'
    }
  } catch (e) {
    embedding.value = false
    embedMsg.value = '生成向量失败: ' + e.message
  }
}

// index management
const reindexing = ref(false)
const reindexMsg = ref('')
const idxStats = ref(null)
const phase = ref(0)         // 0=idle, 1=title, 2=content
const progress = ref({ done:0, total:0, percent:0, scanned:0, current:'', content_done:0, content_total:0, content_percent:0 })
let progressTimer = null

function defaults() {
  return {
    include_exts: ['txt','md','markdown','html','htm','csv','json','rst','org'],
    exclude_exts: ['tmp','bak','exe','dll','so'],
    max_bytes: 50*1048576,
    exclude_globs: ['**/node_modules/**','**/.git/**','**/~$*','**/Thumbs.db','**/desktop.ini'],
    skip_empty: true, skip_hidden: true, skip_system: true
  }
}
function toggle(arr, e) {
  const i = arr.indexOf(e); if (i >= 0) arr.splice(i,1); else arr.push(e)
}
function syncGlobs() { globs.value = rules.value.exclude_globs.join('\n') }
watch(globs, v => { rules.value.exclude_globs = v.split('\n').map(s => s.trim()).filter(Boolean) })

async function load() {
  try { const r = await api.filter.get(); rules.value = r; mb.value = Math.round(r.max_bytes/1048576); syncGlobs() }
  catch (_) { syncGlobs() }
}

async function save() {
  try { await api.filter.save(rules.value); msg.value = '已保存' }
  catch (e) { msg.value = e.message }
}
function reset() { rules.value = defaults(); mb.value = 50; syncGlobs(); msg.value = '已恢复默认(未保存)' }

async function genMcpPrompt() {
  mcpCopied.value = false
  try {
    const [stdioR, httpR] = await Promise.all([
      api.mcp.prompt(),
      api.mcp.httpPrompt(),
    ])
    mcpPrompt.value = `Memex 知识库 MCP 注册，有两种接入方式，请选择适合的一种（或询问用户）：

## 方式 A — stdio（本地进程）

${stdioR.prompt}
优点：无需额外运行服务，智能体直接启动 memex 进程；延迟低。
缺点：每个智能体独立启动一个进程；需要 memex.exe 路径可访问。

## 方式 B — HTTP（远程服务）

${httpR.prompt}
优点：多智能体共享同一个索引服务；memex serve 常驻后台。
缺点：需要先手动启动 memex serve；多一层网络开销。

## 选择建议

- 单一智能体、本地使用 → 方式 A（stdio）
- 多智能体共享、或 memex serve 已在运行 → 方式 B（HTTP）
- 拿不准 → 询问用户偏好

注册后，智能体即可调用 memex_search / memex_read / memex_tag / memex_report_confidence 四个工具。`
  } catch (e) {
    mcpPrompt.value = '// 生成失败: ' + e.message
  }
}
async function copyMcpPrompt() {
  try {
    await navigator.clipboard.writeText(mcpPrompt.value)
    mcpCopied.value = true
    setTimeout(() => { mcpCopied.value = false }, 2000)
  } catch (_) {}
}

async function genAgentPrompt() {
  agentPromptLoading.value = true
  agentPromptCopied.value = false
  try {
    const r = await api.mcp.agentPrompt()
    agentPromptText.value = r.prompt
    agentPromptModal.value = true
  } catch (e) {
    agentPromptText.value = '// 生成失败: ' + e.message
    agentPromptModal.value = true
  } finally {
    agentPromptLoading.value = false
  }
}

async function copyAgentPrompt() {
  try {
    await navigator.clipboard.writeText(agentPromptText.value)
    agentPromptCopied.value = true
    setTimeout(() => { agentPromptCopied.value = false }, 2000)
  } catch (_) {}
}

// --- (vector status loaded by refreshVectorStatus, embed by doEmbed above) ---

// --- index management ---
async function loadIdxStats() {
  try {
    const r = await api.info()
    idxStats.value = r
  } catch (_) {}
}

function startProgressPoll() {
  stopProgressPoll()
  progressTimer = setInterval(async () => {
    try {
      const p = await api.index.progress()
      phase.value = p.phase || 0
      progress.value = {
        done: p.done || 0,
        total: p.total || 0,
        percent: p.percent || 0,
        scanned: p.scanned || 0,
        current: p.current || '',
        content_done: p.content_done || 0,
        content_total: p.content_total || 0,
        content_percent: p.content_percent || 0,
      }
      // Phase 2 completed → stop polling, reload stats
      if (phase.value === 0 && progress.value.content_done > 0 && progress.value.content_total > 0) {
        stopProgressPoll()
        reindexing.value = false
        reindexMsg.value = '索引完成（含向量嵌入）'
        await loadIdxStats()
        await refreshVectorStatus()
      }
    } catch (_) {}
  }, 1000)
}

function stopProgressPoll() {
  if (progressTimer) { clearInterval(progressTimer); progressTimer = null }
}

async function reindexIncremental() {
  reindexing.value = true
  reindexMsg.value = ''
  phase.value = 1
  try {
    const r = await api.index.run('incremental')
    reindexMsg.value = `标题索引完成：${r.indexed} 新增 / ${r.unchanged} 未变更 / ${r.title_only} 待解析内容`
    await loadIdxStats()
    // Phase 2 runs in background — start polling for progress
    startProgressPoll()
  } catch (e) {
    reindexMsg.value = '失败: ' + e.message
    reindexing.value = false
    phase.value = 0
  }
}

async function reindexFull() {
  reindexing.value = true
  reindexMsg.value = ''
  phase.value = 1
  try {
    const r = await api.index.run('full')
    reindexMsg.value = `全量重建完成：${r.indexed} 文件 / ${r.chunks} chunks`
    phase.value = 0
    reindexing.value = false
    await loadIdxStats()
    await refreshVectorStatus()
  } catch (e) {
    reindexMsg.value = '失败: ' + e.message
    reindexing.value = false
    phase.value = 0
  }
}

// --- update checking ---
const updateCurrent = ref(env_version())
const updateInfo = ref(null)
const updateChecking = ref(false)
const updateMsg = ref('')
const downloading = ref(false)
const updateDownloaded = ref(false)
const dlProgress = ref({ done: 0, total: 0, percent: 0 })
let dlTimer = null

function env_version() {
  // This gets replaced by the backend's CARGO_PKG_VERSION via cmd_info or cmd_check_update
  return '1.0.6'
}

function formatBytes(n) {
  if (!n) return '0 B'
  if (n < 1024) return n + ' B'
  if (n < 1048576) return (n / 1024).toFixed(1) + ' KB'
  return (n / 1048576).toFixed(1) + ' MB'
}

// Exposed globally so the tray menu "检查更新" can trigger it
window.doCheckUpdate = async function() {
  updateChecking.value = true
  updateMsg.value = ''
  try {
    const r = await api.update.check()
    if (r.ok) {
      updateCurrent.value = r.current
      if (r.has_update) {
        updateInfo.value = r
        updateMsg.value = ''
      } else {
        updateInfo.value = null
        updateMsg.value = `当前已是最新版本 v${r.current}`
      }
    } else {
      updateMsg.value = r.error || '检测失败'
    }
  } catch (e) {
    updateMsg.value = '检测失败: ' + e.message
  }
  updateChecking.value = false
}

async function doDownloadUpdate() {
  if (!updateInfo.value || !updateInfo.value.download_url) {
    updateMsg.value = '无下载地址'
    return
  }
  downloading.value = true
  updateDownloaded.value = false
  updateMsg.value = ''
  try {
    const r = await api.update.download(updateInfo.value.download_url, updateInfo.value.installer_filename)
    if (r.ok) {
      // Start polling progress
      dlTimer = setInterval(pollDownloadProgress, 1000)
    } else {
      downloading.value = false
      updateMsg.value = r.error || '下载失败'
    }
  } catch (e) {
    downloading.value = false
    updateMsg.value = '下载失败: ' + e.message
  }
}

async function pollDownloadProgress() {
  try {
    const r = await api.update.progress()
    if (r.in_progress) {
      dlProgress.value = { done: r.done, total: r.total, percent: r.percent }
    } else {
      // Download finished
      if (dlTimer) { clearInterval(dlTimer); dlTimer = null }
      downloading.value = false
      if (r.downloaded) {
        updateDownloaded.value = true
        updateMsg.value = '下载完成，点击「立即安装」开始升级'
      } else {
        updateMsg.value = '下载失败，请重试'
      }
    }
  } catch (_) {}
}

async function doInstallUpdate() {
  try {
    const r = await api.update.install()
    if (r.ok) {
      updateMsg.value = '安装程序已启动，程序即将退出…'
      // The backend will exit after 500ms
    } else {
      updateMsg.value = r.error || '安装失败'
    }
  } catch (e) {
    updateMsg.value = '安装失败: ' + e.message
  }
}

onMounted(() => {
  load()
  refreshVectorStatus()
  loadIdxStats()
  // Check if embedding is already in progress (e.g. started from another tab)
  api.embedProgress().then(r => {
    if (r.in_progress) {
      embedding.value = true
      startEmbedPolling()
    }
  }).catch(() => {})
  // Always do a fresh check on page load (auto-check result may be stale)
  window.doCheckUpdate()
  // Also get version from backend
  api.info().then(r => { if (r.version) updateCurrent.value = r.version }).catch(() => {})
})

onUnmounted(() => {
  stopProgressPoll()
  if (embedTimer) { clearInterval(embedTimer); embedTimer = null }
  if (dlTimer) { clearInterval(dlTimer); dlTimer = null }
})

// ---- Data export / import ----
const exporting = ref(false)
const exportMsg = ref('')

async function downloadFile(filename, content, mime = 'text/plain') {
  const blob = new Blob([content], { type: mime })
  const url = URL.createObjectURL(blob)
  const a = document.createElement('a')
  a.href = url
  a.download = filename
  a.click()
  URL.revokeObjectURL(url)
}

async function exportJSON() {
  exporting.value = true
  exportMsg.value = ''
  try {
    const r = await api.export.json()
    const json = JSON.stringify(r.data, null, 2)
    downloadFile(`memex-export-${Date.now()}.json`, json, 'application/json')
    exportMsg.value = `已导出 ${r.data.files?.length || 0} 个文件、${r.data.tags?.length || 0} 个标签`
  } catch (e) { exportMsg.value = '导出失败：' + e.message }
  finally { exporting.value = false }
}

async function exportCSV() {
  exporting.value = true
  exportMsg.value = ''
  try {
    const r = await api.export.csv()
    downloadFile(`memex-files-${Date.now()}.csv`, r.csv, 'text/csv')
    exportMsg.value = `已导出 ${r.rows} 行 CSV`
  } catch (e) { exportMsg.value = '导出失败：' + e.message }
  finally { exporting.value = false }
}

async function exportMemories() {
  exporting.value = true
  exportMsg.value = ''
  try {
    const r = await api.export.memories()
    const json = JSON.stringify(r.memories, null, 2)
    downloadFile(`memex-memories-${Date.now()}.json`, json, 'application/json')
    exportMsg.value = `已导出 ${r.count} 条记忆`
  } catch (e) { exportMsg.value = '导出失败：' + e.message }
  finally { exporting.value = false }
}

async function importTags(e) {
  const file = e.target.files?.[0]
  if (!file) return
  exportMsg.value = ''
  try {
    const text = await file.text()
    const tags = JSON.parse(text)
    const r = await api.export.importTags(tags)
    exportMsg.value = `已导入 ${r.added} 个标签（共 ${r.total_tags} 个）`
  } catch (err) {
    exportMsg.value = '导入失败：' + err.message
  }
  e.target.value = ''
}
</script>

<style scoped>
.modal-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0,0,0,0.5);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  padding: 20px;
}
.modal-box {
  background: var(--bg, #fff);
  border-radius: 12px;
  padding: 20px;
  width: 100%;
  box-shadow: 0 8px 32px rgba(0,0,0,0.2);
}
.model-row {
  display: flex;
  align-items: center;
  gap: 8px;
}
.progress-bar {
  height: 6px;
  background: var(--border, #e0e0e0);
  border-radius: 3px;
  overflow: hidden;
}
.progress-fill {
  height: 100%;
  background: var(--accent, #3b82f6);
  transition: width 0.3s ease;
}
</style>
