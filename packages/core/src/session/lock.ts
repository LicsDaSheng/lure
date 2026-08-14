//! Per-session 互斥锁（对齐 crates/lure-core/src/session/lock.rs 语义，TS 异步适配）。

export class SessionLocks {
  private readonly held = new Set<string>();
  private readonly waiters = new Map<string, Array<() => void>>();

  /// 尝试获取 `key` 的独占锁：空闲返回 guard，已被持有返回 `undefined`（不阻塞）。
  tryLock(key: string): SessionGuard | undefined {
    if (this.held.has(key)) return undefined;
    this.held.add(key);
    return new SessionGuard(this, key);
  }

  /// 获取 `key` 的独占锁，等待直到该 key 空闲。
  async lock(key: string): Promise<SessionGuard> {
    while (this.held.has(key)) {
      await new Promise<void>((resolve) => {
        const list = this.waiters.get(key) ?? [];
        list.push(resolve);
        this.waiters.set(key, list);
      });
    }
    this.held.add(key);
    return new SessionGuard(this, key);
  }

  release(key: string): void {
    this.held.delete(key);
    const list = this.waiters.get(key) ?? [];
    this.waiters.delete(key);
    for (const resolve of list) resolve();
  }
}

export class SessionGuard {
  constructor(
    private readonly locks: SessionLocks,
    readonly key: string,
  ) {}

  release(): void {
    this.locks.release(this.key);
  }
}
