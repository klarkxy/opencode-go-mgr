[English](conventions.md)

# 编码约定

- **保持 crate DAG**。domain 与 gateway 保持无 I/O。门面按条目再导出。适配器返回 `AttemptSpec`。`forward_once` 是一次上游调用。Dashboard V3 不导入 `gateway`。
- **前端不新增 Tauri `invoke()` 路径**。Vue 主数据路径是 HTTP `/dashboard/api/v3`；不注册 `generate_handler`。
- **受保护的 V2 REST 保持退役状态**。新 JSON 属于 V3。410 墓碑挡在已退役 `/dashboard/api/...` 路径前面。
- **安全边界不能为了简化而削弱**。Gateway 鉴权、Key 混淆、URL 校验、冷却写入、SSE 透传以及 ConnectionInfo 密钥边界均不可移除。
- **不引入远端同步**。每个节点由自己的面板管理。
- **`auto_start` 与 `show_dock_icon` 受能力门控**。Windows x64、macOS 和 Linux x64 的 release / 已安装 Tauri 进程注入登录自启同步钩子；Dock 仅 macOS Tauri。
- **本地 Alias 列表保持本地**。带鉴权的 `GET /v1/models` 与面板 `application-models` 不在请求时增加上游发现。供应商页上的显式 Zen Free 刷新是唯一目录抓取例外，且只访问固定官方 endpoint。两份列表保持独立；不发明 `requested_alias` 日志字段。
- **尊重 `parking_lot::Mutex` 不可重入**。CLI 与 core 均使用。函数需要调用另一个持锁函数时，先 `drop` 外层 guard。
- **风格与周围一致**。注释密度、命名、惯用法跟现有代码保持一致。

## 文档归属与编辑

- 当前行为以代码为准。修改运行时事实前，先按 `AGENTS.md` 中对应的权威来源指针
  核对；不要把本页扩写成第二份项目事实清单。
- 根 README 是落地页。详细能力、转换与集成材料放在 `docs/user/` 的对应章节；维护
  流程放在 `docs/maintainer/`。
- 用户可见工作流属于成对的 `docs/user/*.md` 和 `*.zh-CN.md` 指南。保持标题结构、
  链接与 TOC 锚点在两种语言间同步。
- `DESIGN.md` 与 `src/theme.ts` 负责视觉 token 和面向用户的 **Key** 名称。包清单与
  `compose.example.yaml` 负责发行版本钉；同一次发版变更中同步更新相应 Docker 示例。
- 只描述当前行为和明确限制。已知缺口放入 `docs/user/limits.md` 或
  `docs/maintainer/known-debt.md`；只有实际运行过相应检查，才能声称浏览器、可能
  计费的推理或已安装桌面版可用。
- 文档索引只做读者路由。事实归属和编辑约定放在本页，不再放进另一张很长的索引表。
- 仓库文档、`AGENTS.md` 与 `.agents/skills/` 只记录共享的项目事实和维护流程。
  个人模型选择、智能体分工、回复风格、审批偏好和本机工具路径放在用户级配置中，不写入仓库指令。
  仓库 skill 应能在没有某位维护者私有插件或智能体角色配置的环境中使用。

---

[维护者指南索引](../MAINTAINER.zh-CN.md) · [English](conventions.md) · [文档索引](../README.zh-CN.md)
