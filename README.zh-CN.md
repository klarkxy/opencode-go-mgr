# OCG Manager

<p align="center">
  <img src="assets/logo/ocg_logo_final_transparent.png" alt="OCG Manager Logo" width="160">
</p>

> 一款 Windows 桌面应用，用于管理多个 OpenCode-Go API key，并提供内置的 OpenAI 兼容 Gateway，可透明地在多个账号间轮询、故障转移并统计用量。

[功能特性](#功能特性) · [面向使用者](#面向使用者) · [面向维护者](#面向维护者) · [许可证](#许可证)

---

## 功能特性

- **多账号管理**：添加、编辑、启用/禁用、切换多个 OpenCode-Go 账号；key 本地存储，绝不上传。
- **OpenAI 兼容 Gateway**：本地 `http://127.0.0.1:9042/v1/chat/completions` 端点，任何 OpenAI SDK 或工具都可直连。
- **轮询与故障转移**：顺序 / 随机 / 轮询三种选择策略；请求失败时自动切换到下一个可用账号（最多重试 5 次）。
- **账号级熔断**：账号连续出错时按窗口升级冷却时间；冷却到期自动恢复，也可手动重置。
- **用量与费用统计**：基于 token 数量按官方价格表估算每个账号的 5h / 周 / 月 成本。
- **内置浏览器**：每个账号可打开独立隔离的 WebView2 窗口访问 OpenCode-Go 控制台，用于登录、充值、复制 key。
- **系统托盘**：后台常驻；关闭窗口后驻留托盘。
- **本地优先**：所有数据存于本地 SQLite 文件；无遥测、无远程服务器。

---

## 面向使用者

### 环境要求

- Windows 10 或 11，且已安装 **WebView2**（现代 Windows 多数预装；否则从微软官网下载）。
- 一个或多个 OpenCode-Go API key。

### 安装

从 Releases 页面下载最新的 NSIS 安装包并运行。安装为「每机器」级别，会创建开始菜单条目和桌面快捷方式。卸载时会询问是否同时删除数据目录。

如需从源码构建，请见 [面向维护者 → 从源码构建](#从源码构建)。

### 首次使用

1. 启动 **OCG Manager**，默认进入 **仪表盘**，Gateway 随应用自动在端口 `9042` 启动。
2. 进入 **账号管理** → **添加账号**。填写名称（如 `主号`）并粘贴 OpenCode-Go API key。key 在写入磁盘前会被混淆处理。
3. 同样方式添加更多账号，启用希望 Gateway 使用的账号。
4. 在 **仪表盘** 或 **设置** 中复制你的 **Gateway Key**（格式 `ocg-xxxxxxxx-xxxxxxxx`）。该 key 在首次启动时自动生成，可随时重新生成。

至此，Gateway 已在 `http://127.0.0.1:9042/v1/` 运行。

### 使用 Gateway

将任意 OpenAI 兼容客户端指向 Gateway，并携带你的 Gateway Key：

```bash
curl http://127.0.0.1:9042/v1/chat/completions \
  -H "Authorization: Bearer ocg-xxxxxxxx-xxxxxxxx" \
  -H "Content-Type: application/json" \
  -d '{"model":"glm-5.2","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

```python
from openai import OpenAI

client = OpenAI(base_url="http://127.0.0.1:9042/v1", api_key="ocg-xxxxxxxx-xxxxxxxx")
stream = client.chat.completions.create(
    model="glm-5.2",
    messages=[{"role": "user", "content": "hello"}],
    stream=True,
)
for chunk in stream:
    print(chunk.choices[0].delta.content or "", end="")
```

注意事项：

- Gateway **只**实现 `POST /v1/chat/completions`。其他 OpenAI 端点（`/models`、`/embeddings` 等）不会被路由。
- 请求体原样透传——工具调用、JSON 模式、temperature、`stream` 等都正常工作。
- 客户端的 `Authorization` 头用于 Gateway 鉴权后会被**替换**为所选账号的 OCG key 再转发上游；其他请求头不会被转发。
- SSE 流式响应按字节原样透传。
- Gateway 仅绑定 `127.0.0.1`，局域网/公网不可达。

### 账号选择与故障转移

在 **设置** 中选择策略：

| 策略 | 行为 |
|------|------|
| 顺序 | 按账号列表顺序取第一个可用账号。 |
| 随机 | 在可用账号中随机取一个。 |
| 轮询 | 循环使用可用账号。 |

「可用」指已启用 **且** 不在熔断冷却中。若选中账号请求失败（4xx/5xx/超时），Gateway 会排除该账号并尝试下一个可用账号，最多 5 次。若全部失败，返回 `502` 并附带最后一次错误信息。

### 熔断策略

每个账号独立累计错误。当错误在某个时间窗口内累积到阈值，账号进入冷却并被选择器跳过。

| 触发条件 | 冷却时间 | 含义 |
|---------|---------|------|
| 30 分钟内 6 次错误 | 5 分钟 | 多为临时限流或网络抖动 |
| 5 小时内 6 次错误 | 1 小时 | 5h 额度大概率耗尽 |
| 24 小时内 6 次错误 | 1 天 | 周额度大概率耗尽 |
| 7 天内 7 次错误 | 本月熔断 | 月额度大概率耗尽 |

- 任意**成功**请求都会清零错误计数。
- 短/中/长冷却到期后自动恢复。
- **本月熔断**不会自动恢复——请在账号卡片上点 **重置熔断**，或充值后重置。

### 用量与费用

**仪表盘** 显示今日/本周/本月全账号总成本；每个账号卡片显示该账号的 5h/周/月 用量。成本根据每条响应中的 token 数量按 OpenCode-Go 价格表估算：

| 模型 | 输入 $/M | 输出 $/M | 缓存读取 $/M |
|------|---------:|---------:|-------------:|
| glm-5.2 | 1.40 | 4.40 | 0.26 |
| glm-5.1 | 1.40 | 4.40 | 0.26 |
| kimi-k2.7-code | 0.95 | 4.00 | 0.19 |
| kimi-k2.6 | 0.95 | 4.00 | 0.16 |
| deepseek-v4-pro | 1.74 | 3.48 | 0.0145 |
| deepseek-v4-flash | 0.14 | 0.28 | 0.0028 |
| mimo-v2.5 | 0.14 | 0.28 | 0.0028 |
| mimo-v2.5-pro | 1.74 | 3.48 | 0.0145 |

OpenCode-Go 额度窗口供参考：**5h = $12**、**每周 = $30**、**每月 = $60**。应用会展示你的花费对照这些额度，但**不**强制限制。

> ⚠️ **流式请求不计入用量统计。** 只有非流式响应包含可解析的 `usage` 字段，因此流式补全会以 `0` token、`$0.00` 记录。若依赖用量数据，请优先使用非流式调用，或将流式用量视为少计。

### 内置浏览器

在任意账号卡片上点 **打开浏览器**，会打开一个独立隔离的 WebView2 窗口访问 OpenCode-Go 控制台。每个账号使用各自的 profile 目录，cookies/会话互不干扰。同一时间只允许打开一个浏览器窗口——打开新的会自动关闭前一个。

可用于登录、查看额度、充值或复制 key，无需离开应用。

### 系统托盘

- 应用驻留 Windows 托盘。**左键**点击托盘图标（或右键菜单 → **打开管理界面**）显示窗口。
- 关闭窗口会**隐藏**到托盘——应用和 Gateway 继续运行。
- 右键菜单：**打开管理界面** / **查看网关状态** / **退出**。用 **退出** 才会真正停止 Gateway 并退出。

### 数据与安全

- **数据目录**：`%USERPROFILE%\.ocg-mgr\`——包含 `data.sqlite`（账号、日志、熔断状态、设置）和 `profiles/<account-id>/`（每个账号的 WebView2 数据）。卸载程序会询问是否删除。
- **key 存储**：API key 在写入 `data.sqlite` 前会经过机器绑定的混淆变换。这能阻止随意的明文窥探，但**并非**强加密——不能替代密钥保险库。任何能读取你的用户配置文件并获得该程序的人都能还原 key。
- **Gateway Key**：用于访问 Gateway 的本地密钥。请妥善保管；任何拿到它（且能访问本机）的人都可消耗你的账号额度。
- **网络**：Gateway 仅绑定 `127.0.0.1`。在未自行增加鉴权与 TLS 的情况下，请勿将其隧道暴露到公网。
- **无遥测**。除向 OpenCode-Go 上游转发你的请求外，不向任何服务器发送数据。
- **合规**：应用不会自动注册账号、自动充值或绕过验证码；邀请码仅用于展示/复制。

### 常见问题

**仪表盘显示 $0 但我一直在用。** 你大概率用了 `stream: true`。见 [流式请求注意事项](#用量与费用)——流式用量不会被记录。

**在设置里改了端口，但客户端仍连 9042。** 请将客户端的 `base_url` 更新为新端口。端口改动需 **重启 Gateway** 后生效。

**某账号卡在「本月熔断」。** 充值后在账号卡片上点 **重置熔断**。

**能在局域网或其他机器上用吗？** 出厂不支持——Gateway 绑定 `127.0.0.1`。如有必要可用本地隧道（如 `ngrok`、`cloudflared`），并自行评估安全风险。

### 已知限制

- 仅实现 `/v1/chat/completions`；无 `/models`、`/embeddings`，也不做 Anthropic/Gemini 协议转换。
- 流式请求不计入用量/费用统计。
- 账号卡片上的 **测试** 按钮仅校验存储的 key 能否解密并显示掩码预览，**不会**向上游发送探测请求。
- **开机自启**开关已保存但尚未接入操作系统。
- 额度窗口仅展示，不强制。
- 仅支持单个 Gateway Key（多 key 为后续规划）。

---

## 面向维护者

### 从源码构建

前置要求：

- **Node.js** v20+（v24 实测可用）
- **Rust** ≥ 1.85（edition 2024，MSRV 在 `src-tauri/Cargo.toml` 中强制）
- **Tauri CLI**——已作为开发依赖包含，`npx tauri ...` 无需全局安装即可使用。
- Windows 10/11，已装 WebView2。

```bash
# 安装前端依赖
npm install

# 仅前端开发服务器（http://127.0.0.1:30001）
npm run dev

# 完整 Tauri 应用（前端 + Rust + 桌面窗口）
npx tauri dev

# 类型检查 + 前端生产构建
npm run build        # = vue-tsc && vite build

# Rust 检查 / Release 二进制
cd src-tauri && cargo check
cd src-tauri && cargo build --release

# Windows NSIS 安装包 → src-tauri/target/release/bundle/nsis/
npx tauri build
```

### 项目结构

```
.
├── assets/logo/                # logo 源文件 + 生成脚本
├── docs/{en,zh}/               # logo 使用规范 + 快速开始说明
├── src/                        # Vue 3 + TypeScript 前端
│   ├── api/tauri.ts            # invoke() 封装 + 共享 TS 类型
│   ├── views/                  # 仪表盘 / 账号 / 日志 / 设置
│   ├── App.vue                 # 外壳 + 侧边栏导航
│   ├── main.ts                 # Vue + naive-ui 启动
│   └── styles/main.css
├── src-tauri/                  # Rust + Tauri 后端
│   ├── capabilities/default.json   # 主窗口的 Tauri v2 权限
│   ├── icons/                  # 32/128/256/512 + icon.ico
│   ├── installer.nsh           # NSIS 钩子：卸载时询问是否删除数据目录
│   ├── src/
│   │   ├── commands/           # Tauri 命令处理（account/setting/gateway/log/dashboard/browser）
│   │   ├── gateway/            # Axum Gateway（mod/handler/forwarder/selector/circuit_breaker/cost）
│   │   ├── crypto.rs           # 机器绑定的 key 混淆
│   │   ├── db.rs               # SQLite 打开 + 迁移 + 查询
│   │   ├── models.rs           # serde 结构体、AppConfig、枚举
│   │   ├── state.rs            # AppState、配置读写、gateway-key 生成
│   │   ├── tray.rs             # 系统托盘初始化
│   │   ├── lib.rs              # run()——串联 DB、Gateway、托盘、命令
│   │   └── main.rs             # 可执行入口 → run()
│   ├── Cargo.toml
│   ├── build.rs
│   └── tauri.conf.json
├── Cargo.toml                  # workspace 根（members = src-tauri）
├── package.json
├── vite.config.ts              # 开发端口 30001、@ 别名
├── tsconfig.json
└── index.html
```

### 架构

```text
客户端（OpenAI SDK / curl / 任意工具）
  │  POST http://127.0.0.1:9042/v1/chat/completions
  │  Authorization: Bearer <gateway-key>
  ▼
Tauri 应用（Rust + WebView2）
  ├── WebView2 UI（Vue 3 + naive-ui）──invoke()──► Tauri 命令
  ├── 系统托盘（tray.rs）
  └── 内嵌 Axum Gateway（gateway/）
        ├── 鉴权：校验 Bearer <gateway-key>
        ├── AccountSelector：按策略选出已启用且熔断健康的账号
        ├── Forwarder：用该账号的 OCG key 将请求体转发到 {upstream}/v1/chat/completions
        │     ├── SSE 流式透传 ─► 客户端（字节不变）
        │     └── JSON 响应    ─► 解析 usage、估算成本、写日志
        ├── CircuitBreaker：按账号记录成功/失败，升级冷却
        └── DB（rusqlite）：accounts、settings、logs、circuit_states
                ▲
        失败时：排除该账号，重试下一个（≤5 次）→ 全部失败返回 502
```

### 关键实现

- **Gateway 装配**——`gateway/mod.rs` 构建 Axum 路由（`/v1/chat/completions` + 宽松 CORS），绑定 `127.0.0.1:<port>`，并以 graceful-shutdown oneshot 运行。`start_gateway` / `stop_gateway` 在 `lib.rs`（启动）和 `commands/gateway.rs`（重启）中调用。
- **请求处理**——`gateway/handler.rs` 校验 gateway key，随后循环最多 5 次：选账号（排除上一次失败的）、转发、成功即返回，失败则排除并重试。返回 `401` / `503`（无可用账号）/ `502`（全部失败）。
- **转发器**——`gateway/forwarder.rs` 解密账号 key，将**原始请求体** POST 到 `{upstream_base_url}/v1/chat/completions`（120s 超时），随后要么按字节透传 SSE 流，要么解析 JSON `usage` 估算成本。每次尝试都写入 `forward_logs`，并用熔断器记录成功/失败。
- **选择器**——`gateway/selector.rs` 列出账号，过滤掉已禁用/被排除/熔断不可用的，再按策略挑选（顺序=第一个；随机=纳秒时间索引；轮询=原子计数器）。
- **熔断器**——`gateway/circuit_breaker.rs`。`ERROR_THRESHOLD = 6`、`MONTHLY_THRESHOLD = 7`。`evaluate_level` 把错误数 + 时间窗口映射为等级；`is_available` 在 `cooldown_until > now` 或等级为 `MonthlyBlown` 时为 false。成功会清零计数。
- **成本**——`gateway/cost.rs` 持有价格 `HashMap`，规范化模型名（小写，分隔符→`-`），按 `cost = prompt/1e6·in + completion/1e6·out + cached/1e6·cache_read` 计算。未知模型先模糊匹配，再回退默认价。
- **加密**——`crypto.rs`。以 `USERNAME` / `COMPUTERNAME` / `APPDATA` 为种子的机器绑定 XOR 混淆。文档明确**非**密码学安全；如需真实保密请替换为 `aes-gcm` / KMS。
- **DB**——`db.rs`。SQLite 位于 `<data_dir>/data.sqlite`，通过 `schema_version` 版本化。表：`accounts`、`settings`、`gateway_logs`、`forward_logs`、`circuit_states`。索引在 `forward_logs(timestamp)` 与 `forward_logs(account_id)`。
- **状态**——`state.rs`。`AppState`（Arc）持有 `Mutex<Database>`、`Mutex<AppConfig>`、`Mutex<Option<GatewayHandle>>`、`Mutex<Option<浏览器窗口 label>>`。配置以 JSON 序列化到 `settings` 表的 `config` 键；gateway key 在首次启动时自动生成为 `ocg-<word>-<word>`。
- **托盘**——`tray.rs`。左键显示主窗口；右键菜单含 打开/状态/退出。`lib.rs` 拦截 `CloseRequested` 改为隐藏而非关闭。
- **Tauri 命令**——在 `lib.rs::invoke_handler!` 中注册；类型化封装在 `src/api/tauri.ts`。新增命令需两处都改。

### 数据模型

`accounts(id, name, key_cipher, enabled, referral_code, recharge_date, created_at, updated_at)`
`settings(key, value)`——应用配置以 JSON 存于 `key = "config"`
`gateway_logs(id, level, category, message, created_at)`
`forward_logs(id, timestamp, model, account_id, account_name, status, http_status, prompt_tokens, completion_tokens, cached_tokens, cost, error_message)`
`circuit_states(account_id, consecutive_errors, first_error_at, last_error_at, cooldown_until, level)`

### 配置

`AppConfig`（`models.rs`，默认值如下）：

| 字段 | 默认值 | 说明 |
|------|--------|------|
| `gateway_port` | `9042` | 本地 Gateway 绑定端口。 |
| `gateway_key` | 自动 `ocg-xxxxxxxx-xxxxxxxx` | 可在设置中重新生成。 |
| `selection_strategy` | `sequential` | `sequential` / `random` / `round_robin`。 |
| `upstream_base_url` | `https://api.opencode.ai` | OpenCode-Go API 基址。 |
| `auto_start` | `false` | 仅保存——尚未接入 OS 开机自启。 |

可在应用内（设置）编辑，或直接改 `settings` 表。改端口后需重启 Gateway。`tauri.conf.json` 的 `security.csp` 硬编码了 `connect-src http://localhost:9042`；若 webview 需要连其他端口请同步修改（正常情况下 Gateway 由外部客户端访问，webview 很少直连，故此条多数时候无影响）。

### 扩展指南

- **新增价格表模型**——在 `gateway/cost.rs::price_table()` 加条目。名称会被规范化（小写，` /_.`→`-`），故可宽松匹配上游 model id。
- **调整熔断阈值**——`gateway/circuit_breaker.rs`：`ERROR_THRESHOLD`、`MONTHLY_THRESHOLD`，以及 `evaluate_level` 中的各窗口。
- **修改默认端口/上游**——`models.rs::AppConfig::default()`。
- **新增 Tauri 命令**——在 `src-tauri/src/commands/` 下写 `#[tauri::command]` 函数，在 `lib.rs::invoke_handler!` 注册，并在 `src/api/tauri.ts` 加类型化封装。

### 测试

- `cd src-tauri && cargo test`——目前仅有 crypto 往返测试。
- 无前端测试。建议补充：选择器策略、熔断状态机、成本估算，以及对接 mock 上游的集成测试。

### 打包与发布

- `npx tauri build` 产出每机器级 **NSIS** 安装包，位于 `src-tauri/target/release/bundle/nsis/`。
- Release 配置（`Cargo.toml`）：`opt-level = 3`、`lto = true`、`strip = true`、`panic = "abort"`——生成单个 stripped exe。
- `installer.nsh` 增加卸载钩子，删除 `%USERPROFILE%\.ocg-mgr` 前会询问用户。
- 未配置 CI/CD 或自动发布；手动切版本发布。

### 已知缺口 / TODO

- 除 `crypto` 往返外无自动化测试。
- 流式请求日志记录 0 token/0 成本（用量统计仅覆盖非流式）。
- `test_account` 不探测上游——仅解密并掩码展示 key。
- `auto_start` 标志未接入 OS 自启条目（未注册 `tauri-plugin-autostart`）。
- 按充值日期自动重置月熔断未实现（仅支持手动重置）。
- `reqwest::Client` 每请求新建——无连接池复用。
- 加密是混淆，非 AEAD。
- `tauri-plugin-store` 已在 npm 端声明但 Rust 端被注释掉。

---

## 许可证

MIT
