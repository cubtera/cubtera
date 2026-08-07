import { Button } from '@/components/ui/button';
import { useAppStore } from '@/store/useAppStore';
import { Moon, Sun } from 'lucide-react';

export function Header() {
  const { currentOrg, theme, setTheme } = useAppStore();

  const toggleTheme = () => {
    setTheme(theme === 'light' ? 'dark' : 'light');
  };

  return (
    <header className="border-b bg-background/95 backdrop-blur supports-[backdrop-filter]:bg-background/60">
      <div className="flex h-16 items-center justify-between px-6">
        {/* Organization info */}
        <div className="flex items-center space-x-4">
          <div>
            <h2 className="text-lg font-semibold">Organization: {currentOrg}</h2>
            <p className="text-sm text-muted-foreground">
              Infrastructure Management Dashboard
            </p>
          </div>
        </div>

        {/* Actions */}
        <div className="flex items-center space-x-2">
          <Button
            variant="ghost"
            size="icon"
            onClick={toggleTheme}
            className="h-9 w-9"
          >
            {theme === 'light' ? (
              <Moon className="h-4 w-4" />
            ) : (
              <Sun className="h-4 w-4" />
            )}
          </Button>
        </div>
      </div>
    </header>
  );
} 