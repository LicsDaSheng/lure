import * as React from "react";
import { ArrowUp } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Textarea } from "@/components/ui/textarea";

export function Composer({
  disabled,
  onSend,
}: {
  disabled: boolean;
  onSend: (text: string) => void;
}) {
  const [value, setValue] = React.useState("");
  const ref = React.useRef<HTMLTextAreaElement>(null);

  const autosize = React.useCallback(() => {
    const el = ref.current;
    if (!el) return;
    el.style.height = "auto";
    el.style.height = `${Math.min(el.scrollHeight, 220)}px`;
  }, []);

  const submit = () => {
    const text = value.trim();
    if (!text || disabled) return;
    onSend(text);
    setValue("");
    requestAnimationFrame(autosize);
  };

  return (
    <div className="px-4 pb-4 pt-2">
      <div className="mx-auto flex max-w-[820px] items-end gap-2 rounded-2xl border bg-card px-3 py-2 shadow-sm transition-shadow focus-within:ring-2 focus-within:ring-ring">
        <Textarea
          ref={ref}
          rows={1}
          value={value}
          placeholder="发消息给 Lure…"
          className="max-h-[220px] min-h-[2.25rem] flex-1 py-2"
          onChange={(e) => {
            setValue(e.target.value);
            autosize();
          }}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
          }}
        />
        <Button
          size="icon"
          className="mb-0.5 size-8 rounded-xl"
          disabled={disabled || !value.trim()}
          onClick={submit}
          aria-label="发送"
        >
          <ArrowUp className="size-4" />
        </Button>
      </div>
    </div>
  );
}
