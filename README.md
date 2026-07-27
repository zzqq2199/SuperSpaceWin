# Space++ (Windows)

Space++ 的 Windows 移植版：把空格键变成 Hyper 键。按住空格 + 其他键触发快捷操作，轻敲空格仍然输出空格。核心状态机与 [macOS 版 SuperSpace](https://github.com/zzqq2199/SuperSpace) 保持一致。

使用 Rust 实现：`WH_KEYBOARD_LL` 低级键盘钩子拦截按键，`SendInput` 注入合成按键（带 `dwExtraInfo` 标记防止自捕获），系统托盘图标显示 IDLE / HYPER 状态。

## 快捷键（默认 config.json，已按 Windows 语义翻译）

| 组合 | 功能 |
|------|------|
| `space + h/j/k/l` | ← ↓ ↑ → |
| `space + y / o` | Home / End |
| `space + u / i` | Page Down / Page Up |
| `space + e` | Esc |
| `space + m` | Backspace |
| `space + n` | Ctrl+Backspace（删前一个词） |
| `space + b` | 删至行首（Shift+Home, Backspace） |
| `space + ,` | Delete |
| `space + .` | Ctrl+Delete（删后一个词） |
| `space + /` | 删至行尾（Shift+End, Delete） |
| `space + c / v` | Ctrl+C / Ctrl+V |
| `space + 1..0 - =` | F1..F12 |
| `space + q` | 退出 Space++ |

长按空格（`hold_as_hyper: true`）直接进入 Hyper 模式；敲空格照常输出空格。

## 状态机

四个状态，与 mac 版 `event_handler.py` 一致：

- `IDLE`：空闲。无修饰键时按下空格 → 吞掉，进入 `ONLY_SPACE_DOWN`
- `ONLY_SPACE_DOWN`：空格弹起 → 补发空格（轻敲）；按下普通键 → 记为候选键进入 `SPACE_NORM_DOWN`；空格自动重复 → `HYPER_MODE`
- `SPACE_NORM_DOWN`：消歧关键态。空格先弹起 → 是打字，补发"空格+候选键"；候选键先弹起/重复/按下第三键 → 是 Hyper 组合，发映射键
- `HYPER_MODE`：映射表中的键按下即发映射键，空格弹起回 `IDLE`

## 构建与运行

需要 Rust 工具链（GNU 或 MSVC target 均可）：

```powershell
cargo build --release
.\target\release\spacepp-win.exe
```

产物为单个 exe（静态链接，默认配置已编译内嵌），拷到任意位置即可运行。如需自定义映射，把 `config.json` 放在 exe 同目录（或用环境变量 `SPACEPP_CONFIG` 指定路径）即可覆盖内置默认。程序无窗口，仅显示托盘图标；右键菜单可开关"开机自启"（写入 HKCU Run 注册表项）和退出。

日志（verbose 开启时）写入 `%TEMP%\spacepp.log`。

## 测试

状态机与配置解析为纯逻辑模块，可直接跑单元测试：

```powershell
cargo test
```

## 配置格式

与 mac 版相同的 JSON 结构和键名（`command` 自动翻译为 Ctrl，`option` 为 Alt，`delete` 表示 Backspace）。此外映射值支持数组表示按键序列：

```json
"b": [{"key": "home", "modifiers": ["shift"]}, {"key": "delete"}]
```
