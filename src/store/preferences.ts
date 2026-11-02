import { create } from 'zustand';
import { persist } from 'zustand/middleware';
export type Theme = 'system' | 'dark' | 'light';
type Preferences = {
  theme: Theme;
  sidebarCollapsed: boolean;
  setTheme: (theme: Theme) => void;
  toggleSidebar: () => void;
};
export const usePreferences = create<Preferences>()(
  persist(
    (set) => ({
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
