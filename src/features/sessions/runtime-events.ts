import { createAction } from "@reduxjs/toolkit";

import type { EventEnvelope, PiSessionState } from "@/lib/pi-rpc/types";

/**
 * Pi 的单一事件流在 listener 入口完成排序与流式合并后，投影成各 feature
 * 可以独立消费的领域状态。原始 envelope 一并保留，便于追踪事实来源。
 */
export const piRuntimeProjected = createAction<{
  envelopes: EventEnvelope[];
  state: PiSessionState;
}>("sessions/piRuntimeProjected");
