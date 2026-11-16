import { TerminalWorkspace } from "@/features/terminal/terminal-workspace";

/** Lure 直接通过 Rust PTY 与 xterm.js 呈现 Pi 原生 TUI。 */
function App() {
  return <TerminalWorkspace />;
}

export default App;
