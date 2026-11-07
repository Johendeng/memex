<template>
  <button class="help-fab" @click="startTour" title="使用帮助">
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1 .9-1 1.6v.3"/><circle cx="12" cy="17" r=".5" fill="currentColor"/></svg>
  </button>
  <Teleport to="body">
    <div v-if="tourActive" class="tour-backdrop" @click="endTour"></div>
    <div v-if="tourActive" class="tour-spotlight" :style="spotlightStyle"></div>
    <div v-if="tourActive" class="tour-callout" :style="calloutStyle" @click.stop>
      <div class="tour-callout-head">
        <span class="tour-badge">{{ tourStep + 1 }}/{{ steps.length }}</span>
        <span class="tour-callout-title">{{ steps[tourStep]?.title }}</span>
      </div>
      <div class="tour-callout-body">{{ steps[tourStep]?.text }}</div>
      <div class="tour-callout-actions">
        <button class="btn ghost sm" @click="endTour">跳过</button>
        <div class="row" style="gap:6px">
          <button v-if="tourStep > 0" class="btn ghost sm" @click="prevStep">‹ 上一步</button>
          <button class="btn sm" @click="nextStep">{{ tourStep < steps.length - 1 ? '下一步 ›' : '完成' }}</button>
        </div>
      </div>
    </div>
  </Teleport>
</template>

<script setup>
import { ref, computed, nextTick } from 'vue'

const props = defineProps({
  steps: { type: Array, required: true },
  beforeStart: { type: Function, default: null }
})

const tourActive = ref(false)
const tourStep = ref(0)
const spotRect = ref({ top: 0, left: 0, w: 0, h: 0 })

const spotlightStyle = computed(() => {
  const r = spotRect.value
  return { top: r.top + 'px', left: r.left + 'px', width: r.w + 'px', height: r.h + 'px' }
})

const calloutStyle = computed(() => {
  const r = spotRect.value
  const cw = 380, ch = 160
  let top = r.top + r.h + 12
  if (top + ch > window.innerHeight) top = Math.max(16, r.top - ch - 12)
  let left = r.left
  if (left + cw > window.innerWidth) left = window.innerWidth - cw - 16
  if (left < 16) left = 16
  return { top: top + 'px', left: left + 'px', width: cw + 'px' }
})

function startTour() {
  if (props.beforeStart) props.beforeStart()
  tourActive.value = true
  tourStep.value = 0
  nextTick(() => setTimeout(() => updateSpotlight(), 100))
}
function endTour() { tourActive.value = false }
function nextStep() {
  if (tourStep.value < props.steps.length - 1) {
    tourStep.value++
    updateSpotlight()
  } else {
    endTour()
  }
}
function prevStep() {
  if (tourStep.value > 0) { tourStep.value--; updateSpotlight() }
}
function updateSpotlight() {
  const step = props.steps[tourStep.value]
  if (!step || !step.selector) return
  const el = document.querySelector(step.selector)
  if (!el) return
  el.scrollIntoView({ behavior: 'smooth', block: 'center' })
  setTimeout(() => {
    const rect = el.getBoundingClientRect()
    spotRect.value = { top: rect.top - 8, left: rect.left - 8, w: rect.width + 16, h: rect.height + 16 }
  }, 350)
}

defineExpose({ startTour })
</script>
