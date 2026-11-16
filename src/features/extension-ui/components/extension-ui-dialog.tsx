import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import type { ExtensionUiRequest } from "@/lib/pi-rpc/types";
import { useEffect, useRef, useState } from "react";

/**
 * Pi 扩展 UI 请求的原生对话框。
 *
 * Pi RPC 对话子协议只有 select/confirm/input/editor 四种原语：
 * - select：单选，options 为完整选项文本，用单选框平铺展示；
 * - input/editor：自由文本，扩展会把问题、编号选项和填写说明打包进多行
 *   title（例如多选题的降级形态），因此 title 必须在正文完整展示，
 *   输入语义（如多选的 "1,3" 编号写法、空输入提交空选择）由扩展定义，
 *   桌面端只透传用户输入，不做解析。
 */
// ask_user_question 的 RPC fallback 将自定义回答放在编号选项末尾；选择后
// 扩展会发来独立的 input 请求。仅识别明确的保留标签，普通 select 末项仍需确认。
function isCustomAnswerOption(option: string, index: number, count: number) {
  return (
    index === count - 1 &&
    (option === `${index + 1}. Type something.` ||
      option === `${index + 1}. 输入内容`)
  );
}

export function ExtensionUiDialog({
  request,
  onRespond,
}: {
  request: ExtensionUiRequest | null;
  onRespond: (value: unknown, cancelled?: boolean) => Promise<void>;
}) {
  const [value, setValue] = useState("");
  const submitted = useRef(false);

  useEffect(() => {
    setValue(request?.defaultValue ?? request?.options[0] ?? "");
    submitted.current = false;
  }, [request]);

  if (!request) return null;

  const isChoice = request.method === "select";
  const isEditor = request.method === "editor";
  const isTextEntry = request.method === "input" || isEditor;
  const showsInlinePrompt = isChoice || isTextEntry;

  const submit = async (resolvedValue: unknown) => {
    submitted.current = true;
    await onRespond(resolvedValue, false);
  };

  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !submitted.current) void onRespond(null, true);
      }}
    >
      <DialogContent className="rounded-xl">
        <DialogHeader>
          <DialogTitle>
            {request.method === "confirm"
              ? (request.title ?? "请确认")
              : "Pi 需要你的输入"}
          </DialogTitle>
          <DialogDescription>
            {request.message ??
              (showsInlinePrompt
                ? "请根据下方提示完成后提交。"
                : "请提供继续执行所需的信息。")}
          </DialogDescription>
        </DialogHeader>

        {showsInlinePrompt && request.title && (
          <div className="max-h-60 overflow-y-auto whitespace-pre-wrap rounded-lg border bg-muted/40 p-3 text-sm">
            {request.title}
          </div>
        )}

        {isChoice && (
          <div aria-label="选择一项" className="grid gap-2" role="radiogroup">
            {request.options.map((option, index) => (
              <label
                className="flex cursor-pointer items-start gap-2 rounded-lg border px-3 py-2 text-sm hover:bg-muted/50"
                key={option}
              >
                <input
                  checked={value === option}
                  className="mt-0.5 accent-primary"
                  name={`extension-ui-${request.requestId}`}
                  onChange={() => {
                    setValue(option);
                    if (
                      isCustomAnswerOption(
                        option,
                        index,
                        request.options.length,
                      )
                    ) {
                      void submit(option);
                    }
                  }}
                  type="radio"
                  value={option}
                />
                <span>{option}</span>
              </label>
            ))}
          </div>
        )}

        {isTextEntry &&
          (isEditor ? (
            <textarea
              aria-label="编辑内容"
              className="min-h-24 resize-y rounded-lg border bg-background p-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
              onChange={(event) => setValue(event.target.value)}
              placeholder={request.placeholder ?? undefined}
              value={value}
            />
          ) : (
            <input
              aria-label="输入内容"
              className="h-10 rounded-lg border bg-background px-3 text-sm outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
              onChange={(event) => setValue(event.target.value)}
              placeholder={request.placeholder ?? undefined}
              type="text"
              value={value}
            />
          ))}

        <DialogFooter>
          <Button onClick={() => void onRespond(null, true)} variant="outline">
            取消
          </Button>
          {request.method === "confirm" ? (
            <Button onClick={() => void submit(true)}>确认操作</Button>
          ) : isChoice ? (
            <Button disabled={!value} onClick={() => void submit(value)}>
              确认选择
            </Button>
          ) : (
            <Button onClick={() => void submit(value)}>提交输入</Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
