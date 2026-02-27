# List Hover + Calculator Redesign Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add consistent hover/active interaction states to clipboard/app/currency lists, and redesign calculator results to a clean equation format without icons or emoji.

**Architecture:** Introduce a single hover-focus message path so pointer hover can sync `focus_id` across list UIs. Unify row interaction styling via status-aware button styles (`active/hover/pressed`) and convert clipboard rows into interactive buttons. Normalize calculator query parsing to support `x`/`×` as multiplication and emit display text as `expression = result`.

**Tech Stack:** Rust, iced 0.14 UI widgets/styles, existing launcher state machine in `src/app/tile/update.rs`.

---

### Task 1: Add Row Hover Focus Message

**Files:**
- Modify: `src/app.rs`
- Modify: `src/app/tile/update.rs`

**Step 1: Write the failing test**

```rust
// add to an existing update test module or create one:
#[test]
fn hover_result_updates_focus_id() {
    let mut tile = make_tile_fixture();
    tile.page = Page::Main;
    tile.results = vec![make_app("A"), make_app("B")];
    tile.focus_id = 0;

    let _ = handle_update(&mut tile, Message::HoverResult(1));

    assert_eq!(tile.focus_id, 1);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test hover_result_updates_focus_id -- --nocapture`
Expected: FAIL with unknown `Message::HoverResult` or missing match arm.

**Step 3: Write minimal implementation**

```rust
// src/app.rs
pub enum Message {
    // ...
    HoverResult(u32),
}

// src/app/tile/update.rs
Message::HoverResult(id) => {
    let len = match tile.page {
        Page::ClipboardHistory => tile.clipboard_display_count() as u32,
        Page::AgentList => tile.agent_display_count() as u32,
        Page::WindowSwitcher => tile.window_list.len() as u32,
        _ => tile.results.len() as u32,
    };

    if id < len {
        tile.focus_id = id;
    }
    Task::none()
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test hover_result_updates_focus_id -- --nocapture`
Expected: PASS.

**Step 5: Commit**

```bash
git add src/app.rs src/app/tile/update.rs
git commit -m "feat: add shared hover-focus message for list rows"
```

### Task 2: Add Hover/Active Styles to App/Currency Rows

**Files:**
- Modify: `src/styles.rs`
- Modify: `src/app/apps.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn app_row_uses_status_aware_button_style() {
    // compile-time regression test idea:
    // ensure style closure accepts button::Status and compiles with new signature.
    // This fails before refactor because function signature doesn't accept status.
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test app_row_uses_status_aware_button_style -- --nocapture`
Expected: FAIL or compile error due to old style function signature.

**Step 3: Write minimal implementation**

```rust
// src/styles.rs
pub fn result_button_style(
    theme: &ConfigTheme,
    focused: bool,
    status: button::Status,
) -> button::Style {
    // base + hovered/pressed treatment
}

// src/app/apps.rs
Button::new(row)
    .style(move |_, status| result_button_style(&theme_for_button, focused, status));
```

**Step 4: Run test to verify it passes**

Run: `cargo test --no-fail-fast`
Expected: PASS and app rows visually respond on hover/press.

**Step 5: Commit**

```bash
git add src/styles.rs src/app/apps.rs
git commit -m "feat: add hover and pressed states for app and currency rows"
```

### Task 3: Add Hover/Active to Clipboard List Rows

**Files:**
- Modify: `src/app/pages/clipboard.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn clipboard_row_is_interactive_button() {
    // compile-level test: clipboard_row should build a Button-based row
    // and emit hover focus message.
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test clipboard_row_is_interactive_button -- --nocapture`
Expected: FAIL before converting row to button + hover handling.

**Step 3: Write minimal implementation**

```rust
let content = Button::new(row)
    .on_press(Message::OpenFocused)
    .style(move |_, status| result_button_style(&theme_for_button, focused, status));

mouse_area(container(content) /* row shell */)
    .on_enter(Message::HoverResult(display_idx));
```

**Step 4: Run test to verify it passes**

Run: `cargo test --no-fail-fast`
Expected: PASS and clipboard row now shows hover + pressed feedback.

**Step 5: Commit**

```bash
git add src/app/pages/clipboard.rs
git commit -m "feat: make clipboard rows hoverable and active"
```

### Task 4: Calculator Input/Display Redesign

**Files:**
- Modify: `src/app/tile/update.rs`
- Modify: `src/app/apps.rs`
- Modify: `src/calculator.rs`

**Step 1: Write the failing test**

```rust
#[test]
fn parser_supports_x_as_multiplication() {
    let expr = Expr::from_str("2 x 3").unwrap();
    assert_eq!(expr.eval(), Some(6.0));
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test parser_supports_x_as_multiplication -- --nocapture`
Expected: FAIL with parse error on `x`.

**Step 3: Write minimal implementation**

```rust
// src/calculator.rs lexer
'x' | 'X' | '×' => Token::Star,

// src/app/tile/update.rs calculator branch
let display = format!("{} = {}", format_calc_expr(&tile.query), format_calc_value(value));
name = display;
name_lc = format!("__calc__|{}", tile.query_lc);

// src/app/apps.rs
let is_calculator_result = self.name_lc.starts_with("__calc__|");
if theme.show_icons && !is_currency_result && !is_calculator_result {
    // existing icon rendering
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test parser_supports_x_as_multiplication -- --nocapture`
Expected: PASS and UI shows `1 + 1 = 2` style text for calculator result.

**Step 5: Commit**

```bash
git add src/calculator.rs src/app/tile/update.rs src/app/apps.rs
git commit -m "feat: redesign calculator result and support x multiplication"
```

### Task 5: Regression Validation

**Files:**
- Modify if needed: `src/app/tile/elm.rs` (only if style signature updates require call-site changes)

**Step 1: Run focused checks**

Run: `cargo test --no-fail-fast`
Expected: all tests pass.

**Step 2: Run static checks**

Run: `cargo clippy --all-targets --all-features -- -D warnings`
Expected: no warnings.

**Step 3: Manual smoke verification**

Run: `cargo run`
Expected:
- App list rows: hover + pressed feedback present.
- Currency list rows: hover + pressed feedback present.
- Clipboard rows: hover + pressed feedback present.
- Calculator query `1 + 1`: shows exactly `1 + 1 = 2` style (no emoji, no icon).
- Calculator query `2 x 3`: computes correctly.

**Step 4: Commit**

```bash
git add -A
git commit -m "chore: validate list interaction and calculator ui refresh"
```
