import { create } from 'zustand';
import { persist } from 'zustand/middleware';
export type Theme = 'system' | 'dark' | 'light';
type Preferences = {
  metricsInterval: number;
  setMetricsInterval: (seconds: number) => void;
  theme: Theme;
  sidebarCollapsed: boolean;
  setTheme: (theme: Theme) => void;
  toggleSidebar: () => void;
};
export const usePreferences = create<Preferences>()(
  persist(
    (set) => ({
      metricsInterval: 10,
      setMetricsInterval: (seconds) =>
        set({
          metricsInterval: [5, 10, 30, 60].includes(seconds) ? seconds : 10,
        }),
      theme: 'system',
      sidebarCollapsed: false,
      setTheme: (theme) => set({ theme }),
      toggleSidebar: () =>
        set((state) => ({ sidebarCollapsed: !state.sidebarCollapsed })),
    }),
    {
      name: 'remotedock-preferences',
      merge: (stored, current) => {
        const value = stored as Partial<Preferences> | undefined;
        return {
          ...current,
          metricsInterval: [5, 10, 30, 60].includes(value?.metricsInterval ?? 0)
            ? value!.metricsInterval!
            : 10,
          theme:
            value?.theme === 'dark' || value?.theme === 'light'
              ? value.theme
              : 'system',
          sidebarCollapsed: value?.sidebarCollapsed === true,
        };
      },
    },
  ),
);
