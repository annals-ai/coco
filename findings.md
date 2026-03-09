# 剪贴板功能研究发现

## 当前架构

### 数据模型
- `ClipBoardContentType` 枚举：`Text(String)` | `Image(ImageData<'static>)`
- 存储在 `Tile.clipboard_content: Vec<ClipBoardContentType>`（仅内存，无持久化）
- 每 10ms 轮询系统剪贴板，自动去重
- 无容量限制，内存会无限增长

### 导航方式
- 全局热键 `SHIFT+SUPER+V`（可配置）
- 搜索输入 `cbhist` 切换到剪贴板页面

### 现有 UI
- 两栏布局：左 1/3 列表 + 右 2/3 预览
- 固定高度 360px
- 列表复用 `App.render()` 渲染（原设计给搜索结果的）
- 每行显示第一行文本 + "Clipboard Item" 描述

---

## 已发现的 BUG 和问题

### BUG 1: Enter 键无法粘贴选中项
- `Message::OpenFocused` 中，剪贴板页没有专门处理
- 代码只检查 `tile.results`（搜索结果），不检查 `tile.clipboard_content`
- 剪贴板页面的 `tile.results` 为空 → Enter 键永远是 `None` → 无任何反应
- **根因**：`update.rs:277` → `tile.results.get(tile.focus_id)` 取不到值

### BUG 2: ESC 键行为不正确
- 剪贴板页面没有专门的 ESC 处理
- 当搜索框为空时，ESC 直接关闭窗口（应该先回到 Main 页面）
- 当搜索框不为空时，清空搜索后切回 Main（此时搜索框本来就应该是空的）

### BUG 3: 打字会切离剪贴板页
- `FocusTextInput` 不区分页面，直接修改 query
- `SearchQueryChanged` 触发后，搜索逻辑对剪贴板页 early return
- 但 UI 不会自动切回 Main，导致状态混乱

### BUG 4: 鼠标点击可以工作（按钮有 on_press），但键盘 Enter 不行
- 按钮通过 `content.to_app().render()` 渲染，有 `on_press` 事件
- 所以鼠标点击实际上是可以复制的
- 但 Enter 键走的是 `Message::OpenFocused`，它没有处理剪贴板页

---

## 缺失的功能

| 功能 | 状态 |
|------|------|
| 选中项（键盘 + 鼠标） | 键盘 ✗ / 鼠标 ✓ |
| Enter 粘贴 | ✗ |
| 删除单条记录 | ✗ |
| 复制（不粘贴） | ✗（当前只能复制到系统剪贴板但没有视觉反馈） |
| 清空历史 | ✗ |
| 收藏/置顶 | ✗ |
| 标签 | ✗ |
| 搜索/过滤 | ✗ |
| 持久化存储 | ✗（仅内存，重启后丢失） |
| 容量限制 | ✗（无限增长） |
| 图片预览 | ✗（只显示 `<img>` 文字） |

---

## 关键代码位置

| 文件 | 行 | 内容 |
|------|-----|------|
| `src/clipboard.rs` | 全部 | 数据模型 |
| `src/app/pages/clipboard.rs` | 全部 | UI 视图 |
| `src/app/tile/update.rs:244-291` | OpenFocused | **BUG** 未处理剪贴板 |
| `src/app/tile/update.rs:105-159` | EscKeyPressed | **BUG** 未处理剪贴板 |
| `src/app/tile/update.rs:167-242` | ChangeFocus | 方向键导航（可工作） |
| `src/app/tile/update.rs:348-374` | SwitchToPage | 页面切换（可工作） |
| `src/app/tile/update.rs:466-469` | ClipboardHistory | 数据收集（可工作） |
| `src/app/tile/elm.rs:169-175` | view | 剪贴板视图集成 |
| `src/app/apps.rs:132-205` | render | 行渲染（需要改造） |
| `src/commands.rs:132-139` | CopyToClipboard | 复制执行 |

---

# Coco 内存排查发现

## 待验证假设

- 剪贴板历史仅内存保存且无容量限制，长时间运行可能持续推高内存
- 若剪贴板中包含图片，`ImageData<'static>` 可能把解码后的像素数据长期留在内存
- 应用列表/图标加载路径可能一次性把大量图标解码进内存并长期缓存

## 第一批证据

- 当前运行进程为 `/Applications/Coco.app/Contents/MacOS/coco`，PID `41315`
- 代码搜索显示以下高风险路径同时存在：
  - `src/clipboard.rs` 使用 `ImageData<'static>` 保存图片剪贴板内容
  - `src/app/tile.rs` / `src/app/tile/update.rs` 仍有剪贴板订阅与动画轮询
  - `src/app/tile/elm.rs` 初始化时调用 `get_installed_apps(store_icons)`
  - `src/platform/macos/discovery.rs` / `src/platform/macos/mod.rs` 会为应用列表和运行中应用加载 icon

## 当前判断

- 目前最可疑的不是单点泄漏，而是多处“长期常驻缓存”叠加：
  - 已安装应用 icon 索引
  - 运行中应用 icon
  - 剪贴板历史内容，尤其图片

## 系统层证据

- `ps` 显示 PID `41315` 的 RSS 约为 `2683104 KB`（约 `2.56 GB`）
- `top` 显示同一进程常驻内存约 `2060 MB`
- `vmmap` 显示：
  - `Physical footprint: 2.0G`
  - `Physical footprint (peak): 2.8G`
  - `mapped file: 1.2G / resident 467.3M`
  - `CG image: 160.5M`
  - `Image IO: 160.4M`
  - `MALLOC_LARGE: 90.7M`
  - `MALLOC_SMALL: 131.8M`

## 代码层证据

- `src/app/tile/elm.rs`
  - `new()` 启动阶段直接执行 `get_installed_apps(store_icons)`，不是懒加载
- `src/platform/macos/discovery.rs`
  - `get_installed_apps()` 会遍历 Launch Services 返回的所有 app
  - `query_app()` 在 `store_icons = true` 时为每个 app 尝试加载 icon
  - icon 优先 `.icns`，失败后回退到 `NSWorkspace.iconForFile()`，后者会触发更重的系统图标解码路径
- `src/platform/macos/mod.rs`
  - `get_running_apps(store_icons)` 也会为运行中 app 再次取 icon
- `src/app/tile.rs`
  - `handle_clipboard_history()` 以 `10ms` 轮询系统剪贴板
  - 图片内容直接封装为 `ClipBoardContentType::Image(a)` 并发送
- `src/clipboard.rs`
  - 图片以 `ImageData<'static>` 常驻在 Rust 侧内存
- `src/clipboard_store.rs`
  - 当前 `max_entries = 500`
  - 但 `save()` 只持久化文本；图片不会落盘，意味着图片历史全靠内存保留直到被 trim

## 中间结论

- 当前 2GB 级别占用和 `vmmap` 的分布更像“图像/icon 资源被大量解码并长期持有”，不是普通字符串或小对象堆积
- 剪贴板图片是第二热点，因为它们以原始字节常驻内存；但如果用户没有频繁复制大图，单靠它通常很难解释到 2GB
- 启动即全量 icon 预加载是目前最像主因的一条路径

## 更强证据：图标解码链路

- `src/utils.rs`
  - `icon_from_workspace()` 已经做了 128px 限制，说明作者自己也知道“直接解码大图标会很重”
  - 但 `handle_from_icns()` 仍然 `max_by_key(width * height)` 选取 **最大的** icon layer，并直接 `Handle::from_rgba(...)`
  - 这意味着 `.icns` 快路径会把 512px / 1024px 图标完整展开为 RGBA 常驻内存
- `src/platform/macos/discovery.rs`
  - `query_app()` 的优先顺序是先走 `.icns` 快路径，再 fallback 到 `NSWorkspace`
  - 所以大量普通 app 会先命中这个“解最大图标”的路径
- `heap 41315`
  - `NSBitmapImageRep: 863`
  - `ISImageDescriptor: 832`
  - `NSISIconImageRep: 832`
  - `CGImage: 497`
  - 这说明进程内确实持有了大批图标位图对象，而不是只有少量 UI 资源

## 进一步判断

- 最可能的主因是：
  - 启动时全量扫描已安装应用
  - 对每个应用预加载 icon
  - `.icns` 路径还会优先解码最大分辨率图层
- 次要放大因素是：
  - `NSWorkspace` / IconServices 映射出的图标缓存文件很多，`mapped file` 常驻高
  - 剪贴板图片历史和 10ms 轮询会进一步增加内存压力，但更像“次要叠加项”

## 已实施修复

- `src/utils.rs`
  - `.icns` 运行时解码不再取最大图层
  - 改为选择最接近 `128px` 的图层，并在必要时下采样到目标尺寸
  - 新增统一的 app bundle icon 加载函数
- `src/app/tile/elm.rs`
  - 启动时不再预加载 installed apps 的 icon
- `src/app/tile/update.rs`
  - 新增当前可见结果 icon 懒加载
  - 增加 icon cache 和 in-flight 去重
  - 零查询、搜索结果、焦点移动都会按需补图标
- `src/clipboard_store.rs`
  - 图片历史新增单独预算：
    - 最多 `24` 张图片
    - 总图片字节最多 `64MB`
  - 优先裁剪最旧的非置顶图片

## 修复后验证

- `cargo test` 通过：`101 passed`
- 重新安装后的新进程观测：
  - 启动约 6 秒：RSS `130976 KB`，`top` 常驻约 `89MB`
  - 启动约 23 秒：RSS `117296 KB`，`top` 常驻约 `78MB`

## 结果判断

- 修复后已从原先的约 `2.0G - 2.6G` 常驻占用，降到约 `80MB - 130MB`
- 这基本确认主因就是“全量 icon 预加载 + 大尺寸图层解码”，不是传统意义上的堆泄漏

---

# Async Icon Placeholder Follow-up

## 新问题

- 纯异步 icon 加载下，某些结果项会先以弱占位态渲染
- 由于缓存回填发生在后续消息里，已见过的结果也可能先闪一下占位，再切成真实 icon

## 根因

- `rebuild_results_for_current_query()` 重建搜索结果后，没有立刻把 `icon_cache` 里的已知 icon 同步回填
- `OpenWindow` 重建 zero-query 列表后，同样没有立刻用 `icon_cache` 补回已缓存 icon
- 当前 `None` icon 的 fallback 过于轻，视觉上容易像“空白”

## 已实施修复

- 在结果重建后立即执行缓存 icon 回填，减少已缓存结果的闪烁
- 在打开窗口重建 zero-query 后立即回填缓存 icon
- 将 pending icon 的 fallback 改成稳定的字母 badge，而不是几乎空白的弱占位态

## 剩余抖动根因

- 当前可见 icon 仍然是“每个 icon 单独异步完成、每完成一个就发一条消息”
- `Message::AppIconLoaded(...)` 会反复更新 `tile.results` 和 `tile.zero_query_cache`
- 同一屏里多个 icon 接连完成时，列表会短时间连续重绘，因此用户仍会感受到抖动

## 本轮修复

- 将“逐个 icon 回填”改为“当前可见窗口批量回填”
- 同一批次 icon 统一通过一次 `Message::AppIconsLoaded(...)` 写入缓存并更新列表
- 保留异步加载、缓存命中即时回填和字母 badge placeholder，不回退到首屏同步解码

## 本轮自测结论

- 新增回归测试，直接验证 `Message::AppIconsLoaded(...)` 前后：
  - 结果顺序不变
  - `focus_id` 不变
  - 搜索结果高度不变
  - `target_window_height` / `target_blur_height` / `pending_window_height` 不变
- 这说明剩余“抖动”已经不是数据层或窗口 resize 在跳，而更像 icon slot 的视觉替换本身在闪

## 继续修复

- 去掉字母 badge placeholder，改成和真实 icon 共用同一个固定 slot
- 未加载时显示统一的单色 app glyph；加载完成后仅替换 slot 内部内容
- 这样可以避免 badge/image 两种完全不同的视觉结构来回切换

## 再进一步

- 将 icon slot 改为分层结构：
  - 底层 placeholder 常驻
  - 顶层真实 icon 到位后覆盖
- 这样即使 icon 在异步时刻完成，整行 widget 树和 slot 结构也不需要在“无 icon / 有 icon”之间切换
- 当前这条路径更接近真正的“只换像素，不换结构”

## 左侧空隙来源

- 当前结果行左侧缩进不是单一 margin，而是以下几段叠加：
  - `RESULT_LIST_PADDING_X`
  - `RESULT_ROW_PADDING_X`
  - 固定 `RESULT_ICON_SLOT`
  - `RESULT_ROW_CONTENT_GAP`
- 这会让结果文字起点明显比搜索框更靠右，所以视觉上会觉得左边“空了一大块”

## 本轮调整

- 收紧列表横向 padding、行内 padding、icon slot 和 icon/text gap
- 让结果行的文字起点更靠近搜索框的视觉起点

## 为什么上一轮没生效

- 左侧大空隙的真正主因不是常量，而是 `src/app/apps.rs` 里的 `app_icon_slot()`
- 最外层 icon slot 容器调用了 `.center_x(Fill)` / `.center_y(Fill)`
- 在 `iced` 里，`center_x(width)` 会先执行 `width(width)`；传入 `Fill` 等于把 icon slot 自身宽度直接改成了 `Fill`
- 结果是整行左半大块空间先分给 icon 容器，图标再在这块空间里居中，看起来就像左边一直空了一大截

## 本轮自测

- 直接阅读 `iced_widget::container::center_x()` 实现，确认它会修改容器宽度
- 查看实际运行日志 `/Users/kcsx/coco_debug.log`，确认 Coco 的打开/渲染链路正常，问题不在窗口 resize
- 新增回归测试：
  - `app_icon_slot_keeps_fixed_size_hint`
  - 断言 icon slot 的 `size_hint()` 仍然是固定 `RESULT_ICON_SLOT`，不再是 `Fill`
