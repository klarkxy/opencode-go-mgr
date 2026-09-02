# Contributors / 贡献者

OCG Manager is maintained by [Klarkxy](https://github.com/klarkxy) and improved by community contributions. Thank you to everyone who reports issues, proposes changes, reviews code, and helps validate releases.

OCG Manager 由 [Klarkxy](https://github.com/klarkxy) 维护，也受益于社区的代码贡献、问题反馈、审查与发布验证。感谢每一位参与者。

## Community contributors / 社区贡献者

### [Clement (@moton16)](https://github.com/moton16)

- [#10](https://github.com/klarkxy/opencode-go-mgr/pull/10) (v1.4.1): MiniMax model pricing and corrected cache-hit cost calculations.
- [#11](https://github.com/klarkxy/opencode-go-mgr/pull/11) (v1.4.1): request-log filtering by model and time, multi-field sorting, and top-level summaries.
- [#12](https://github.com/klarkxy/opencode-go-mgr/pull/12) (v1.4.1): separated cooldown windows and reset countdowns on the account interface.
- [#22](https://github.com/klarkxy/opencode-go-mgr/pull/22) (v1.5.1): fixed MiniMax cache/usage windows under real traffic and improved the request-log time-range selector.
- [#36](https://github.com/klarkxy/opencode-go-mgr/pull/36) (v1.5.9): normalize the `developer` role to `system` when converting to Chat-only upstreams.
- [#44](https://github.com/klarkxy/opencode-go-mgr/pull/44) (v1.8.1): Muse Spark 1.2 and Contributor models — protocol registration, pricing, and application exposure, verified against a live Go account.

为 v1.4.1–v1.8.1 贡献了 MiniMax 计费补全与缓存窗口修复、请求日志筛选与排序、独立冷却窗口倒计时、Chat 上游 `developer` 角色归一化，以及 Muse Spark 1.2 / Contributor 模型接入等改进。

### [eveloki (@eveloki)](https://github.com/eveloki)

- [#43](https://github.com/klarkxy/opencode-go-mgr/pull/43) (v1.7.0): multiple client access keys with per-key usage attribution, including the database-owned `access_keys` storage redesign.
- [#46](https://github.com/klarkxy/opencode-go-mgr/pull/46) (v1.8.1): multi-architecture container releases — native `linux/amd64` and `linux/arm64` builds merged into one OCI index per tag.
- [#48](https://github.com/klarkxy/opencode-go-mgr/pull/48): per-model list proxy routing, the fourth outbound proxy mode (allowlist/denylist by model).
- [#53](https://github.com/klarkxy/opencode-go-mgr/pull/53): fixed cached-token totals in request logs so cache reads are not counted twice.

贡献了多客户端 Key 与按 Key 用量归因（含 `access_keys` 数据库化重构）、amd64/arm64 多架构容器发布、按模型名单的第四种出站代理模式，以及请求日志缓存 token 去重统计。

### [Mark Yan (@xyzs996)](https://github.com/xyzs996)

- [#50](https://github.com/klarkxy/opencode-go-mgr/pull/50): corrected DeepSeek peak/off-peak pricing to apply the official Monday-to-Friday schedule, with weekend regression coverage.

修正了 DeepSeek 峰谷计价的工作日判定，并增加周末回归测试，确保计价遵循官方的周一至周五时段规则。

### [zkz098 (@zkz098)](https://github.com/zkz098)

- [#31](https://github.com/klarkxy/opencode-go-mgr/pull/31) (v1.5.6): fixed the Arch Linux AppImage crash by removing the bundled `libwayland-*` libraries.

修复了 AppImage 捆绑 `libwayland-*` 导致的 Arch 系 Linux 启动崩溃。

---

[Docs index / 文档索引](README.md) · [README](../README.md) · [中文 README](../README.zh-CN.md)
