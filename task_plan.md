# Task Plan: 修复 Coco 动画系统 + 卡死问题 + 内存优化

## 问题分析

### Bug 1: 快捷键卡死 — 按了没反应
- **根因**: bootstrap close 执行 `window::close()` 关闭窗口后，iced daemon 不再正常传递全局热键事件
- **log 证据**: bootstrap close 后只剩 `[perm]` 轮询日志，再无 `KeyPressed` 事件
- **关联**: 每次隐藏都 close+reopen 窗口，极易导致状态不同步

### Bug 2: 动画方式错误
- **当前做法**: 用 `NSWindow.setFrame:` 改变窗口尺寸来模拟缩放 → 内容重排、模糊层错位、像"拉窗帘"
- **用户要求**: 整体缩放+烟雾感模糊，像 macOS Spotlight
- **正确做法**: 用 `CALayer.transform`（CATransform3D scale）做视觉缩放，窗口帧不变

### Bug 3: 权限轮询风暴
- `handle_permission_polling` 每 2 秒调用 `AXIsProcessTrustedWithOptions` + `CGPreflightListenEventAccess`
- 已硬编码 `permissions_ok = true`，但 subscription 仍在运行
- 产生大量 `[perm]` 日志刷屏

### Issue 4: 内存占用 ~2.3GB RSS
- 后续单独处理（可能是图标全分辨率加载）

---

## 修复方案

### Phase 1: 根治卡死 — 窗口不关闭，用 orderOut/orderFront [核心]

**核心思路**: 窗口在 `new()` 创建后**永不** `window::close()`

| 操作 | 旧方式 | 新方式 |
|------|--------|--------|
| 隐藏 | `window::close(id)` → 窗口销毁 | `NSWindow.orderOut` → 窗口仅不可见 |
| 显示 | `window::open()` → 创建新窗口 | `NSWindow.makeKeyAndOrderFront` → 恢复 |
| 状态 | main_window_id 不断变化 | main_window_id 固定不变 |

**改动文件**:
- `src/platform/macos/mod.rs`: 新增 `hide_main_window()` / `show_main_window()`
- `src/app/tile/update.rs`: HideWindow 改用 orderOut，OpenWindow 改用 orderFront
- `src/app/tile/elm.rs`: 去掉 bootstrap close 逻辑
- `src/app/tile.rs`: 去掉 `bootstrapped` 字段

### Phase 2: Spotlight 式 CALayer 动画 [动画]

**参考 Spotlight**:
- 显示: alpha 0→1 + scale 0.97→1.0, 200ms, cubic-bezier(0.25, 0.1, 0.25, 1.0)
- 隐藏: alpha 1→0 + scale 1.0→0.97, 150ms, 同曲线
- 缩放幅度极小（3%），动画曲线缓和

**实现**: 用 `NSAnimationContext.runAnimationGroup` 驱动原生动画
- 对 `NSWindow.alphaValue` 做淡入淡出
- 对窗口 contentView 的 `CALayer.transform` 做缩放
- 动画完成回调中执行 `orderOut`（隐藏时）
- **不再需要 iced 的 Tick subscription 驱动动画** — 全部交给 Core Animation

**改动文件**:
- `src/platform/macos/mod.rs`: 重写 `set_window_appearance` → `animate_show()` / `animate_hide()`
- `src/app/tile/update.rs`: 去掉 `Message::Tick` 处理，简化 HideWindow/OpenWindow
- `src/app/tile.rs`: 去掉 `anim_visibility*`、`hiding_window_id` 等动画状态字段、去掉 anim_tick subscription

### Phase 3: 停止权限轮询风暴

- 去掉 `handle_permission_polling` subscription
- 去掉 `Message::RefreshPermissions`
- 已经硬编码 `permissions_ok = true`，banner 永远不显示

---

## 执行顺序

Phase 1 → Phase 2 → Phase 3（一次性完成，因为它们紧密关联）

## 关键文件清单

| 文件 | 修改内容 |
|------|---------|
| `src/platform/macos/mod.rs` | hide/show/animate 函数，去掉 setFrame 动画 |
| `src/app/tile/update.rs` | HideWindow/OpenWindow/KeyPressed 重写 |
| `src/app/tile/elm.rs` | new() 去掉 bootstrap，view() 简化 |
| `src/app/tile.rs` | 去掉动画字段，去掉 anim_tick/permission subscription |
| `src/app.rs` | 去掉 Tick/WindowHideAnimComplete/SyncBlur 等无用 Message |

---

# Task Plan: 调查 Coco 运行时内存占用过高

## 目标

- 复现并量化 Coco 的实际内存占用
- 找到主要内存热点是常驻缓存、数据结构无限增长，还是图像/图标解码导致
- 给出基于证据的根因判断和后续修复方向

## 本轮排查步骤

### Phase 1: 复现与量化 [completed]
- 确认运行中的 Coco 进程与 RSS
- 如有必要启动应用并观察稳定态内存
- 采集 `vmmap` / `ps` / `sample` 等系统证据

### Phase 2: 代码路径排查 [completed]
- 检查剪贴板历史是否无限增长
- 检查应用图标、图片解码和缓存逻辑
- 检查是否存在高频轮询导致对象持续累积

### Phase 3: 交叉验证与结论 [completed]
- 将系统层证据与代码实现对照
- 输出最可能根因、次要因素与修复建议

### Phase 4: 实施运行时内存修复 [completed]
- 新增内存修复设计文档与实施计划
- `.icns` 运行时解码改为目标尺寸选择 + 下采样
- 已安装 app icon 改为可见结果懒加载 + 小缓存
- 剪贴板图片新增数量/总字节预算

### Phase 5: 验证与安装 [completed]
- `cargo fmt`
- `cargo test`
- 重新构建、签名并安装到 `/Applications/Coco.app`
- 复测新进程内存占用

## Errors Encountered

| Error | Attempt | Resolution |
|-------|---------|------------|
| `session-catchup.py` 路径不存在 | 1 | 直接读取现有 `task_plan.md` / `findings.md` / `progress.md` 继续排查 |
