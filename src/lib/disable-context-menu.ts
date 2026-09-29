/**
 * 禁用 WebView 原生右键上下文菜单。
 *
 * 桌面客户端不提供原生右键菜单，默认菜单（刷新、检查元素等）属于
 * 浏览器行为，会让桌面应用显得不像原生应用。返回清理函数以便测试
 * 或未来局部恢复。
 */
export function disableNativeContextMenu(target: Document): () => void {
  const handler = (event: MouseEvent) => event.preventDefault();
  target.addEventListener("contextmenu", handler);
  return () => target.removeEventListener("contextmenu", handler);
}
