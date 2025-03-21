<template>
  <div class="mx-auto max-w-7xl px-8">
    <div class="border-b border-zinc-700 py-5">
      <h3 class="text-base font-semibold font-display leading-6 text-zinc-100">
        Settings
      </h3>
    </div>
    <div class="mt-5 flex flex-row gap-12">
      <nav class="flex flex-col" aria-label="Sidebar">
        <ul role="list" class="-mx-2 space-y-1">
          <li v-for="(item, itemIdx) in navigation" :key="item.prefix">
            <NuxtLink :href="item.route" :class="[
              itemIdx === currentPageIndex
                ? 'bg-zinc-800/50 text-zinc-100'
                : 'text-zinc-400 hover:bg-zinc-800/30 hover:text-zinc-200',
              'transition group flex gap-x-3 rounded-md p-2 pr-12 text-sm font-semibold leading-6',
            ]">
              <component :is="item.icon" :class="[
                itemIdx === currentPageIndex
                  ? 'text-zinc-100'
                  : 'text-zinc-400 group-hover:text-zinc-200',
                'transition h-6 w-6 shrink-0',
              ]" aria-hidden="true" />
              {{ item.label }}
            </NuxtLink>
          </li>
        </ul>
      </nav>
      <div class="grow">
        <NuxtPage />
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import {
  ArrowDownTrayIcon,
  CubeIcon,
  HomeIcon,
  RectangleGroupIcon,
  BugAntIcon,
} from "@heroicons/vue/16/solid";
import type { Component } from "vue";
import type { NavigationItem } from "~/types";
import { platform } from '@tauri-apps/plugin-os';
import { invoke } from "@tauri-apps/api/core";

const systemData = await invoke<{
  clientId: string;
  baseUrl: string;
  dataDir: string;
  logLevel: string;
}>("fetch_system_data");

const isDebugMode = ref(systemData.logLevel.toLowerCase() === "debug");
const debugRevealed = ref(false);

// Track shift key state and debug reveal
onMounted(() => {
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Shift') {
      isDebugMode.value = true;
      debugRevealed.value = true;
    }
  });
  
  window.addEventListener('keyup', (e) => {
    if (e.key === 'Shift') {
      isDebugMode.value = debugRevealed.value || systemData.logLevel.toLowerCase() === "debug";
    }
  });

  // Reset debug reveal when leaving the settings page
  const router = useRouter();
  router.beforeEach((to) => {
    if (!to.path.startsWith('/settings')) {
      debugRevealed.value = false;
      isDebugMode.value = systemData.logLevel.toLowerCase() === "debug";
    }
  });
});

// Make navigation reactive by wrapping in computed
const navigation = computed(() => [
  {
    label: "Home",
    route: "/settings",
    prefix: "/settings",
    icon: HomeIcon,
  },
  {
    label: "Interface", 
    route: "/settings/interface",
    prefix: "/settings/interface",
    icon: RectangleGroupIcon,
  },
  {
    label: "Downloads",
    route: "/settings/downloads",
    prefix: "/settings/downloads",
    icon: ArrowDownTrayIcon,
  },
  ...(isDebugMode.value ? [{
    label: "Debug Info",
    route: "/settings/debug", 
    prefix: "/settings/debug",
    icon: BugAntIcon,
  }] : []),
]);

const currentPlatform = platform();

// Use .value to unwrap the computed ref
const currentPageIndex = useCurrentNavigationIndex(navigation.value);

// Watch for navigation changes and update currentPageIndex
watch(navigation, (newNav) => {
  currentPageIndex.value = useCurrentNavigationIndex(newNav).value;
});
</script>
