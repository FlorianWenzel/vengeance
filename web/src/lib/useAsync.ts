import { ref, shallowRef, type Ref, type ShallowRef } from 'vue'
import { errorMessage } from './api'

export interface AsyncState<T> {
  data: ShallowRef<T | null>
  loading: Ref<boolean>
  error: Ref<string | null>
  run: () => Promise<void>
}

/** Loading / error / data triple for a page-level fetch. Stale responses are dropped. */
export function useAsync<T>(fn: () => Promise<T>): AsyncState<T> {
  const data = shallowRef<T | null>(null)
  const loading = ref(false)
  const error = ref<string | null>(null)
  let token = 0

  async function run() {
    const mine = ++token
    loading.value = true
    error.value = null
    try {
      const result = await fn()
      if (mine === token) data.value = result
    } catch (e) {
      if (mine === token) error.value = errorMessage(e)
    } finally {
      if (mine === token) loading.value = false
    }
  }

  return { data, loading, error, run }
}
