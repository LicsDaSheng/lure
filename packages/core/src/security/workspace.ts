//! workspace 路径边界：把路径解析并强制落在 workspace 内（越界抛错）。

import fs from "node:fs";
import path from "node:path";

/// 解析路径并强制落在 workspace 内；越界抛错。
export function resolveInWorkspace(inputPath: string, workspace: string): string {
  const abs = path.resolve(workspace, inputPath);
  const ws = path.resolve(workspace);
  const rel = path.relative(ws, abs);
  if (rel === ".." || rel.startsWith(`..${path.sep}`) || path.isAbsolute(rel)) {
    throw new Error(`路径越界 workspace: ${inputPath}`);
  }

  // realpath 防 symlink 逃逸（文件可能不存在，退回到父目录 realpath）。
  try {
    return fs.realpathSync(abs);
  } catch {
    const realParent = fs.realpathSync(path.dirname(abs));
    return path.join(realParent, path.basename(abs));
  }
}
