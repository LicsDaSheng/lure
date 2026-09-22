import type { ProjectDescriptor } from "@/lib/pi-rpc/types";
import { directoryScope, readLocalValue, writeLocalValue } from "@/lib/local-storage";

/**
 * 本地界面偏好：记住项目导航、上次工作目录、未发送草稿与任务标题。
 *
 * 这些数据只保存在本机浏览器存储中，不发送给 Pi。
 */

const TITLE_PREFIX = "lure:title:";
const LAST_DIRECTORY_KEY = "lure:last-directory";
const PROJECTS_KEY = "lure:projects";

export function readTitle(directory: string | null): string | null {
  return readLocalValue(`${TITLE_PREFIX}${directoryScope(directory)}`);
}

export function writeTitle(directory: string | null, title: string | null) {
  writeLocalValue(`${TITLE_PREFIX}${directoryScope(directory)}`, title);
}

export function readLastDirectory(): string | null {
  return readLocalValue(LAST_DIRECTORY_KEY);
}

export function writeLastDirectory(directory: string | null) {
  writeLocalValue(LAST_DIRECTORY_KEY, directory);
}

export function readProjects(): ProjectDescriptor[] {
  const value = readLocalValue(PROJECTS_KEY);
  if (!value) return [];
  try {
    const projects = JSON.parse(value) as unknown;
    if (!Array.isArray(projects)) return [];
    return projects.filter(
      (project): project is ProjectDescriptor =>
        typeof project === "object" &&
        project !== null &&
        typeof project.name === "string" &&
        project.name.trim().length > 0 &&
        typeof project.directory === "string" &&
        project.directory.trim().length > 0,
    );
  } catch {
    return [];
  }
}

export function writeProjects(projects: ProjectDescriptor[]) {
  writeLocalValue(PROJECTS_KEY, JSON.stringify(projects));
}
