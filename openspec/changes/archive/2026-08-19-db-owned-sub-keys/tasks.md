## 1. 回滚 config 内嵌形态（后端）

- [x] 1.1 `models.rs`：移除 `GatewayKeyEntry` 与 `AppConfig.gateway_keys` 字段；**新增**主 Key "trim 后非空"校验到 `AppConfig::validate`（现状该校验仅在 update_settings payload 守卫，validate 本不含——勿按"确认在"理解成已有）；确认与 `load_config` 空 key 自动铸新路径兼容（铸新先于 validate，不冲突）+ 单测（空值被 set_config 拒绝）；补 serde 单测（旧 JSON 含 `gateway_keys` 时字段被忽略、`gateway_key` 保留）
- [x] 1.2 `state.rs`：删除 config 内 key 列表迁移/normalize 调用/镜像不变量；routing reset 条件改为"主 Key 值变化（set_config 原有逻辑）或子 Key 撤销类变更由端点显式触发"（改名/新增不清粘性会话，与 PR #43 修复后语义一致）
- [x] 1.3 `dashboard.rs`：`update_settings` 恢复 v1.6.1 的 `gateway_key` 可设置语义（trim + 非空守卫 + **拒绝与任何非删除子 Key 相同的值（含已禁用）**，经统一闸口 helper 与 Tauri `update_settings_inner`、子 Key 启用路径复用同一互斥检查），移除 keys 防清空覆盖；校验错误串 "gateway key is required" 顺带改为 "key is required"（DESIGN.md 命名合规）；补断言"POST /settings 携带子 Key 字段 → `sub_gateway_keys` 表不变"；适配既有单测（settings trim/require gateway key 用例恢复）
- [x] 1.4 `models.rs` `GatewayStatus` 移除 PR #43 增加的 `keys`/`primary_key_id`（保留 `key`）；`src-tauri/src/commands/gateway.rs`、`setting.rs` 同步适配构造（不改对外行为）
- [x] 1.5 删除/改写 PR #43（提交 `dc42da5`）引入的键测试（按文件与测试组定位，行号以提交为准）：`models.rs` 的 `legacy_config_json_without_gateway_keys_*`/`gateway_keys_round_trip_*`/`gateway_key_entry_defaults_*` 全删；`gateway_keys.rs` 的 create/rename/disable/delete/regenerate/normalize 等 config 形态单测全删（DB 形态由 2.x 新测替代）；`state.rs` 的单 key 迁移用例全删、`routing_resets_only_when_enabled_key_values_change` 按新触发条件改写；`gateway/handler.rs` 鉴权矩阵按新签名改写；`tests/dashboard_auth.rs` 的 key 生命周期集成测试改写为子表 API 形态。保留仍适用的（regenerate 主 Key、409 乐观锁框架）

## 2. 子 Key 存储与快照（后端）

- [x] 2.1 `db.rs` schema v19：建 `sub_gateway_keys` 表 + 活跃值部分唯一索引 + 活跃数量查询；迁移幂等测试（重放收敛）
- [x] 2.2 `db.rs` CRUD：list（含墓碑）/ by_id / insert / rename / set_enabled / soft_delete（清明文）/ regenerate 值；名称与数量上限（≤64 活跃，墓碑不计）校验；生成循环避开主 Key 当前值（跨层唯一）；**启用路径校验值 ≠ 主 Key 当前值**；单测覆盖唯一索引兜底、上限拒绝、跨层避让与"禁用 → 主 Key 同值 → 重新启用被拒"绕过序列
- [x] 2.3 `gateway_keys.rs` 重写为子 Key 门面：DB CRUD 封装 + 凭证快照 `RwLock<HashMap<value, (id, name)>>`（**含主 Key 条目**（name="Primary"），`set_config` 后刷新主 Key 值；鉴权与日志写入名称快照共用）；快照重建顺序硬规则——撤销类（禁用/软删/轮换）**先改快照再提交表写**（fail-closed，表写失败回滚快照），创建/启用类**先提交再重建**（fail-open 仅延迟生效），两步均持 `settings_update` 锁，崩溃由重启重建自愈；**回滚快照自身失败的兜底**——`eprintln!` 告警 + 端点返回 500，下一次任意 Key API 操作入口处（已持锁）先从 DB 重建快照，启动加载为天然自愈点；快照 `RwLock` 纳入 state.rs 锁序文档；快照代码处注释声明失效模型（表仅由 Key API 写入、config 仅经 set_config 写入，外部改动重启前不生效，按决策忽略）
- [x] 2.4 常量 `PRIMARY_KEY_ID`（硬编码 UUID，实现时定值；式样选可识别固定形态如 `00000000-0000-0000-0000-000000000001`，与生成的 v4 UUID 及 nil 视觉区分；注释标注"发布后保持稳定，直至显式迁移"）；主 Key 名称快照/显示固定 "Primary"（UI 用 i18n "主 Key"）

## 3. 鉴权（后端）

- [x] 3.1 `gateway/handler.rs`：`extract_client_key_id` 改为收集全部非空候选头（Bearer/x-api-key/x-goog-api-key 固定顺序）× 快照任一命中即通过；归因顺序定案——按候选头顺序、主 Key（快照条目）先于子 Key，首个命中即返回其 id（主 Key 返回 `PRIMARY_KEY_ID`）
- [x] 3.2 更新鉴权矩阵单测：三头 × {主 Key, 启用/停用/软删子 Key, 未知值} × 组合场景（错误 x-api-key + 正确 x-goog-api-key 必须通过）
- [x] 3.3 确认全部调用点（中间件/models/claude-desktop/proxy/gemini 等 7 处收口）在新签名下编译与行为正确；**`forwarder.rs::set_client_key` 的名称解析改走快照**（现状从内存 config 查 `key_name`，子 Key 移 DB 后断源；快照按 id 取 name，主 Key 恒 "Primary"）；未认证不落日志行断言保持

## 4. Key 生命周期 API（后端）

- [x] 4.1 `dashboard.rs`：`POST/PATCH/POST {id}/regenerate/DELETE /settings/keys*` 改为操作 `sub_gateway_keys`；持锁 + `expected_revision` 409 + 审计（rename 记旧名与新名）；创建返回完整值仅一次；撤销类操作（禁用/软删/轮换）后显式触发 `routing.reset()`（改名/新增不触发）；审计消息措辞遵循 DESIGN.md——只称 "key"（如 "created key `Laptop`"），不出现 "gateway key"
- [x] 4.2 新端点 `GET /dashboard/api/connection`（定案，不扩展 gateway/status）：主 Key 值 + 子 Key 列表（**id/name/enabled/value 明文**，与 `primary_key` 明文同层——切换器掩码预览在前端本地算、复制需完整值；"创建返回完整值仅一次"只约束 Key API 创建响应）+ revision + gateway_port/client_root_url/upstream_base_url；与 `/settings`、`/gateway/status` 同一 dashboard 会话保护层（会话 Cookie 或回环 local-mode）
- [x] 4.3 回填目标改 `PRIMARY_KEY_ID`：`gateway/mod.rs` 与 `core_state.rs` 相关调用；`/logs/forward/keys` 按 id 取最新名称（保持纯日志驱动，不合成常量条目；空名快照沿用现状"回填为 id"兜底）；相应单测（含重命名后显示新名）
- [x] 4.4 Tauri `regenerate_gateway_key_inner` 对齐主 Key 轮换；审计 category "keys" 保持，消息措辞同步去 gateway（"regenerated primary key `X`"）

## 5. 前端

- [x] 5.1 `api/tauri.ts`：移除 `AppConfig.gateway_keys`/`GatewayKeyEntry` 依赖；新增 `ConnectionInfo` 与子 Key 类型；Key API 与 `ConnectionInfo` 端点封装；`settings-merge.ts` 恢复 `gateway_key` 可编辑
- [x] 5.2 `Dashboard.vue`：`ref<AppConfig>` → `ConnectionInfo`；切换器默认主 Key、复制、单 Key 重新生成交互保持，`regenerateKey` 主/子 Key 分流（主走遗留端点）；断言"单凭证不渲染切换器（布局与 v1.6.1 一致）"与"重新生成成功但后续刷新失败时仍显示新值"两场景
- [x] 5.3 `Settings.vue`：Key 区双层——主 Key 行（只读 + 轮换，无启停/删除）、子 Key 行（改名/启停/轮换/软删）；`gateway_key` 输入恢复可编辑保存（与 Key 管理区行的"只读"不矛盾）；重构时顺手清理无模板引用的死 CSS（`.key-display/.key-editor/.key-stack/.key-field` 等 PR #43 残留）并归一 off-scale 圆角（8px→6px 刻度）
- [x] 5.4 `Logs.vue`：确认筛选/汇总在 `ConnectionInfo`/新 keys 端点下不回归；`Applications.vue` 复制主 Key 值路径确认
- [x] 5.5 i18n 与命名边界（范围显式化，与 design D7 一致）：`Dashboard.vue`/`Settings.vue`/`Logs.vue`/`Applications.vue` 及后端审计消息不出现 "gateway key" 字样（审计消息用 "created key `X`" 式措辞）；**范围外**——CLI stdout（`gateway key: ...`）、上游 401 错误串、URL 路径/表名/能力名维持现状；复用既有 Key 文案、清理"主 Key 可停用"类措辞（若存在），并删除 9 语言中无消费者的 "GATEWAY KEY" i18n key（en-US.ts L425 等）；9 语言 + zh-CN 派生一致性测试保持

## 6. 测试与收尾

- [x] 6.1 `cargo test --workspace` 全绿（重点：鉴权矩阵、子 Key 生命周期集成、回填 PRIMARY_KEY_ID、**回填 DONE 后注入 NULL 行→重启再归因**、主 Key 无启停/删除操作路径断言、v19 幂等、降级往返——子 Key 表在旧"形态"保存 settings 后不变（集成层断言，落 `tests/core_state.rs` 的升级/降级演练用例）、快照重建顺序（撤销类 fail-closed）；**HTTP `POST /settings/regenerate-gateway-key` 轮换断言**——旧值立即 401、新值通过（spec "Primary key rotation" 场景的 HTTP 面回归）
- [x] 6.2 `pnpm run test` 全绿（web + typecheck + build；`doesNotMatch(dashboard, /ref<AppConfig>/)` 断言恢复成立）
- [x] 6.3 手动冒烟：真实发送主/子 Key 请求各 2-3 个验证归因（参考 #43 验证方式）；Logs 按 Key 过滤显示最新名称
- [x] 6.4 文档：USER/USER.zh-CN 多 Key 章节按新模型改写（主 Key 恒有效、子 Key 独立、降级安全说明）；中英对等

## 7. 发布验证（人工，发布前执行）

- [ ] 7.1 大表升级演练（≥100 万行 forward_logs）：量化首次启动耗时与回填期间 P95 延迟增量
- [ ] 7.2 桌面 updater 通道演练：升级后 `sub_gateway_keys` 表与 config 主 Key 状态正确
- [ ] 7.3 迁移/回填各阶段 kill 注入：schema_version 与回填水位幂等收敛
