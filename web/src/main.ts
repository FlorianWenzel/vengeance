import { createApp } from 'vue'
import { createPinia } from 'pinia'
import '@fontsource-variable/big-shoulders-display'
import '@fontsource-variable/jetbrains-mono'
import '@fontsource/ibm-plex-sans/400.css'
import '@fontsource/ibm-plex-sans/500.css'
import '@fontsource/ibm-plex-sans/600.css'
import './style.css'
import App from './App.vue'
import { router } from './router'

createApp(App).use(createPinia()).use(router).mount('#app')
