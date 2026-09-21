# Redux Toolkit Agent 开发规约

本规约是 Lure 的前端开发规约之一，适用于所有引入或修改 Redux 状态的前端功能。它与 [UI 设计规约](./ui-design/index.md) 共同生效；二者冲突时，应先更新规约再实现。

## 1. 版本基线

- Redux Toolkit 必须使用 **2.x**：`@reduxjs/toolkit >=2.0.0 <3.0.0`。
- React 绑定必须使用与 React 19 兼容的 `react-redux 9.x`：`react-redux >=9.0.0 <10.0.0`。
- 实际安装版本必须由 `package.json` 与 `pnpm-lock.yaml` 一并锁定；不得混用 Redux Toolkit 1.x 的迁移写法或依赖其行为。
- 升级 RTK 主版本前，必须更新本规约、依赖版本和相关测试。

## 2. 状态职责与事实边界

Redux store 只保存桌面端的可观察 UI 状态、事件归并结果和本地偏好；它不是 Pi Agent 的替代实现。

- Pi RPC 返回的会话、模型、命令、消息、工具执行和运行生命周期是事实来源。reducer 只保存、归并和投影这些事实，不推断或复制 Pi 的调度、认证、队列、上下文压缩或 Agent 规则。
- 每个 Pi 事件先保留可追溯的领域事实，再由 selector 派生视图状态；禁止在组件中扫描、重排或二次拼接事件流。
- Pi 高频文本/思考增量在 listener middleware 入口按有界时间窗口合并，以单个批次 action 进入 reducer；工具、完成、错误和交互事件必须在派发前先刷新已排队增量，保持 Pi 事件顺序与事实内容不变。
- 输入框临时编辑内容、局部展开状态等仅被单个组件使用的状态，留在组件本地；跨页面、跨 feature 或需要在 Pi 事件后保持一致的状态才进入 store。
- 不在 Redux state 中存放 `Promise`、函数、Tauri 句柄、订阅对象、DOM 节点、React 元素或其他不可序列化值。二进制附件仅保存可序列化的元数据与引用。

## 3. 文件结构

按产品能力建立 feature，禁止建立按 Redux 技术层堆叠的全局 `actions/`、`reducers/` 或 `stores/` 目录。

```text
src/
├── app/
│   ├── store.ts                 # configureStore、rootReducer、middleware
│   ├── hooks.ts                 # useAppDispatch、useAppSelector
│   └── providers.tsx            # <Provider> 与应用级 Provider 组合
├── features/
│   └── conversation/
│       ├── conversation-slice.ts    # 状态、reducer、action
│       ├── conversation-selectors.ts# 纯 selector 与派生视图模型
│       ├── conversation-listeners.ts# Pi/Tauri 副作用与事件订阅
│       ├── conversation-types.ts    # feature 领域类型
│       ├── conversation-api.ts      # 窄 IPC/RPC 适配接口（如需要）
│       ├── components/              # 该 feature 的 UI
│       └── conversation.test.ts     # reducer、selector、listener 行为测试
├── components/                  # 已证实可跨 feature 复用的无业务组件
└── lib/                         # 与 Redux 无关的通用工具和基础类型
```

- feature 只能通过其公开的 action、selector、类型或适配接口被其他 feature 使用；不得导入其内部 state 结构。
- `app/store.ts` 只负责装配，不承载领域 reducer 或业务副作用。
- 初始状态、action 和 selector 与所属 feature 同置；只有稳定的共享类型才可上移。

## 4. RTK API 使用边界

### 必须使用

- 使用 `configureStore` 创建唯一应用 store，并保留 RTK 默认 middleware；新增 middleware 必须说明用途与顺序。
- 使用 `createSlice` 定义 feature 状态和同步领域转换。reducer 必须保持同步、确定且无副作用。
- 使用 `createSelector` 派生列表、计数、显示状态和跨 slice 组合视图；组件不得直接依赖复杂 state 路径或自行计算同类派生数据。
- 对按 ID 管理的消息、工具调用等集合，优先使用 `createEntityAdapter`；展示顺序由显式 ID 序列或领域字段定义，不依赖对象遍历顺序。
- 使用 `createListenerMiddleware` 承载 Pi 事件订阅、Tauri IPC 调用、取消、重试、跨 slice 编排和副作用后的 dispatch。监听器必须在应用启动时注册，并在断开或卸载时清理订阅。

### 受限使用

- `createAsyncThunk` 仅用于有明确 pending/fulfilled/rejected 生命周期的一次性、可取消请求。长连接、Pi 流式事件和持续订阅必须使用 listener middleware，不能以 thunk 持有。
- `extraReducers` 只响应已定义的跨 feature 领域 action 或 thunk 生命周期；不得把其他 feature 的内部 action 当作隐式接口。
- `createEntityAdapter` 的 selector 必须以 feature selector 为入口导出，不能让组件直接访问 adapter state。

### 禁止使用

- 不使用手写 Redux action type 字符串、手写 `createStore`、全局可变 store 单例访问，或在 React 组件外直接 `store.dispatch()`。
- 不使用 RTK Query 管理 Pi RPC、JSONL 流、Tauri command 或 Agent 生命周期。只有出现可缓存、请求/响应式的 HTTP 数据源，并经架构评审后，才可单独引入 RTK Query。
- 不在 reducer、selector 或 `prepare` 回调中调用 `invoke`、订阅事件、读写文件、计时、生成随机 ID 或发起网络请求。

## 5. 组件与副作用边界

- React 组件只能通过 `useAppSelector` 读取状态、通过 `useAppDispatch` 发出 feature 公共 action；禁止直接调用 Tauri IPC 改写同一领域状态。
- `useAppDispatch`、`useAppSelector` 的类型从 `AppDispatch`、`RootState` 推导，统一定义在 `app/hooks.ts`；业务代码不得使用未类型化的 `useDispatch` 或 `useSelector`。
- IPC/RPC 适配层负责协议参数和错误归一化；listener 负责何时调用适配层与如何分派结果；slice 负责状态转移；组件负责展示和用户意图。
- 一项用户操作涉及外部影响、取消或错误恢复时，先 dispatch 意图 action，再由 listener 执行；界面状态以 listener 分派的结果 action 更新。

## 6. 命名、测试与变更门禁

- slice 文件名使用 `*-slice.ts`，slice 名称使用 feature 领域名；action 使用业务动词，例如 `promptSubmitted`、`piEventReceived`，不用 `setData`、`updateState` 等无语义名称。
- 每个新增 reducer 至少覆盖正常转换、边界输入和事件乱序/重复时的行为；每个 selector 覆盖其派生规则；每个 listener 覆盖成功、失败和取消或清理路径。
- Pi 流式事件的测试必须断言消息、`parts` 和工具调用的顺序保持与 Pi 事实一致。
- 完成 Redux 相关变更前必须运行 `pnpm check` 与 `pnpm test`；涉及前端产物时还必须运行 `pnpm build`。无法执行时，在交付中说明原因与未验证范围。
