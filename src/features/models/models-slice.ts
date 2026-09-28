import { createSlice, type PayloadAction } from "@reduxjs/toolkit";

import { piRuntimeProjected } from "@/features/sessions/runtime-events";
import type { ModelSnapshot } from "@/lib/pi-rpc/types";

export type ModelsState = {
  availableModels: ModelSnapshot[];
  current: ModelSnapshot | null;
  thinkingLevel: string | null;
};

export const initialModelsState: ModelsState = {
  availableModels: [],
  current: null,
  thinkingLevel: null,
};

const modelsSlice = createSlice({
  name: "models",
  initialState: initialModelsState,
  reducers: {
    modelsReset: () => initialModelsState,
    modelSelectionRequested: (_state, _action: PayloadAction<ModelSnapshot>) =>
      undefined,
    thinkingLevelSelectionRequested: (_state, _action: PayloadAction<string>) =>
      undefined,
    modelSelected: (state, action: PayloadAction<ModelSnapshot>) => {
      state.current = action.payload;
    },
    thinkingLevelSelected: (state, action: PayloadAction<string>) => {
      state.thinkingLevel = action.payload;
    },
    modelsLoaded: (state, action: PayloadAction<ModelSnapshot[]>) => {
      state.availableModels = action.payload;
    },
    capabilitiesCleared: (state) => {
      state.availableModels = [];
    },
  },
  extraReducers: (builder) => {
    builder.addCase(piRuntimeProjected, (state, action) => {
      state.current = action.payload.state.connection.model;
      state.thinkingLevel = action.payload.state.connection.thinkingLevel;
    });
  },
});

export const modelsActions = modelsSlice.actions;
export const modelsReducer = modelsSlice.reducer;
