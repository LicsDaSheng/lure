import { describe, expect, it } from "vitest";
import { formatElapsed } from "./activity-timer";

describe("formatElapsed", () => {
  it("60 秒内显示 Ns", () => {
    expect(formatElapsed(0)).toBe("0s");
    expect(formatElapsed(1)).toBe("1s");
    expect(formatElapsed(59)).toBe("59s");
  });

  it("60 秒以上显示 M:SS", () => {
    expect(formatElapsed(60)).toBe("1:00");
    expect(formatElapsed(65)).toBe("1:05");
    expect(formatElapsed(125)).toBe("2:05");
  });
});
