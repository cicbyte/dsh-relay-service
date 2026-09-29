<script setup lang="ts">
// 刷新中转页：/redirect/xxx → /xxx（多页签“刷新当前页”实现）
import { onMounted } from 'vue'
import { useRoute, useRouter } from 'vue-router'

const route = useRoute()
const router = useRouter()

onMounted(() => {
  const path = '/' + String(route.params.path ?? '')
  router.replace({ path, query: route.query, hash: route.hash })
})
</script>

<template>
  <div />
</template>
