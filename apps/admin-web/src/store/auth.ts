import { defineStore } from 'pinia'
import { ref } from 'vue'
import { login as apiLogin, doLogout, getProfile } from '@/api/auth'
import { tokenStore } from '@/utils/request'

/** 管理端会话（token 持久在 localStorage，见 utils/request.tokenStore） */
export const useAuthStore = defineStore('auth', () => {
  const username = ref(tokenStore.username)
  const role = ref(tokenStore.role)
  const loggedIn = ref(!!tokenStore.access)

  /** 启动补全角色（旧版本升级会话无 role 字段；以 profile 为准） */
  async function hydrate() {
    if (!tokenStore.access) return
    try {
      const p = await getProfile()
      tokenStore.save(tokenStore.access, tokenStore.refresh, p.username, p.role)
      username.value = p.username
      role.value = p.role
      loggedIn.value = true
    } catch {
      // 401 由请求层统一走登出/刷新
    }
  }

  async function login(user: string, password: string) {
    const out = await apiLogin(user, password)
    tokenStore.save(out.accessToken, out.refreshToken, out.username, out.role)
    username.value = out.username
    role.value = out.role
    loggedIn.value = true
  }

  async function logout() {
    await doLogout()
    username.value = ''
    role.value = ''
    loggedIn.value = false
  }

  return { username, role, loggedIn, login, logout, hydrate }
})
