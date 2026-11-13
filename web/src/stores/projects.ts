import { defineStore } from "pinia";
import { ref } from "vue";
import { api, errorMessage } from "../lib/api";
import type { Project } from "../lib/types";

/** Projects are referenced by many pages (filters, forms) so they are cached here. */
export const useProjects = defineStore("projects", () => {
  const list = ref<Project[]>([]);
  const loaded = ref(false);
  const loading = ref(false);
  const error = ref<string | null>(null);

  async function refresh() {
    loading.value = true;
    error.value = null;
    try {
      list.value = (await api.get<Project[]>("/api/v1/projects")) ?? [];
      loaded.value = true;
    } catch (e) {
      error.value = errorMessage(e);
    } finally {
      loading.value = false;
    }
  }

  async function ensure() {
    if (!loaded.value && !loading.value) await refresh();
  }

  function nameOf(id: number | null | undefined): string | null {
    if (id === null || id === undefined) return null;
    return list.value.find((p) => p.id === id)?.name ?? null;
  }

  return { list, loaded, loading, error, refresh, ensure, nameOf };
});
