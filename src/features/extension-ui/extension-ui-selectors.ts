import type { RootState } from "@/app/store";

export const selectExtensionUi = (state: RootState) => state.extensionUi;
export const selectExtensionRequest = (state: RootState) => state.extensionUi.request;
