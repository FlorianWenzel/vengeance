import { nextTick, onBeforeUnmount, watch, type Ref } from 'vue'

const FOCUSABLE =
  'a[href], button:not([disabled]), input:not([disabled]):not([type="hidden"]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'

// Stack so nested overlays (a dialog opened from the drawer) handle Esc innermost-first.
const stack: symbol[] = []

/**
 * Keyboard behaviour for modal surfaces: Esc closes (innermost first), Tab is trapped
 * inside the container, focus moves in on open and returns to the opener on close.
 */
export function useFocusTrap(open: Ref<boolean>, container: Ref<HTMLElement | null>, onClose: () => void) {
  const id = Symbol('overlay')
  let opener: HTMLElement | null = null

  function onKey(e: KeyboardEvent) {
    if (stack[stack.length - 1] !== id) return
    const el = container.value
    if (!el) return
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onClose()
      return
    }
    if (e.key !== 'Tab') return
    const items = Array.from(el.querySelectorAll<HTMLElement>(FOCUSABLE)).filter((n) => n.offsetParent !== null)
    if (!items.length) {
      e.preventDefault()
      el.focus()
      return
    }
    const first = items[0]
    const last = items[items.length - 1]
    if (e.shiftKey && (document.activeElement === first || document.activeElement === el)) {
      e.preventDefault()
      last.focus()
    } else if (!e.shiftKey && document.activeElement === last) {
      e.preventDefault()
      first.focus()
    }
  }

  function activate() {
    opener = document.activeElement as HTMLElement | null
    stack.push(id)
    document.addEventListener('keydown', onKey, true)
    nextTick(() => {
      const el = container.value
      if (!el) return
      const target = el.querySelector<HTMLElement>('[autofocus], [data-autofocus]') ?? el
      target.focus({ preventScroll: true })
    })
  }

  function deactivate() {
    const i = stack.indexOf(id)
    if (i >= 0) stack.splice(i, 1)
    document.removeEventListener('keydown', onKey, true)
    if (opener && document.contains(opener)) opener.focus({ preventScroll: true })
    opener = null
  }

  watch(
    open,
    (v, old) => {
      if (v && !old) activate()
      else if (!v && old) deactivate()
    },
    { immediate: true },
  )
  onBeforeUnmount(() => {
    if (open.value) deactivate()
  })
}
