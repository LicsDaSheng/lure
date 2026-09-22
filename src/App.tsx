import { TooltipProvider } from "@/components/ui/tooltip";
import { AppProviders } from "@/app/providers";
import { AppShell } from "@/app/app-shell";

/** 应用装配层只提供全局 Provider；窗口级编排位于 AppShell。 */
function App() {
  return (
    <AppProviders>
      <TooltipProvider>
        <AppShell />
      </TooltipProvider>
    </AppProviders>
  );
}

export default App;
