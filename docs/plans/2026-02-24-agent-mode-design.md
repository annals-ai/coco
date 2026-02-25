# Agent Mode Design

## Overview

在 Coco launcher 中集成 Claude Code，让用户通过双击 ⌘ 快速进入 AI Agent 对话。后端使用 `claude` CLI 的 `--print --output-format stream-json` 模式，前端用 iced 渲染自定义对话 UI。

## 交互流程

```
Alt+Space     → 打开 launcher（搜索模式，现有功能不变）
双击 ⌘⌘       → 切换到 Agent 对话列表页
              ├── 选择历史对话 → 弹出 Agent 窗口（--resume）
              └── 新建对话 / 搜索框输入后回车 → 弹出 Agent 窗口（new）
再双击 ⌘⌘    → 切回搜索模式
Agent 窗口关闭 → 保留 session_id，下次可从列表恢复
```

## 页面结构

### Agent 列表页（在 launcher 主窗口内）

新增 `Page::AgentList`，与 `ClipboardHistory`、`EmojiSearch` 平级。

```
┌───────────────────────────────┐
│  🔍 Search conversations...   │  ← 搜索框（复用现有 text_input）
├───────────────────────────────┤
│  ➕ New conversation       ⏎  │  ← 第一项始终是「新建」
│  🤖 帮我加单元测试        2m  │  ← 历史对话，按时间倒序
│  🤖 修复搜索bug          15m  │
│  🤖 重构config模块      1h   │
│  🤖 添加CI配置          3h   │
├───────────────────────────────┤
│  N conversations   ⌘⌘ Search │  ← footer
└───────────────────────────────┘
```

- 列表项显示：对话标题（取首条用户消息的前 N 个字符）+ 相对时间
- 搜索框可过滤对话历史
- 上下箭头选择，回车打开
- 对话历史从 `~/.claude/projects/` 目录读取 session 文件

### Agent 对话窗口（独立窗口）

通过 `iced::window::open()` 创建新窗口。

```
┌─────────────────────────────────────────┐
│  🤖 Agent            ● Working    ✕    │  ← 标题栏（可拖拽，状态指示）
├─────────────────────────────────────────┤
│                                         │
│  You                                    │
│  帮我给这个项目加单元测试                 │
│                                         │
│  Agent                                  │
│  让我先看看项目结构。                     │
│                                         │
│  ▶ 📄 Read src/main.rs              ✓  │  ← 工具调用卡片（可折叠）
│  ▶ 📝 Write tests/test_search.rs    ✓  │
│                                         │
│  已创建测试文件：                         │
│  ┌─────────────────────────────────┐    │
│  │ #[test]                         │    │  ← 代码块（等宽字体，背景色区分）
│  │ fn test_fuzzy_search() {        │    │
│  │     let result = search("ff");  │    │
│  │     assert!(!result.is_empty());│    │
│  │ }                               │    │
│  └─────────────────────────────────┘    │
│                                         │
├─────────────────────────────────────────┤
│  > 输入消息...                     ⏎   │  ← 输入框
└─────────────────────────────────────────┘
```

窗口规格：
- 默认大小：720x520
- 毛玻璃背景（复用现有 NSVisualEffectView 方案）
- 居中弹出
- 关闭窗口 = 结束当前 claude 进程，保留 session

## 技术架构

### 后端：Claude CLI 子进程

```
Command::new("claude")
  .args(["--print", "--output-format", "stream-json"])
  .args(["--resume", session_id])  // 多轮对话用 resume 串联
  .stdin(Stdio::piped())
  .stdout(Stdio::piped())
  .stderr(Stdio::piped())
  .spawn()
```

每轮对话流程：
1. 用户在输入框输入消息，按回车
2. spawn claude 子进程，通过 stdin 传入 prompt（或用 `--print "message"` 参数）
3. tokio 异步逐行读 stdout，解析 JSON 事件
4. 通过 iced channel 发 Message 更新 UI
5. claude 进程退出 → 等待用户下一条消息 → 用 `--resume` 开新进程

### JSON 事件流解析

claude `stream-json` 输出的事件类型：

```rust
enum ClaudeEvent {
    // 文本内容
    Assistant { message: String },
    // 工具调用开始
    ToolUse { name: String, input: Value },
    // 工具调用结果
    Result { content: String },
}
```

### 数据模型

```rust
/// Agent 对话列表中的一条记录
struct AgentSession {
    session_id: String,        // claude session ID
    title: String,             // 首条消息摘要
    last_active: SystemTime,   // 最后活跃时间
    message_count: usize,      // 消息数
}

/// 对话中的一条消息
enum ChatMessage {
    User(String),
    Assistant(Vec<ContentBlock>),
}

/// 内容块
enum ContentBlock {
    Text(String),              // 普通文本（需 Markdown 解析）
    CodeBlock { lang: String, code: String },
    ToolCall { name: String, status: ToolStatus },
}

enum ToolStatus {
    Running,
    Done,
    Error(String),
}
```

### iced 集成

新增 Message variants：

```rust
enum Message {
    // ... 现有 variants ...

    // 双击 ⌘ 检测
    ToggleAgentMode,

    // Agent 列表页
    AgentSessionSelected(String),  // session_id
    NewAgentSession,

    // Agent 对话窗口
    AgentInput(String),            // 输入框内容变化
    AgentSubmit,                   // 回车发送
    AgentEvent(ClaudeEvent),       // claude 输出事件
    AgentProcessExited(i32),       // claude 进程退出
    AgentWindowClosed(window::Id), // 窗口关闭
    ToggleToolCallExpanded(usize), // 折叠/展开工具调用
}
```

### 双击 ⌘ 检测

在现有 `handle_hotkeys` 中扩展：

```rust
// 记录上次 ⌘ 释放时间
let now = Instant::now();
if now.duration_since(last_cmd_press) < Duration::from_millis(300) {
    // 双击检测成功
    return Message::ToggleAgentMode;
}
last_cmd_press = now;
```

注意：需要用 `global-hotkey` 注册单独的 ⌘ 键监听（modifier-only hotkey），这可能需要用 CGEventTap 或 NSEvent.addGlobalMonitorForEvents 实现，因为 `global-hotkey` 不支持 modifier-only 快捷键。

### Markdown 渲染（MVP 范围）

MVP 阶段只处理：
- **代码块** — ` ``` ` 包裹的内容，用等宽字体 + 深色背景 container 渲染
- **行内代码** — `` ` `` 包裹，等宽字体
- **加粗** — `**text**`，用 bold font weight
- **普通文本** — 直接 `text()` 渲染
- **换行** — 保留原始换行

不处理：标题、列表、表格、链接、图片（后续迭代）

### 文件结构

```
src/
├── app/
│   ├── pages/
│   │   ├── clipboard.rs      // 现有
│   │   ├── emoji.rs           // 现有
│   │   └── agent.rs           // 新增：Agent 列表页 view
│   └── tile/
│       ├── elm.rs             // 扩展 view() 支持 agent 页面和窗口
│       └── update.rs          // 扩展 handle_update() 处理 agent messages
├── agent/
│   ├── mod.rs                 // Agent 模块入口
│   ├── claude_process.rs      // spawn/管理 claude 子进程，解析 stream-json
│   ├── session.rs             // AgentSession 数据模型，session 文件读写
│   ├── chat.rs                // ChatMessage, ContentBlock 数据模型
│   └── markdown.rs            // 简易 Markdown → iced widget 渲染
└── platform/
    └── macos/
        └── mod.rs             // 扩展：⌘ 键监听（CGEventTap）
```

## 范围限制

不做：
- 工具调用审批 UI（使用 `--dangerously-skip-permissions` 或预配置 `--allowedTools`）
- 文件 diff 预览
- 多会话并行（一次只能开一个 Agent 窗口）
- 远程/手机访问
- 完整 Markdown 渲染（MVP 只做代码块 + 加粗）
- 语法高亮（MVP 用纯等宽字体）
