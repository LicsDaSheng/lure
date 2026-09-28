import {
  directoryScope,
  readLocalValue,
  writeLocalValue,
} from "@/lib/local-storage";

const DRAFT_PREFIX = "lure:draft:";

export function readDraft(directory: string | null): string {
  return readLocalValue(`${DRAFT_PREFIX}${directoryScope(directory)}`) ?? "";
}

export function writeDraft(directory: string | null, draft: string) {
  writeLocalValue(`${DRAFT_PREFIX}${directoryScope(directory)}`, draft);
}
