import * as React from "react";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { updateAgent, type Settings } from "@/lib/settings";
import { SectionTitle, Field, SaveBar, useSaver } from "./parts";

export function GeneralPanel({
  token,
  settings,
  onChange,
}: {
  token: string;
  settings: Settings;
  onChange: (s: Settings) => void;
}) {
  const { agent, providers, model_presets } = settings;
  const [model, setModel] = React.useState(agent.model);
  const [provider, setProvider] = React.useState(agent.provider);
  const [contextWindow, setContextWindow] = React.useState(
    String(agent.context_window_tokens),
  );
  const [preset, setPreset] = React.useState(agent.model_preset);
  const saver = useSaver();

  // 载荷更新（如保存返回新值）时同步本地表单。
  React.useEffect(() => {
    setModel(agent.model);
    setProvider(agent.provider);
    setContextWindow(String(agent.context_window_tokens));
    setPreset(agent.model_preset);
  }, [agent]);

  const save = () =>
    saver.run(async () => {
      const next = await updateAgent(token, {
        model,
        provider,
        context_window_tokens: Number(contextWindow) || 0,
        model_preset: preset,
      });
      onChange(next);
    });

  return (
    <div>
      <SectionTitle
        title="通用"
        desc="默认 agent 使用的模型、提供商与上下文窗口，以及当前生效的模型预设。"
      />

      <Field label="模型" htmlFor="model" hint="provider 下的模型标识，如 deepseek/deepseek-chat">
        <Input id="model" value={model} onChange={(e) => setModel(e.target.value)} />
      </Field>

      <Field label="提供商" hint="从已配置的提供商中选择">
        <Select value={provider} onValueChange={setProvider}>
          <SelectTrigger>
            <SelectValue placeholder="选择提供商" />
          </SelectTrigger>
          <SelectContent>
            {providers.map((p) => (
              <SelectItem key={p.name} value={p.name}>
                {p.label}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>

      <Field label="上下文窗口 (tokens)" htmlFor="ctx">
        <Input
          id="ctx"
          type="number"
          min={0}
          value={contextWindow}
          onChange={(e) => setContextWindow(e.target.value)}
        />
      </Field>

      <Field label="生效模型预设" hint="切换后默认 agent 使用该预设参数">
        <Select value={preset} onValueChange={setPreset}>
          <SelectTrigger>
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            {model_presets.map((p) => (
              <SelectItem key={p.name} value={p.name}>
                {p.label}
                {p.is_default ? "（默认）" : ""}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </Field>

      <SaveBar {...saver} onSave={save} />
    </div>
  );
}
