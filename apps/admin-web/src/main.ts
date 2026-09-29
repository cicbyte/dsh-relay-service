import { createApp } from 'vue'
import { createPinia } from 'pinia'
import dayjs from 'dayjs'
import 'dayjs/locale/zh-cn'
import Antd from 'ant-design-vue'
import 'ant-design-vue/dist/reset.css'
import App from './App.vue'
import router from './router'

dayjs.locale('zh-cn')

const app = createApp(App)
app.config.errorHandler = (err, _inst, info) => {
  console.error('[global]', info, err)
}
window.addEventListener('unhandledrejection', (e) => {
  console.error('[unhandledrejection]', e.reason)
})
app.use(createPinia())
app.use(router)
app.use(Antd)
app.mount('#app')
