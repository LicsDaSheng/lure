import { createSlice } from "@reduxjs/toolkit";

import { idleRunState } from "@/features/conversation/public";
import { piRuntimeProjected } from "@/features/sessions/runtime-events";
import type { RunState } from "@/lib/pi-rpc/types";

export type ExecutionState = {
  notice: string | null;
  diagnostics: string[];
  run: RunState;
};

export const initialExecutionState: ExecutionState = {
  notice: null,
  diagnostics: [],
  run: idleRunState(),
};

const executionSlice = createSlice({
  name: "execution",
  initialState: initialExecutionState,
  reducers: {
    executionReset: () => initialExecutionState,
  },
  extraReducers: (builder) => {
    builder.addCase(piRuntimeProjected, (state, action) => {
      state.notice = action.payload.state.notice;
      state.diagnostics = action.payload.state.diagnostics;
      state.run = action.payload.state.run;
    });
  },
});

export const executionActions = executionSlice.actions;
export const executionReducer = executionSlice.reducer;
