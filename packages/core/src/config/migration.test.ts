import { describe, expect, it } from "vitest";
import { migrateConfig } from "./migration.js";

function migrate(input: unknown): Record<string, unknown> {
  const data = input as Record<string, unknown>;
  migrateConfig(data);
  return data;
}

function obj(v: unknown): Record<string, unknown> {
  return v as Record<string, unknown>;
}

describe("config migration", () => {
  it("drops legacy max messages", () => {
    const data = migrate({ agents: { defaults: { maxMessages: 25, model: "x/y" } } });
    const defaults = obj(obj(data["agents"])["defaults"]);
    expect(defaults["maxMessages"]).toBeUndefined();
    expect(defaults["model"]).toBe("x/y");

    const snake = migrate({ agents: { defaults: { max_messages: 25 } } });
    expect(obj(obj(snake["agents"])["defaults"])["max_messages"]).toBeUndefined();
  });

  it("moves exec restrictToWorkspace to tools", () => {
    const data = migrate({ tools: { exec: { restrictToWorkspace: true, timeout: 5 } } });
    const tools = obj(data["tools"]);
    expect(tools["restrictToWorkspace"]).toBe(true);
    expect(obj(tools["exec"])["restrictToWorkspace"]).toBeUndefined();
    expect(obj(tools["exec"])["timeout"]).toBe(5);
  });

  it("does not override existing restrictToWorkspace", () => {
    const data = migrate({
      tools: { restrictToWorkspace: false, exec: { restrictToWorkspace: true } },
    });
    expect(obj(data["tools"])["restrictToWorkspace"]).toBe(false);
  });

  it("migrates legacy my tool keys", () => {
    const data = migrate({ tools: { myEnabled: false, mySet: true } });
    const tools = obj(data["tools"]);
    expect(tools["myEnabled"]).toBeUndefined();
    expect(tools["mySet"]).toBeUndefined();
    expect(tools["my"]).toEqual({ enable: false, allowSet: true });
  });

  it("new my tool keys take precedence over legacy", () => {
    const data = migrate({
      tools: { myEnabled: false, mySet: false, my: { enable: true, allowSet: true } },
    });
    expect(obj(data["tools"])["my"]).toEqual({ enable: true, allowSet: true });
  });
});
