import { createRouter, createWebHistory } from 'vue-router'
import type { RouteRecordRaw } from 'vue-router'
import { tokenStore } from '@/utils/request'
import { getProfile } from '@/api/auth'

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
        path: 'audit',
        name: 'audit',
        component: () => import('@/views/audit/index.vue'),
        meta: { title: '审计日志', parent: '系统管理', icon: 'FileSearchOutlined' },
      },
      {
        path: 'users',
        name: 'users',
        component: () => import('@/views/users/index.vue'),
        meta: { title: '用户管理', parent: '系统管理', icon: 'TeamOutlined', admin: true },
      },
    ],
  },
  { path: '/:pathMatch(.*)*', redirect: '/dashboard' },
]

const router = createRouter({
  history: createWebHistory(),
  routes,
})

router.beforeEach(async (to) => {
  document.title = `${String(to.meta.title ?? '管理台')} · dsh-relay`
  if (to.path !== '/login' && !tokenStore.access) {
    return { path: '/login', query: { redirect: to.fullPath } }
  }
  // 角色守卫（展示裁剪；真实权限在后端写死）。旧版本会话无 role：先补全再判
  if (to.meta.admin && tokenStore.role !== 'admin') {
    if (tokenStore.access && !tokenStore.role) {
      try {
        const p = await getProfile()
        tokenStore.save(tokenStore.access, tokenStore.refresh, p.username, p.role)
      } catch {
        // 401 由请求层统一处理
      }
    }
    if (tokenStore.role !== 'admin') {
      return { path: '/dashboard' }
    }
  }
  return true
})

export default router
