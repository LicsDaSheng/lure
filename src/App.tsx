import { Badge } from "@/components/ui/badge";

function App() {
  return (
    <main className="grid min-h-screen place-items-center overflow-hidden bg-[radial-gradient(circle_at_50%_15%,oklch(0.5_0.16_264/0.22),transparent_38%)] p-12 max-sm:p-6">
      <section
        className="w-full max-w-2xl rounded-3xl border bg-card/80 p-12 shadow-2xl shadow-black/35 backdrop-blur-xl max-sm:p-8"
        aria-labelledby="product-name"
      >
        <Badge variant="secondary" className="mb-4 text-primary">
          Pi 桌面客户端
        </Badge>
        <h1
          id="product-name"
          className="text-7xl leading-none font-bold tracking-[-0.07em] max-sm:text-6xl"
        >
          Lure
        </h1>
        <p className="my-8 max-w-xl text-base leading-7 text-muted-foreground">
          通过 Pi RPC 连接本机已安装的 Pi，在原生桌面窗口中管理会话与执行过程。
        </p>
        <div
          className="inline-flex items-center gap-2.5 text-sm text-foreground/85"
          role="status"
        >
          <span
            className="size-2 rounded-full bg-emerald-400 shadow-[0_0_16px_oklch(0.75_0.17_160/0.7)]"
            aria-hidden="true"
          />
          工程基础设施已就绪
        </div>
      </section>
    </main>
  );
}

export default App;
