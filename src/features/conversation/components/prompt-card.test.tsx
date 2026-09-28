import { ThreadPrimitive } from "@assistant-ui/react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { PiAssistantRuntimeProvider } from "./assistant-runtime";
import { PromptCard } from "./prompt-card";
import type { MessageQueue } from "@/lib/pi-rpc/types";

const emptyQueue: MessageQueue = { steering: [], followUp: [] };

function renderCard({
  isRunning = true,
  queue = emptyQueue,
  onSteer = vi.fn(),
  onFollowUp = vi.fn(),
  onClearQueue = vi.fn(),
  onStop = vi.fn(),
} = {}) {
  const props = {
    branch: null,
    directoryName: "项目",
    isRunning,
    model: null,
    models: [],
    onAddImages: vi.fn().mockResolvedValue([]),
    onClearQueue,
    onDraftChange: vi.fn(),
    onFollowUp,
    onSelectModel: vi.fn(),
    onSelectThinkingLevel: vi.fn(),
    onSteer,
    onStop,
    phase: (isRunning ? "running" : "ready") as "running" | "ready",
    queue,
    textareaRef: { current: null },
    thinkingLevel: null,
  };
  render(
    <PiAssistantRuntimeProvider
      activeAssistantId={null}
      canSend={!isRunning}
      isRunning={isRunning}
      messages={[]}
      onCancel={() => undefined}
      onNew={() => undefined}
    >
      <ThreadPrimitive.Root>
        <ThreadPrimitive.Viewport>
          <PromptCard {...props} />
        </ThreadPrimitive.Viewport>
      </ThreadPrimitive.Root>
    </PiAssistantRuntimeProvider>,
  );
  return props;
}

function typeInstruction(text: string) {
  const input = screen.getByRole("textbox", { name: "任务指令" });
  fireEvent.change(input, { target: { value: text } });
  return input;
}

describe("运行中的排队操作", () => {
  it("运行中提供插队引导、排队发送和停止入口", () => {
    renderCard({ isRunning: true });

    expect(screen.getByRole("button", { name: "插队引导" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "排队发送" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "停止生成" })).toBeInTheDocument();
  });

  it("空闲时不提供排队入口", () => {
    renderCard({ isRunning: false });

    expect(screen.queryByRole("button", { name: "插队引导" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "排队发送" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "发送消息" })).toBeInTheDocument();
  });

  it("无文本时排队入口不可用", () => {
    renderCard({ isRunning: true });

    expect(screen.getByRole("button", { name: "插队引导" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "排队发送" })).toBeDisabled();
  });

  it("点击排队发送提交文本并清空输入", async () => {
    const { onFollowUp, onSteer } = renderCard({ isRunning: true });
    const input = typeInstruction("接着补测试");

    fireEvent.click(screen.getByRole("button", { name: "排队发送" }));

    expect(onFollowUp).toHaveBeenCalledWith("接着补测试");
    expect(onSteer).not.toHaveBeenCalled();
    await vi.waitFor(() => expect(input).toHaveValue(""));
  });

  it("点击插队引导提交 steer 文本", () => {
    const { onSteer } = renderCard({ isRunning: true });
    typeInstruction("先停下重构");

    fireEvent.click(screen.getByRole("button", { name: "插队引导" }));

    expect(onSteer).toHaveBeenCalledWith("先停下重构");
  });

  it("运行中 Enter 视为排队发送", () => {
    const { onFollowUp } = renderCard({ isRunning: true });
    const input = typeInstruction("接着补测试");

    fireEvent.keyDown(input, { key: "Enter" });

    expect(onFollowUp).toHaveBeenCalledWith("接着补测试");
  });

  it("运行中中文输入法确认候选词的 Enter 不排队发送", () => {
    const { onFollowUp } = renderCard({ isRunning: true });
    const input = typeInstruction("继续检查");

    fireEvent.keyDown(input, { key: "Enter", keyCode: 229 });

    expect(onFollowUp).not.toHaveBeenCalled();
    expect(input).toHaveValue("继续检查");
  });

  it("运行中 Shift+Enter 不触发排队发送", () => {
    const { onFollowUp } = renderCard({ isRunning: true });
    const input = typeInstruction("接着补测试");

    fireEvent.keyDown(input, { key: "Enter", shiftKey: true });

    expect(onFollowUp).not.toHaveBeenCalled();
  });
});

describe("待处理队列展示", () => {
  it("队列非空时列出插队与排队消息", () => {
    renderCard({
      isRunning: true,
      queue: { steering: ["先停下重构"], followUp: ["接着补测试", "再更新文档"] },
    });

    expect(screen.getByText(/已排队 3 条/)).toBeInTheDocument();
    expect(screen.getByText("先停下重构")).toBeInTheDocument();
    expect(screen.getByText("接着补测试")).toBeInTheDocument();
    expect(screen.getByText("再更新文档")).toBeInTheDocument();
  });

  it("队列为空时不显示队列区", () => {
    renderCard({ isRunning: true });

    expect(screen.queryByRole("group", { name: "待处理队列" })).not.toBeInTheDocument();
  });

  it("清空按钮触发清空队列", () => {
    const { onClearQueue } = renderCard({
      isRunning: true,
      queue: { steering: [], followUp: ["接着补测试"] },
    });

    fireEvent.click(screen.getByRole("button", { name: "清空队列" }));

    expect(onClearQueue).toHaveBeenCalledOnce();
  });
});
