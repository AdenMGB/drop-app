<template>
  <NuxtLayout class="select-none w-screen h-screen">
    <NuxtPage />
  </NuxtLayout>
  <ModalStack />
</template>

<script setup lang="ts">
import "~/composables/queue";

import { invoke } from "@tauri-apps/api/core";
import { AppStatus } from "~/types";
import { listen } from "@tauri-apps/api/event";
import { useAppState } from "./composables/app-state.js";
import {
  initialNavigation,
  setupHooks,
} from "./composables/state-navigation.js";

const router = useRouter();

const state = useAppState();
state.value = await invoke("fetch_state");

router.beforeEach(async () => {
  state.value = await invoke("fetch_state");
});

setupHooks();
initialNavigation(state);

useHead({
  title: "Drop",
});
</script>
