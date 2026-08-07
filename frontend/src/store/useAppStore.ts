import { create } from 'zustand';
import { devtools } from 'zustand/middleware';

interface AppState {
  // Current organization
  currentOrg: string;
  setCurrentOrg: (org: string) => void;
  
  // Theme
  theme: 'light' | 'dark' | 'system';
  setTheme: (theme: 'light' | 'dark' | 'system') => void;
  
  // Sidebar state
  sidebarOpen: boolean;
  setSidebarOpen: (open: boolean) => void;
  
  // Current dimension selection
  selectedDimensionType?: string;
  setSelectedDimensionType: (type?: string) => void;
  
  // Loading states
  isLoading: boolean;
  setIsLoading: (loading: boolean) => void;
}

export const useAppStore = create<AppState>()(
  devtools(
    (set) => ({
      // Organization
      currentOrg: 'cubtera', // default org
      setCurrentOrg: (org) => set({ currentOrg: org }),
      
      // Theme
      theme: 'system',
      setTheme: (theme) => set({ theme }),
      
      // Sidebar
      sidebarOpen: true,
      setSidebarOpen: (open) => set({ sidebarOpen: open }),
      
      // Dimension selection
      selectedDimensionType: undefined,
      setSelectedDimensionType: (type) => set({ selectedDimensionType: type }),
      
      // Loading
      isLoading: false,
      setIsLoading: (loading) => set({ isLoading: loading }),
    }),
    {
      name: 'cubtera-app-store',
    }
  )
); 