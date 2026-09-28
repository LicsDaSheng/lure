import type { LureError } from "@/lib/pi-rpc/types";

export function normalizeLureError(
  error: unknown,
  fallback: string,
): LureError {
  if (typeof error === "object" && error !== null) {
    const candidate = error as Partial<LureError>;
    if (
      typeof candidate.code === "string" &&
      typeof candidate.message === "string"
    ) {
      return { code: candidate.code, message: candidate.message };
    }
  }
  return {
    code: "CLIENT_ERROR",
    message: error instanceof Error ? error.message : fallback,
  };
}
