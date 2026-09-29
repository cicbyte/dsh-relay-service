import axios, {
  type AxiosInstance,
  type AxiosRequestConfig,
  type AxiosResponse,
  type AxiosError,
} from 'axios'
import { message } from 'ant-design-vue'

declare module 'axios' {
  export interface AxiosRequestConfig {
    /** 请求失败时不弹全局错误提示 */
    skipErrorToast?: boolean
  }
}

/**
 * 统一请求封装（对齐 byte-admin utils/request）：
 * - 自动附加 Authorization: Bearer <accessToken>
 * - 归一后端响应 {code, result, message}：成功返回 result
 * - 401 无感刷新：refreshToken 调 /api/auth/refresh 后重放原请求（一次）；
 *   并发 401 共享同一次刷新（单飞），防 nonce 轮转语义下互相踢出
 */

const TOKEN_KEY = 'dsh-relay.accessToken'
const REFRESH_KEY = 'dsh-relay.refreshToken'
const USER_KEY = 'dsh-relay.username'

export { TOKEN_KEY, REFRESH_KEY, USER_KEY }

export const tokenStore = {
  get access() {
    return localStorage.getItem(TOKEN_KEY) ?? ''
  },
  get refresh() {
    return localStorage.getItem(REFRESH_KEY) ?? ''
  },
  get username() {
    return localStorage.getItem(USER_KEY) ?? ''
  },
  save(accessToken: string, refreshToken: string, username: string) {
    localStorage.setItem(TOKEN_KEY, accessToken)
    localStorage.setItem(REFRESH_KEY, refreshToken)
    localStorage.setItem(USER_KEY, username)
  },
  clear() {
    localStorage.removeItem(TOKEN_KEY)
    localStorage.removeItem(REFRESH_KEY)
    localStorage.removeItem(USER_KEY)
  },
}

interface ApiResp<T = unknown> {
  code: number
  result?: T
  message: string
}

function forceLogout() {
  if (loggingOut) return // 并发 401 只登出一次
  loggingOut = true
  tokenStore.clear()
  if (!location.pathname.startsWith('/login')) {
    message.warning('登录已过期，请重新登录')
    location.href = '/login'
  }
}
let loggingOut = false

/** 登录/刷新自身的 401 是「凭据错误」而非「会话过期」：不走无感刷新与登出跳转，交调用方报错。 */
const AUTH_PATHS = ['/api/auth/login', '/api/auth/refresh']
const isAuthPath = (url = '') => AUTH_PATHS.some((p) => url.includes(p))

// ---------------------------------------------------------------------------
// 401 无感刷新（单飞）
// ---------------------------------------------------------------------------

let refreshing: Promise<string> | null = null

function refreshAccessToken(): Promise<string> {
  if (!refreshing) {
    refreshing = (async () => {
      const rt = tokenStore.refresh
      if (!rt) throw new Error('无刷新令牌')
      // 裸 axios：不走 http 实例，避免拦截器递归
      const { data } = await axios.post<ApiResp<{ accessToken: string; refreshToken: string }>>(
        '/api/auth/refresh',
        { refreshToken: rt },
      )
      if (data.code !== 200 || !data.result?.accessToken) {
        throw new Error(data.message || '刷新失败')
      }
      tokenStore.save(data.result.accessToken, data.result.refreshToken, tokenStore.username)
      return data.result.accessToken
    })().finally(() => {
      refreshing = null
    })
  }
  return refreshing
}

async function retryWithRefresh(config: AxiosRequestConfig): Promise<AxiosResponse> {
  const retried = (config.headers as Record<string, unknown> | undefined)?.['X-Retried']
  if (!tokenStore.refresh || retried) {
    forceLogout()
    return Promise.reject(new Error('登录已过期'))
  }
  let accessToken: string
  try {
    accessToken = await refreshAccessToken()
  } catch {
    // refresh 也过期/被吊销：整体登出并跳登录页（否则页面原地卡死）
    forceLogout()
    return Promise.reject(new Error('登录已过期'))
  }
  const headers = Object.assign({}, config.headers, {
    Authorization: `Bearer ${accessToken}`,
    'X-Retried': '1',
  })
  return http.request({ ...config, headers })
}

// ---------------------------------------------------------------------------
// 请求实例
// ---------------------------------------------------------------------------

const http: AxiosInstance = axios.create({
  baseURL: '/',
  timeout: 15000,
})

http.interceptors.request.use((config) => {
  const token = tokenStore.access
  if (token && !config.headers.Authorization) {
    config.headers.Authorization = `Bearer ${token}`
  }
  return config
})

http.interceptors.response.use(
  (response) => {
    const config = response.config
    const body = response.data as ApiResp
    if (response.status !== 200 || typeof body?.code !== 'number') {
      if (!config.skipErrorToast) message.error('响应异常')
      return Promise.reject(new Error('响应异常'))
    }
    if (body.code === 200) return body.result as unknown as AxiosResponse
    if (body.code === 401) {
      if (isAuthPath(config.url)) return Promise.reject(new Error(body.message || '登录失败'))
      return retryWithRefresh(config)
    }
    if (!config.skipErrorToast) message.error(body.message || `请求失败 ${body.code}`)
    return Promise.reject(new Error(body.message || `请求失败 ${body.code}`))
  },
  (error: AxiosError<ApiResp>) => {
    const config = error.config as AxiosRequestConfig & { skipErrorToast?: boolean }
    if (error.response?.status === 401 && config && !isAuthPath(config.url)) {
      return retryWithRefresh(config)
    }
    if (config && isAuthPath(config.url)) {
      // 登录接口错误由登录页自行展示（防双弹）
      return Promise.reject(new Error(error.response?.data?.message || '登录失败'))
    }
    if (!config?.skipErrorToast) {
      message.error(error.response?.data?.message || error.message || '网络异常')
    }
    return Promise.reject(error)
  },
)

export function request<T = unknown>(config: AxiosRequestConfig): Promise<T> {
  return http.request(config) as Promise<T>
}

export default http
