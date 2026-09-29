<script setup lang="ts">
// 外观设置抽屉（对齐 byte-admin SettingsDrawer 精简版）：
// 主题模式 / 主题色 / 圆角 / 紧凑密度 / 侧栏跟随主题色 / 恢复默认
import { computed } from 'vue'
import { useAppStore, PRESET_COLORS, RADIUS_OPTIONS, type ThemeMode } from '@/store/app'

const open = defineModel<boolean>('open', { default: false })

const app = useAppStore()

const mode = computed({
  get: () => app.themeMode,
  set: (v: ThemeMode) => app.setThemeMode(v),
})

const radius = computed({
  get: () => app.radius,
  set: (v: number) => app.setRadius(v),
})

const compact = computed({
  get: () => app.compact,
  set: (v: boolean) => app.setCompact(v),
})

const siderFollow = computed({
  get: () => app.siderFollowPrimary,
  set: (v: boolean) => app.setSiderFollowPrimary(v),
})

const MODE_OPTIONS: { value: ThemeMode; label: string }[] = [
  { value: 'light', label: '亮色' },
  { value: 'dark', label: '暗色' },
  { value: 'auto', label: '跟随系统' },
]
</script>

<template>
  <a-drawer v-model:open="open" title="外观设置" :width="300" placement="right">
    <div class="setting-block">
      <div class="setting-title">主题模式</div>
      <a-segmented v-model:value="mode" block :options="MODE_OPTIONS" />
    </div>

    <div class="setting-block">
      <div class="setting-title">主题色</div>
      <div class="color-grid">
        <span
          v-for="c in PRESET_COLORS"
          :key="c.value"
          class="color-dot"
          :class="{ active: app.primaryColor === c.value }"
          :style="{ background: c.value }"
          :title="c.name"
          @click="app.setPrimaryColor(c.value)"
        />
      </div>
    </div>

    <div class="setting-block">
      <div class="setting-title">圆角</div>
      <a-segmented
        v-model:value="radius"
        block
        :options="RADIUS_OPTIONS.map((r) => ({ value: r, label: `${r}px` }))"
      />
    </div>

    <a-divider />

    <div class="setting-row">
      <span>紧凑密度</span>
      <a-switch v-model:checked="compact" />
    </div>
    <div class="setting-row">
      <span>侧栏跟随主题色</span>
      <a-switch v-model:checked="siderFollow" />
    </div>

    <a-divider />

    <a-button block @click="app.resetAppearance()">恢复默认</a-button>
  </a-drawer>
</template>

<style scoped>
.setting-block {
  margin-bottom: 20px;
}

.setting-title {
  color: var(--app-text-secondary);
  font-size: 13px;
  margin-bottom: 10px;
}

.color-grid {
  display: flex;
  flex-wrap: wrap;
  gap: 10px;
}

.color-dot {
  width: 24px;
  height: 24px;
  border-radius: 50%;
  cursor: pointer;
  border: 2px solid transparent;
  transition: transform 0.15s;
}

.color-dot:hover {
  transform: scale(1.15);
}

.color-dot.active {
  border-color: var(--app-text);
}

.setting-row {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 10px 0;
  color: var(--app-text);
  font-size: 14px;
}
</style>
