// reasoning-summary 模型（OpenAI gpt-5.x 家族、经 Responses API 中继到 OpenAI chat wire 的模型）
// 每个完成的 summary part 产生一个 delta，以 markdown 加粗标题开头。chat wire 无 summary_index，
// 拼接后得到 `...PRs****Inspecting...` 的 `****` 粘连，markdown 读作既不闭合也不开启的半个加粗。
// 后端在 delta 到达时插入换行修复；此处幂等地修复持久化前的旧数据与仍粘连的 provider 输出。

// 标题紧贴上一 part 的两种 wire 形态：
//   1. 标题接标题 —— `**One****Two**`，裸 `****` 运行。
//   2. prose 接标题 —— `interaction!**Two**`。
const GLUED_HEADING_RUN = /(?<!\*)\*{4}(?!\*)/g;
const GLUED_AFTER_PROSE = /(?<=[^\s*])(\*\*(?=[^\s*])[^\n]*?\*\*)/g;

export function separateGluedReasoningBlocks(text: string): string {
  return text
    .replace(GLUED_HEADING_RUN, "**\n\n**")
    .replace(GLUED_AFTER_PROSE, "\n\n$1");
}
