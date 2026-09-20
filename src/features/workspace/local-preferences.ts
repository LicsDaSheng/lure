/**
 * 本地界面偏好：记住上次工作目录、未发送草稿与任务标题。
 *
 * 这些数据只保存在本机浏览器存储中，不发送给 Pi。
 */

const DRAFT_PREFIX = "lure:draft:";
const TITLE_PREFIX = "lure:title:";
const LAST_DIRECTORY_KEY = "lure:last-directory";

function storage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

function read(key: string): string | null {
  const store = storage();
  if (!store) return null;
  try {
    return store.getItem(key);
  } catch {
    return null;
  }
}

function write(key: string, value: string | null) {
  const store = storage();
  if (!store) return;
  try {
    if (value === null || value === "") store.removeItem(key);
    else store.setItem(key, value);
  } catch {
    // 存储不可用时忽略，界面仍然可用。
  }
}

function scope(directory: string | null) {
  return directory ?? "default";
}

export function readDraft(directory: string | null): string {
  return read(`${DRAFT_PREFIX}${scope(directory)}`) ?? "";
}

export function writeDraft(directory: string | null, draft: string) {
  write(`${DRAFT_PREFIX}${scope(directory)}`, draft);
}

export function readTitle(directory: string | null): string | null {
  return read(`${TITLE_PREFIX}${scope(directory)}`);
}

export function writeTitle(directory: string | null, title: string | null) {
  write(`${TITLE_PREFIX}${scope(directory)}`, title);
}

export function readLastDirectory(): string | null {
  return read(LAST_DIRECTORY_KEY);
}

export function writeLastDirectory(directory: string | null) {
  write(LAST_DIRECTORY_KEY, directory);
}