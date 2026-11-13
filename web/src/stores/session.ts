import { defineStore } from "pinia";
import { ref } from "vue";
import { api, errorMessage } from "../lib/api";
import type { Me } from "../lib/types";

export const useSession = defineStore("session", () => {
  const me = ref<Me | null>(null);
  const error = ref<string | null>(null);
  const loading = ref(false);

  async function load() {
    loading.value = true;
    error.value = null;
    try {
      me.value = await api.get<Me>("/api/v1/me");
    } catch (e) {
      error.value = errorMessage(e);
    } finally {
      loading.value = false;
    }
  }

  async function signOut() {
    try {
      await api.post("/auth/logout");
    } catch {
      /* the session is gone either way */
    }
    location.href = "/auth/login";
  }

  return { me, error, loading, load, signOut };
});
