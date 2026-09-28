import { useCallback } from "react";

import { useAppDispatch, useAppSelector } from "@/app/hooks";
import type { ImageAttachment } from "@/lib/pi-rpc/client";

import { conversationActions } from "./conversation-slice";
import { selectConversation } from "./conversation-selectors";

export function useConversation() {
  const state = useAppSelector(selectConversation);
  const dispatch = useAppDispatch();
  return {
    ...state,
    prompt: useCallback(
      (message: string, images: ImageAttachment[] = []) =>
        dispatch(conversationActions.promptRequested({ images, message })),
      [dispatch],
    ),
    abort: useCallback(
      () => dispatch(conversationActions.abortRequested()),
      [dispatch],
    ),
    steer: useCallback(
      (message: string, images: ImageAttachment[] = []) =>
        dispatch(conversationActions.steerRequested({ images, message })),
      [dispatch],
    ),
    followUp: useCallback(
      (message: string, images: ImageAttachment[] = []) =>
        dispatch(conversationActions.followUpRequested({ images, message })),
      [dispatch],
    ),
    clearQueue: useCallback(
      () => dispatch(conversationActions.queueClearRequested()),
      [dispatch],
    ),
  };
}
