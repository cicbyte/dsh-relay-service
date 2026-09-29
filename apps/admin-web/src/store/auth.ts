import { defineStore } from 'pinia'
import { ref } from 'vue'
import { login as apiLogin, doLogout } from '@/api/auth'
import { tokenStore } from '@/utils/request'

/** 管理端会话（token 持久在 localStorage，见 utils/request.tokenStore） */
export const useAuthStore = defineStore('auth', () => {
  const username = ref(tokenStore.username)
  const loggedIn = ref(!!tokenStore.access)

  async function login(user: string, password: string) {
    const out = await apiLogin(user, password)
    tokenStore.save(out.accessToken, out.refreshToken, out.username)
    username.value = out.username
    loggedIn.value = true
  }

  async function logout() {
    await doLogout()
    username.value = ''
    loggedIn.value = false
  }

  return { username, loggedIn, login, logout }
})
