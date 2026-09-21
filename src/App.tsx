import { TooltipProvider } from "@/components/ui/tooltip";
import { AppProviders } from "@/app/providers";
import { MainContent } from "@/layouts/main-content";

/** 应用装配层只提供全局 Provider；页面编排位于 MainContent。 */
function App() {
  return (
    <AppProviders>
      <TooltipProvider>
        <MainContent />
      </TooltipProvider>
    </AppProviders>
  );
}

export default App;
