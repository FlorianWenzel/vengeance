import { defineStore } from "pinia";
import { ref } from "vue";

export interface Toast {
  id: number;
  kind: "ok" | "error" | "info";
  text: string;
}

let seq = 0;

export const useToasts = defineStore("toasts", () => {
  const items = ref<Toast[]>([]);

  function push(text: string, kind: Toast["kind"] = "ok", ms = 4000) {
    const id = ++seq;
    items.value.push({ id, kind, text });
    window.setTimeout(() => dismiss(id), kind === "error" ? ms * 2 : ms);
  }

  function dismiss(id: number) {
    items.value = items.value.filter((t) => t.id !== id);
  }

  return {
    items,
    push,
    dismiss,
    ok: (t: string) => push(t, "ok"),
    error: (t: string) => push(t, "error"),
  };
});
