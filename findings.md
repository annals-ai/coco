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
