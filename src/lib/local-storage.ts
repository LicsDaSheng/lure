function storage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

export function readLocalValue(key: string): string | null {
  const store = storage();
  if (!store) return null;
  try {
    return store.getItem(key);
  } catch {
    return null;
  }
}

export function writeLocalValue(key: string, value: string | null) {
  const store = storage();
  if (!store) return;
  try {
    if (value === null || value === "") store.removeItem(key);
    else store.setItem(key, value);
  } catch {
    // 存储不可用时忽略，界面仍然可用。
  }
}

export function directoryScope(directory: string | null) {
  return directory ?? "default";
}
