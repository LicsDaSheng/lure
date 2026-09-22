import { useCallback } from "react";

import { useAppDispatch, useAppSelector } from "@/app/hooks";

import { extensionUiActions } from "./extension-ui-slice";
import { selectExtensionUi } from "./extension-ui-selectors";

export function useExtensionUi() {
  const state = useAppSelector(selectExtensionUi);
  const dispatch = useAppDispatch();
  return {
    ...state,
    respond: useCallback((value: unknown, cancelled = false) => dispatch(extensionUiActions.responseRequested({ cancelled, value })), [dispatch]),
  };
}
