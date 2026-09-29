import { createRouter, createWebHistory } from 'vue-router'
import type { RouteRecordRaw } from 'vue-router'
import { tokenStore } from '@/utils/request'

const routes: RouteRecordRaw[] = [
  {
    path: '/login',
    name: 'login',
    component: () => import('@/views/login/index.vue'),
    meta: { title: '登录' },
  },
  {
    path: '/redirect/:path(.*)',
    name: 'redirect',
    component: () => import('@/views/redirect/index.vue'),
    meta: { title: '刷新' },
  },
  {
    path: '/',
    component: () => import('@/layouts/BasicLayout.vue'),
    redirect: '/dashboard',
    children: [
      {
        path: 'dashboard',
        name: 'dashboard',
        component: () => import('@/views/dashboard/index.vue'),
        meta: { title: '概览', icon: 'DashboardOutlined' },
      },
      {
        path: 'devices',
        name: 'devices',
        component: () => import('@/views/devices/index.vue'),
        meta: { title: '设备管理', parent: '通道管理', icon: 'MobileOutlined' },
      },
      {
        path: 'pairing',
        name: 'pairing',
        component: () => import('@/views/pairing/index.vue'),
        meta: { title: '配对码', parent: '通道管理', icon: 'KeyOutlined' },
      },
      {
        path: 'audit',
        name: 'audit',
        component: () => import('@/views/audit/index.vue'),
        meta: { title: '审计日志', parent: '系统管理', icon: 'FileSearchOutlined' },
      },
    ],
  },
  { path: '/:pathMatch(.*)*', redirect: '/dashboard' },
]

const router = createRouter({
  history: createWebHistory(),
  routes,
})

router.beforeEach((to) => {
  document.title = `${String(to.meta.title ?? '管理台')} · dsh-relay`
  if (to.path !== '/login' && !tokenStore.access) {
    return { path: '/login', query: { redirect: to.fullPath } }
  }
  return true
})

export default router
