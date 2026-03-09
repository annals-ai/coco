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
