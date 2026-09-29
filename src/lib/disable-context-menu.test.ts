import { describe, expect, it } from "vitest";

import { disableNativeContextMenu } from "./disable-context-menu";

describe("disableNativeContextMenu", () => {
  it("阻止 document 上 contextmenu 事件的默认行为", () => {
    const restore = disableNativeContextMenu(document);

    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    document.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(true);
    restore();
  });

  it("返回的清理函数恢复默认右键行为", () => {
    const restore = disableNativeContextMenu(document);
    restore();

    const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true });
    document.dispatchEvent(event);

    expect(event.defaultPrevented).toBe(false);
  });
});
