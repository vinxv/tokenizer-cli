# GitHub Actions Node.js 24 迁移

## 目标与决策

移除 `.github/workflows/build.yml` 中使用 Node.js 20 的 JavaScript Actions，直接升级到支持 Node.js 24 的当前版本，不添加临时运行时覆盖变量。

- `actions/checkout`: v4 → v7（两处）。
- `actions/upload-artifact`: v4 → v7；默认 `archive: true` 保持归档行为与 artifact 名称。
- `actions/download-artifact`: v4 → v8；保留 `path` 和 `merge-multiple`。
- `softprops/action-gh-release`: v2 → v3；保留现有发布参数。
- `dtolnay/rust-toolchain@stable` 是 composite Action，无 Node.js 运行时。
- 仓库没有 `actions/setup-node` 引用，无需新增。
- 使用 GitHub 托管的 latest runners；Node.js 24 Actions 要求 runner 至少 v2.327.1。

## 实施清单

- [x] 检查全部 workflow 与上游版本的 action.yml。
- [x] 更新所有 Node.js 20 Actions 引用。
- [x] 验证 YAML、现有输入参数与 diff。

## 进度日志

- **2026-10-12**：确认上游 checkout@v7、upload-artifact@v7、download-artifact@v8、action-gh-release@v3 的 `runs.using` 均为 `node24`，完成工作流升级。
- **2026-10-12**：YAML 解析、全部 Action 输入与上游 metadata 对照、`git diff --check` 通过。Ruby 网络读取超时，改用 Python 获取 metadata、Ruby 解析 YAML 后验证成功。未触发远端构建或发布。

## 参考

- https://github.com/actions/checkout
- https://github.com/actions/upload-artifact
- https://github.com/actions/download-artifact
- https://github.com/softprops/action-gh-release
- https://github.blog/changelog/2025-09-19-deprecation-of-node-20-on-github-actions-runners/
