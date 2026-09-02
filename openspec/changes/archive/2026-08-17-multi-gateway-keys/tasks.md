## 1. 阶段一：存储结构与配置迁移（后端）

- [x] 1.1 `models.rs` 新增 `GatewayKeyEntry`（id/name/key/enabled/deleted_at/created_at）与 `AppConfig.gateway_keys: Vec<GatewayKeyEntry>`（`#[serde(default)]`），`gateway_key` 字段保留；补 serde 兼容单测（旧 JSON 缺字段→空 vec、round-trip、镜像字段）
- [x] 1.2 `state.rs::load_config`：`gateway_keys` 为空且旧 `gateway_key` 非空时，生成主 key 条目（name="Primary"）并置 `needs_persist`；为空时生成新 key；`gateway_key` 始终镜像主 key 值（含删主 key 后的提升同步）
- [x] 1.3 `state.rs::set_config`：routing reset 触发条件改为"曾有启用 key 值失效"（regenerate/停用/删除触发；改名、纯新增 key 不清粘性会话，避免无谓扰动）
- [x] 1.4 新增 `GatewayKeyStore` 或等价辅助：key CRUD、启用/停用、软删除（置空明文值）、按 id 反查名称、唯一值校验（所有未删除 key 值唯一）、主 key/最后启用 key 不变量校验（单测覆盖不变量），主 key 删除后剩余最早启用 key 提升并同步镜像

## 2. 阶段一：鉴权多 key 匹配（后端）

- [x] 2.1 重构 `check_auth` → `extract_client_key_id(headers, config) -> Option<String>`：遍历启用且未删除的 key，支持 Bearer / x-api-key / x-goog-api-key
- [x] 2.2 替换 handler.rs 中全部现有调用点（handler.rs:37 中间件、135 claude_desktop_models、238 models、286 proxy_handler_inner、386 gemini_proxy_handler，及 helper 内部 gemini_error/gemini_expected_fallback 两处，共 7 处），统一走新 helper
- [x] 2.3 更新/新增单测：多 key 匹配、停用拒绝、软删除拒绝、未知拒绝、空值拒绝、三种头（Bearer/x-api-key/x-goog-api-key）× 多 key × 状态矩阵

## 3. 阶段一：key 管理 API（后端）

- [x] 3.1 `dashboard.rs` 新增 `POST /settings/keys`（创建，返回完整值仅此一次）、`PATCH /settings/keys/{id}`（改名/启停，拒绝空 name）、`POST /settings/keys/{id}/regenerate`（返回新值）、`DELETE /settings/keys/{id}`（软删除并清除明文值），全部走 `settings_update` 锁 + revision 乐观锁；**每个变更操作写 gateway_logs 审计条目**
- [x] 3.2 `GatewayStatus` 增加 `keys` 与 `primary_key_id`，保留 `key` 字段（= 主 key 值）
- [x] 3.3 `update_settings` 忽略请求中的 `gateway_key/gateway_keys`（key 只经 key API 管理，文档化该语义变化），且必须保留 `config.gateway_keys = previous_config.gateway_keys.clone()`（防 `serde(default)` 缺字段清空 keys 导致全线 401）；保留"gateway_key 非空"最小守卫防空/残缺 JSON 静默重置其它字段；`set_config` 统一强制"主 key 非空、≥1 启用 key、gateway_key 镜像主 key"不变量；`/settings` 返回 keys 列表
- [x] 3.4 保留旧端点 `POST /settings/regenerate-gateway-key` 并收敛为"重新生成主 key"；路由表注册新端点；API 层单测/集成测试（含"删除最后启用 key 被拒""删除主 key 后提升""缺 gateway_keys 的 POST /settings 不清空 keys"用例）

## 4. 阶段一：前端 settings 与 dashboard

- [x] 4.1 `tauri.ts`：`AppConfig`/`GatewayStatus`/`GatewayKeyEntry` 类型、key CRUD API 封装
- [x] 4.2 `settings-merge.ts`：确认 key 字段继续排除在可编辑列表外，并补一条测试断言 `gateway_keys` 不在 `EDITABLE_SETTING_KEYS`
- [x] 4.3 `Settings.vue`：新增"接入 Key"管理区（列表、新建、改名、启停、重新生成、删除确认）
- [x] 4.4 `Dashboard.vue` 接入中心：key 选择器 + 当前 key 掩码展示/复制 + 单 key 重新生成确认文案（"仅当前 Key 失效"）；`dashboard-connection.ts` 草稿失效/掩码还原逻辑适配（含其测试文件）
- [x] 4.5 `Applications.vue`：指南复制改为主 key 值
- [x] 4.6 i18n 新增文案：编辑 9 个翻译文件（en-US 及 zh-TW/ja-JP/ko-KR/es-ES/fr-FR/de-DE/pt-BR/ru-RU；zh-CN 由 en-US key 恒等派生无需单独文件）；`src/i18n` 相关测试更新

## 5. 阶段一：CLI 与收尾

- [x] 5.1 `ocg-cli` 状态输出展示主 key（值不变）
- [x] 5.2 既有测试全量适配（core_state / dashboard_auth / handler 单 key 断言）；`cargo test` + `pnpm test` 通过

## 6. 阶段二：forward_logs 埋点与查询（后端）

- [x] 6.1 `db.rs` schema v18：`ensure_column(forward_logs, client_key_id, TEXT)`、`ensure_column(forward_logs, client_key_name, TEXT)`、`idx_forward_logs_client_key` 索引
- [x] 6.2 `ForwardAttemptContext` 新增 `client_key_id: Option<String>`；`proxy_handler_inner` 鉴权后取得 key id，沿 `execute_plan` → `forward_request` → `forward_request_impl` 参数链传入，在 `ForwardAttemptContext::new` 写入
- [x] 6.3 `log_forward` 落库 `client_key_id` 与写入时刻的 `client_key_name`（含按 id 反查启用/软删除 key 名称、查不到为 None 的逻辑）；`ForwardLog` 模型与 `forward_log_from_row`/`list_forward_logs`/`query_forward_logs` SELECT 列同步加字段
- [x] 6.4 `ForwardLogQueryOptions` 增加 `key_id`（`__unattributed__` 特值 = `client_key_id IS NULL`）；`forward_log_filter` 与 `query_forward_logs` SQL 及 summary 支持按 key 过滤
- [x] 6.5 新增 `list_forward_log_client_keys`（`SELECT DISTINCT client_key_id, client_key_name FROM forward_logs`，镜像 `list_forward_log_models` 模式 db.rs:1353）+ `/logs/forward/keys` 端点，作为 Logs 页 key 筛选下拉的数据来源（覆盖有日志的全部 key 含已停用/已软删除/悬空 id）
- [x] 6.6 埋点与查询单测：归因正确、未认证不落 forward 日志、key 过滤与汇总、未归因特值、`/logs/forward/keys` 去重正确

## 7. 阶段二：大表安全回填

- [x] 7.1 回填任务在**构造完成后的 runtime 上下文**启动，实现收敛为挂在 `start_gateway_on`（桌面/CLI/端口重启的唯一收口，未来调用点不会漏跑）。实现期修订：原计划分别挂 Tauri `setup` hook / CLI async main 以规避测试 spawn 竞争，改为两层缓解——①首块内联：空表/小表在启动调用栈内同步完成并写完成标记，不 spawn 线程；②大表才 spawn 只持 `Weak` 引用的专用 `std::thread`（state drop 后 `upgrade()` 失败即退出，与测试临时目录清理不竞争）。主 key id 与名称在取锁前从 config 快照读取；纯同步构造（单测）下不启动、不 panic
- [x] 7.2 按 rowid 分块（块大小常量可调，如 50k/块）`UPDATE ... WHERE client_key_id IS NULL AND rowid BETWEEN ? AND ?`，每块独立事务、块间让出 CPU；**持 `state.db` 锁期间不得 `.await`**
- [x] 7.3 幂等/断点续跑：settings 表记录已回填最大 rowid（`backfill_forward_logs_client_key`），启动从断点续跑；无 NULL 行即完成并记录完成标记；写入路径保证新行必带 key id
- [x] 7.4 回填期间网关可用性验证（已认证请求日志写入短暂排队、未认证不受影响）；日志查询对 NULL 行走"未归因"展示
- [x] 7.5 回填测试：中断续跑不重复、断点 watermark、块边界、大表（构造较大 fixture）不阻塞、持锁不跨 await

## 8. 阶段二：Logs 页按 key 查询（前端）

- [x] 8.1 `Logs.vue`：key 筛选下拉（数据来自 `/logs/forward/keys`，含已停用/已软删除/悬空 id 的 key + 未归因选项）、筛选联动 summary 统计
- [x] 8.2 升级提示文案（"升级前用量统一计入主 Key"）与 i18n
- [x] 8.3 `logs.test.ts` / `accounts-usage` 相关测试适配与新增用例

## 9. 发布验证

- [x] 9.1 全渠道升级演练：从单 key 版本构建数据 → 升级新版本 → 主 key 迁移、旧客户端继续鉴权、历史日志回填正确
- [x] 9.2 降级演练：升级后创建次级 key → 降级旧二进制 → 断言主 key 值保留、次级 key 被移除（文档化预期）、config 不含 gateway_keys；再升级 → 断言历史日志悬空 id 在 Logs 页不崩溃（兜底展示）
- [x] 9.3 Docker 镜像升级路径验证（compose 卷内既有 `data.sqlite`），升级前停止旧容器；多进程并发启动场景断言为可自愈 `SQLITE_BUSY`（可选：`Database::open` 设 `busy_timeout` 消除）
- [x] 9.4 用户文档更新：USER.md / USER.zh-CN.md 补充多 key 用法、升级说明与降级限制（README 按仓库规范仅作落地页，不承载能力文档，无需改动）
- [x] 9.5 回填大表冒烟（自动化集成测试）：种入超过一个分块（50k + 5k 行）的 forward_logs，验证回填期间网关持续可服务（未认证不受影响、已认证日志写入仅短暂排队）、回填收敛到完成标记、无未归因残留
- [ ] 9.6 发布前人工验证：≥100 万行 forward_logs 升级演练，量化首次启动耗时（索引创建 + 清理 + 回填）与回填期间已认证请求 P95 延迟增量，给出启动超时上限
- [ ] 9.7 发布前人工验证：桌面自动更新通道演练（经 updater 而非覆盖安装升级），断言 `~/.ocg-mgr` 内既有 `data.sqlite` 迁移后 config 含 `gateway_keys`、数据 sentinel 保留
- [ ] 9.8 发布前人工验证：迁移/回填各阶段 kill 后重启注入，断言 schema_version 与 config 幂等收敛（schema migrate 重放与回填断点续跑已由自动化测试覆盖：`v18_migration_is_idempotent_and_crash_replay_safe`、watermark 断点用例）
- [x] 9.9 CI 冒烟对 CLI `status` 输出追加主 key 断言（当前 release 冒烟未检查 gateway key 输出，防 D10 展示层回归）
