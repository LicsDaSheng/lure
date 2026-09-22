/** Pi 连接 feature 的公开边界：跨 feature 仅从此处导入类型、action、selector 和 IPC 适配接口。 */
export type * from "./pi-session-types";
export { messageText, messageThinking } from "./pi-session-types";
export { piConnectionActions, type PiConnectionState } from "./pi-session-slice";
export {
  selectCanSend,
  selectConnection,
  selectExpandedProjects,
  selectIsRunning,
  selectLoadingDirectories,
  selectMessages,
  selectPiConnection,
  selectPiSessionState,
  selectProjectSessions,
  selectRecentSessions,
  selectSessionError,
} from "./pi-session-selectors";
export type {
  ImageAttachment,
  PiCommand,
  SelectedImage,
  SessionSwitchOutcome,
  WorkspaceContext,
} from "./api";
export {
  getSessionEntries,
  listProjectSessions,
  readImageAttachments,
  selectImageFiles,
  selectProjectDirectory,
  switchPiSession,
} from "./api";