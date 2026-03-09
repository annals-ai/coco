# 进度日志

## Session 2026-02-24

### 研究阶段 ✅
- 读取并分析了所有剪贴板相关代码
- 发现 4 个核心 BUG（Enter 无效、ESC 行为错误、打字切离、键盘/鼠标不一致）
- 发现 10+ 项缺失功能
- 创建了 findings.md 和 task_plan.md

### Phase 1: 修复核心 BUG [pending]
### Phase 2: UI 重设计 [pending]
### Phase 3: 新消息类型 [pending]
### Phase 4: 删除功能 [pending]
### Phase 5: 搜索功能 [pending]
### Phase 6: 收藏功能 [pending]
### Phase 7: 容量限制 [pending]

## Session 2026-03-08

### 内存排查阶段 [in_progress]
- 读取 `using-superpowers`、`systematic-debugging`、`planning-with-files` 技能
- 发现项目已有旧的计划文件，未覆盖，改为追加本轮内存排查记录
- `session-catchup.py` 默认路径不存在，已记录并改用手动恢复上下文
- 下一步开始量化实际内存占用并对照代码热点
- 已确认运行进程 PID 为 `41315`
- 已初步圈定三类高风险来源：剪贴板图片、已安装应用 icon 预加载、运行中应用 icon 获取
- 已用 `ps` / `top` / `vmmap` 量化：常驻约 `2.0G-2.6G`，峰值 `2.8G`
- 已确认启动路径存在全量 app icon 预加载，剪贴板图片会以内存对象形式保留
- 已确认 `.icns` 快路径会解码最大图层，而不是目标尺寸
- 已用 `heap` 交叉验证：进程内存在数百个 icon/image 相关对象，和 `vmmap` 结论一致

### 实施阶段 ✅
- 新增设计文档：`docs/plans/2026-03-09-memory-icon-design.md`
- 新增实施计划：`docs/plans/2026-03-09-memory-icon-plan.md`
- 完成 `.icns` 目标尺寸解码和 bundle icon loader
- 完成 installed apps icon 懒加载、缓存和 in-flight 去重
- 完成剪贴板图片预算限制和单元测试

### 验证阶段 ✅
- `cargo fmt` 完成
- `cargo test` 通过，`101 passed`
- 已重新部署到 `/Applications/Coco.app`
- 新进程内存复测从 GB 级降到约 `80MB - 130MB`

## Session 2026-03-10

### 异步 icon 占位态修复 ✅
- 新增设计文档：`docs/plans/2026-03-10-icon-placeholder-design.md`
- 新增实施计划：`docs/plans/2026-03-10-icon-placeholder-plan.md`
- 搜索结果和 zero-query 重建后立即回填 `icon_cache`
- `App.icons == None` 时改为稳定的字母 badge placeholder
- `cargo test` 再次通过，`101 passed`
- 已重新部署到 `/Applications/Coco.app`

### 异步 icon 批量回填防抖 ✅
- 确认剩余抖动来自“每个 icon 单独完成就触发一次列表重绘”
- 将可见结果 icon 加载改为单批次后台任务
- 新增 `Message::AppIconsLoaded(...)`，统一写入 cache 并一次性回填列表
- `cargo test` 再次通过，`101 passed`

### icon slot 视觉稳定化 ✅
- 新增回归测试，确认 `AppIconsLoaded(...)` 不会改结果顺序、focus 或窗口高度缓存
- 据此确认剩余问题主要在 placeholder/image 的视觉替换，不是状态层抖动
- 将未加载态改为与真实 icon 共用同一个固定 slot，仅替换 slot 内部内容
- `cargo test` 再次通过，`102 passed`

### 分层 icon slot 防闪 ✅
- 根据实际截图继续收敛：将 icon slot 改成“底层 placeholder + 顶层真实 icon”
- 避免在有无 icon 之间切换不同 widget 结构
- `cargo test` 再次通过，`102 passed`

### 收紧左侧 gutter ✅
- 根据截图定位到左侧空隙来自多段固定横向预留的叠加
- 收紧结果列表 padding、行内 padding、icon slot 和 icon/text gap
- `cargo test` 再次通过，`102 passed`

### 修正 icon slot 被错误拉伸 ✅
- 进一步自测后确认：左侧大空隙主因是 `app_icon_slot` 最外层容器使用了 `.center_x(Fill)` / `.center_y(Fill)`
- 这会把 icon slot 自身尺寸改成 `Fill`，不是单纯做居中
- 改为固定尺寸 + 对齐，不再让 icon slot 占满整行左侧空间
- 新增回归测试 `app_icon_slot_keeps_fixed_size_hint`
- 查看 `/Users/kcsx/coco_debug.log` 验证运行链路正常
- `cargo test` 再次通过，`103 passed`
