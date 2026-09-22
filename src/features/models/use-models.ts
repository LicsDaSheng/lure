import { useCallback } from "react";

import { useAppDispatch, useAppSelector } from "@/app/hooks";

import { modelsActions } from "./models-slice";
import { selectModels } from "./models-selectors";

export function useModels() {
  const state = useAppSelector(selectModels);
  const dispatch = useAppDispatch();
  return {
    ...state,
    setModel: useCallback((provider: string, id: string) => dispatch(modelsActions.modelSelectionRequested({ provider, id })), [dispatch]),
    setThinkingLevel: useCallback((level: string) => dispatch(modelsActions.thinkingLevelSelectionRequested(level)), [dispatch]),
  };
}
