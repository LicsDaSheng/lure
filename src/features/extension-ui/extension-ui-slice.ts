import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

import { piRuntimeProjected } from "@/features/sessions/runtime-events";
import type { ExtensionUiRequest } from "@/lib/pi-rpc/types";

export type ExtensionUiState = {
  request: ExtensionUiRequest | null;
};

export const initialExtensionUiState: ExtensionUiState = { request: null };

const extensionUiSlice = createSlice({
  name: "extensionUi",
  initialState: initialExtensionUiState,
  reducers: {
    extensionUiReset: () => initialExtensionUiState,
    responseRequested: (_state, _action: PayloadAction<{ value: unknown; cancelled: boolean }>) => undefined,
  },
  extraReducers: (builder) => {
    builder.addCase(piRuntimeProjected, (state, action) => {
      state.request = action.payload.state.extensionRequest;
    });
  },
});

export const extensionUiActions = extensionUiSlice.actions;
export const extensionUiReducer = extensionUiSlice.reducer;
