/** Pi 连接 feature 的公开边界：跨 feature 仅从此处导入类型、action、selector 和 IPC 适配接口。 */
export type * from "./pi-session-types";
export { messageText, messageThinking } from "./pi-session-types";
export { piConnectionActions, type PiConnectionState } from "./pi-session-slice";
export {
  selectCanSend,
  selectConnection,
  selectIsRunning,
  selectMessages,
  selectPiConnection,
  selectPiSessionState,
  selectSessionError,
} from "./pi-session-selectors";
export type { ImageAttachment, PiCommand, SelectedImage, WorkspaceContext } from "./api";
export { readImageAttachments, selectImageFiles } from "./api";
