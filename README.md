# rust-ime 终极使用手册

> **声明：本人不懂代码，这代码全是 Gemini 写的。有什么 Bug 别找我，找这个软件真正的作者 Gemini。**

这是一个专为 Linux 设计的高性能、现代化拼音输入法引擎。采用 Rust 编写，深度适配 Wayland/COSMIC 桌面环境，致力于为程序员、极客和文字工作者提供极致的“零干扰”盲打体验。

![界面展示](picture/ni_without_style.png)
![界面展示](picture/niP.png)
![界面展示](picture/gn.png)
![界面展示](picture/gnG.png)

---

## 🎯 产品定位：回归输入的本质

rust-ime 不仅仅是一个输入法，它是一个高效的文字生产工具。与传统输入法框架（如 Fcitx5/IBus）不同，它通过底层的 `evdev` 直接与硬件通信，消除了中间层，带来了丝滑般的输入反馈。

### 核心设计哲学
- **全拼直觉，双拼效率**：通过独创的“双快”模式，在保留全拼拼写习惯的同时，大幅减少击键次数。
- **语义辅助，零重码选择**：利用英文本能解决拼音同音字的重码问题。
- **界面极致自定义**：提供完整的 Web 配置中心，支持秒级实时调色与字号调节。

---

## 🚀 核心灵魂特性 (Key Features)

### 1. 英文辅助码：解决重码的终极方案
这是 rust-ime 的特色功能。在拼音重码极多时，您无需翻页，只需输入一个**大写字母**即可根据“义”精准定位。

*   **逻辑**：系统自动扫描候选词的英文释义，匹配第一个大写单词的首字母。
*   **示例**：
    *   输入 `li` ⮕ 候选词：里、离、礼、理...
    *   输入 <code>li<b>C</b></code> ⮕ **礼** (Ceremony) 瞬间排到第一。
    *   输入 <code>li<b>L</b></code> ⮕ **理** (Logic) 瞬间排到第一。
*   **优势**：大幅提高盲打成功率，尤其是在简拼状态下。

### 2. 笔画辅助码 (SBSRF 4键系统)：解决重码的第二重保障
如果您更习惯根据汉字的形状来筛选，可以使用基于 **SBSRF (笔画森林)** 逻辑的 4 键辅助码系统。

*   **布局逻辑 (5x5 矩阵)**：将 5 种基本笔画映射到 5x5 的按键矩阵中，每 1 个按键代表 **2 个连续笔画**。
    *   **行**：代表该对笔画中的 **第 1 笔**。
    *   **列**：代表该对笔画中的 **第 2 笔**。
    *   **笔画索引**：1:横(G-A), 2:竖(H-M), 3:撇(T-Q), 4:捺/点(Y-P), 5:折(N-X)。
    *   **矩阵分布**：
        *   横 (1)：G F D S A
        *   竖 (2)：H J K L M
        *   撇 (3)：T R E W Q
        *   捺 (4)：Y U I O P
        *   折 (5)：N B V C X
*   **编码规则**：
    *   **第 1 个辅助码按键**：对应汉字的 **前 2 笔**。
    *   **第 2 个辅助码按键** (可选)：对应汉字的 **末 2 笔** (仅限 3 画及以上的字)。
*   **操作方法**：
    *   输入拼音后，直接输入 **小写字母** 辅助码。
    *   **示例**：
        *   输入 `ren` ⮕ 候选词：人、任、认...
        *   输入 `renv` ⮕ **人** (首两笔为“撇捺”，撇3+捺4 -> `v`)。
        *   输入 `de` ⮕ 候选词：的、得、地...
        *   输入 `detu` ⮕ **的** (前两笔“撇竖”=31=`t`，末两笔“捺竖”=42=`u`)。
*   **优势**：纯小写输入，不破坏输入节奏，利用汉字结构直觉瞬间定位重码。

### 3. 双击快捷输入 (Double-Tap)
**通过快速连击同一个键，实现长韵母或常用短语的极速注入。**
- **操作**：快速点击同一个字母键两次（判定时间可在 Web 配置中心自定义，建议 250-400ms）。
- **默认映射**：
    - `i, i` ⮕ `ing`
    - `u, u` ⮕ `sh`
    - `l, l` ⮕ `uang`
    - (可在 Web 配置中心完全自定义，支持任何字母到字符串的映射)
- **优势**：无需按住修饰键，手指移动幅度最小，节奏感极强。

### 4. 长按行为定制 (Long Press)
**通过长按某个字母键，实现大小写切换、变音符号或音调符号的快速输入。**
- **操作**：按住某个字母键不放（判定时间可在 Web 配置中心自定义，建议 400-600ms）。
- **默认映射**：
    - 长按字母键 ⮕ 该字母的大写形式。
    - (可在 Web 配置中心完全自定义，支持任何字母到字符串、变音字符或音调的映射)
- **优势**：解决 Linux 下输入大写字母或特殊字符（如 ā, é）需要频繁操作 Shift 或组合键的痛点。

### 5. 平卷舌互换 (Retroflex Toggle)
**一键纠正 z/zh, c/ch, s/sh 拼写错误。**
- **操作**：在拼音缓冲区不为空时，按下 `/` 键。
- **功能**：自动将当前音节的平舌音转为卷舌音，或将卷舌音转为平舌音（如 `zan` ⮕ `zhan`）。

### 6. 极客导航模式 (HJKL Navigation)
**像 Vim 一样在候选词中穿梭。**
- **操作**：在拼音缓冲区不为空时，按下 `` ` `` (反引号)。
- **功能**：进入导航模式（UI 显示 `[H:左 J:下 K:上 L:右]`），此时 `h/j/k/l` 分别映射为 `左/下/上/右`。按任意其他拼音键自动退出。

### 7. 快捷位置切换 (Quick Position)
**无需进入网页，直接调整窗口位置。**
- **操作**：在缓冲区为空时，先按 `` ` `` 进入方案切换模式，再按 `t` (Top) 或 `b` (Bottom)。
- **功能**：立即将传统候选窗移动到屏幕顶部或底部，并永久保存配置。

### 8. 提示词上屏 (Commit Hint)
**快速输入候选词对应的英文或翻译。**
- **操作**：在选中候选词时，按下 `Shift + Space`。
- **功能**：直接将候选词对应的提示（如 English Hint）上屏。

### 9. 粘性筛选模式 (Sticky Filter)
**通过单键触发进入精准筛选，解决重码问题的第二重保障。**
- **CapsLock 筛选**：输入拼音时，按下 `CapsLock` 键，立即针对**当前页**的 5 个候选词开启精准筛选。
- **Shift 筛选**：输入拼音时，按下 `Shift` 键，立即针对**所有**匹配结果开启全局精准筛选。
- **粘性输入**：进入筛选后，后续输入的字母会自动追加为筛选码（如 `liP`, `liPo`），第一个字母大写展示，视觉辨识度极高。
- **自动上屏**：当筛选结果减少到仅剩 1 个时，系统会自动将候选词上屏并重置状态。

### 10. 长句预览编辑框 (Sentence Preview)
**在“双空格上屏”模式下，提供沉浸式的长句编辑体验。**
- **编辑框 UI**：候选窗顶部新增深色编辑框，实时展示当前整句组合结果。
- **视觉光标**：编辑框内自带视觉光标，清晰展示当前拼写进度与空格状态。

### 11. 专有名词原生隔离 (Native Case Sensitivity)
- **全小写输入**：`beijing` 只会匹配普通词汇（如：背景）。
- **TitleCase 输入**：输入 <code><b>B</b>eijing</code> 或 <code><b>B</b>e</code> 将精准触发专有名词词库，匹配“北京”，完全排除同音词干扰。

### 12. 多样化的文字发送方式
除了传统的剪贴板模拟粘贴，rust-ime 现已支持更多先进的注入技术：
- **Fcitx5 接口 (推荐)**：直接调用 Fcitx5 内部指令提交文字。**不占用剪贴板**，速度极快，不会干扰用户的复制粘贴历史。
- **Unicode 十六进制输入**：通过系统标准的 `Ctrl+Shift+U` 序列注入，具有极佳的跨平台兼容性。

### 13. 智能混输与多方案自由组合
- **多语种共存**：支持同时勾选 中文、英文、日文 方案。在输入中文时，可以直接输入 `git`, `cargo`, `ls` 等计算机命令而无需切换模式。
- **自由组合**：通过 Web 界面，您可以实现“中+英”、“英+日”等任何混输组合。

### 14. 智能防呆模式 (Anti-Typo)
- **自动拦截**：禁止输入词库中不存在的非法拼音组合（如 `gog`）。
- **即时反馈**：发生拦截时可配置播放错误提示音，确保输入流不断裂。

### 15. 无缝双拼模式 (Seamless Double Pinyin)
**这是本引擎最引以为傲的创新：兼具全拼的兼容性与双拼的高效率。**
- **全拼无缝衔接**：与传统双拼完全割裂全拼不同，本方案**不对 `a, e, i, o, u, h, n, g, v` 这几个全拼核心字母进行任何单键映射**。这意味着您可以随时在双拼和全拼之间切换，没有任何思维负担。
- **双击化解歧义**：针对小鹤双拼中 `s, l, x` 等按键在不同声母后代表不同韵母的“多义性”痛点，本引擎不依赖复杂的语义判定，而是通过**双击 (Double-Tap)** 完美解决。
    - 示例：`l` 在小鹤中既是 `iang` 也是 `uang`。在本方案中，单击 `l` 保持直觉，而**双击 `l, l` 即可精准注入 `uang`**。
- **极致兼容**：由于保留了核心韵母键位，您可以在开启双拼模式的情况下，依然像往常一样输入 `zhuang`, `jiang` 等全拼，系统会自动识别并处理。
- **配置与自定义**：在 Web 配置中心的“双拼方案”和“双击映射”页面，您可以根据自己的习惯微调这些高效映射。

---

## 🖥️ 桌面环境支持与局限

### 已知局限
**候选窗位置固定**：由于采用底层拦截机制，候选窗目前无法跟随光标移动。建议在 Web 配置中心根据屏幕布局调整至底部居中或侧边位置。

### 环境兼容性
| 桌面环境 | 状态 | 说明 |
| :--- | :--- | :--- |
| **KDE Plasma (Wayland)** | ✅ 完美 | 核心测试环境，体验最佳。 |
| **COSMIC (Wayland)** | ✅ 完美 | 核心测试环境，深度适配。 |
| **Windows 10 / 11** | ⚠️ 初步支持 | 基于 TSF 框架，支持图形候选窗与后台模式。 |
| **Hyprland / Sway** | ✅ 支持 | 基于 wlroots 的合成器通常表现良好。 |
| **GNOME (Wayland)** | ⚠️ 受限 | 候选窗可能需要手动调整层级或作为普通窗口显示。 |

---

## 🪟 Windows TSF 支持 (Experimental / Experimental Windows Support)

rust-ime 现已初步适配 Windows 平台，采用原生 TSF (Text Services Framework) 架构。
rust-ime now has preliminary support for Windows using the native TSF architecture.

### Windows 使用指南 (Windows Guide)

1. **环境准备 (Preparation)**：
   - **以管理员身份**打开 PowerShell (Open PowerShell **as Administrator**).
   - 执行一键编译并注册：`.\make_windows_release.ps1` (这将自动编译程序、DLL 并完成 TSF 注册).

2. **切换目录 (Navigate to Directory)**：
   ```powershell
   cd "C:\Users\xa\Documents\rust_ime"
   ```

3. **注册与注销 (Registration)**：
   - **注册 (Register)**：`.\make_windows_release.ps1` (to package) or use the generated `install.bat`.
   - Manual Register: `.\target\debug\rust-ime.exe --register`
     *Once registered, add "Rust IME" in Windows Settings -> Language -> Input Methods.*
   - **注销 (Unregister)**：`.\target\debug\rust-ime.exe --unregister` or use `uninstall.bat`.

4. **运行服务 (Running the Service)**：
   - **后台运行 (Background)**：`.\target\debug\rust-ime.exe` (or use `--daemon`)
   - **前台调试 (Foreground/Debug)**：`.\target\debug\rust-ime.exe --foreground`
   - **停止服务 (Stop)**：`.\target\debug\rust-ime.exe --stop`

### Windows 特有功能 (Windows Specific Features)
- **原生分层窗口 (Native Layered Windows)**：Win32 API based high-performance candidate window.
- **系统托盘图标 (System Tray)**：Switch modes, select dictionaries, and toggle features.
- **系统通知 (System Notifications)**：Feedback via Windows Toast notifications.
- **自动开机自启 (Autostart)**：`.\target\debug\rust-ime.exe --install` writes to the registry.

---

## 🛠 命令行参数 (Command Line Options)

`rust-ime` 默认以后台守护进程模式运行。

| 参数 | 功能描述 |
| :--- | :--- |
| **(无参数)** | **后台模式** (默认)。自动转入后台，日志输出至 `/tmp/rust-ime.out`。 |
| **`--foreground`** | **前台调试模式**。在当前终端实时显示运行日志，适合排查问题。 |
| **`--daemon`** | 显式开启后台模式运行。 |
| **`--stop`** | **一键停止**。杀掉所有正在运行的后台 `rust-ime` 进程。 |
| **`--install`** | **安装自启**。在系统启动项中创建配置，实现开机自动加载。 |

---

## ⌨️ 默认快捷键 (Default Hotkeys)

| 快捷键 | 功能描述 |
| :--- | :--- |
| **`Tab` (单击)** | **切换 中文 / 直通 (无输入法)** 模式 |
| **`Tab` (按住) + `Caps`** | 临时发送物理 `CapsLock` 给系统 |
| **`CapsLock` (单击)** | 进入 **当前页筛选模式** (仅在候选词显示时) |
| **`Shift` (单击)** | 进入 **全局筛选模式** (仅在候选词显示时) |
| **`Shift + Space`** | **直接上屏当前候选词的提示 (Hint)** |
| **`字母键` (双击)** | **双击快捷输入 (Double-Tap)** |
| **`字母键` (长按)** | **长按行为定制 (Long Press)** |
| **`/` (斜杠)** | **平/卷舌音互换** (在输入拼音时) |
| **`` ` (反引号) ``** | **拼音状态**：进入 HJKL 导航模式 / **空闲状态**：方案切换模式 |
| **`switch + t/b`** | 在方案切换模式下，将窗口移至 **顶部(t)** 或 **底部(b)** |
| **`Ctrl + Alt + S`** | 循环切换活跃方案 (中 -> 英 -> 日 -> 混输) |
| **`Ctrl + Alt + D`** | **开启 / 关闭 双拼模式** |
| **`Ctrl + Alt + V`** | 切换发送方式 (Fcitx5 -> 剪贴板 -> Unicode) |
| **`Ctrl + Alt + M`** | 切换上屏模式 (单空格词模式 / 双空格句模式) |
| **`← / →`** | 移动光标 / 循环选择候选词 |
| **`1 - 5`** | **数字选词** (默认每页 5 个词，可关闭) |
| **`Space`** | **立即上屏** (词模式) / 手动分词 (句模式) |
| **`Enter`** | 确认当前输入缓冲区内容直接上屏 |
| **`Ctrl + Alt + G`** | 显示 / 隐藏 **传统窗口** |
| **`Ctrl + Alt + H`** | 显示 / 隐藏 **卡片式窗口** |
| **`Ctrl + Alt + K`** | 开启 / 关闭 **按键显示** |
| **`Ctrl + Alt + N`** | 开启 / 关闭 **系统通知候选词** |

---

## 📦 安装步骤 (Installation)

Please refer to the following guides for detailed installation instructions:
请参考以下指南了解详细安装步骤：

- **English**: [INSTALL_GUIDE.md](INSTALL_GUIDE.md)
- **中文**: [INSTALL_GUIDE_ZH.md](INSTALL_GUIDE_ZH.md)

### Quick Start (Linux):
1. Extract the release package.
2. Run `bash ./install.sh` in the terminal.
3. Ensure your user is in the `input` group.
4. **Restart your computer**.

### Quick Start (Windows):
1. Extract the ZIP package.
2. Right-click `install.bat` and **Run as Administrator**.

---

## 🌐 Web 配置中心

程序运行后，访问 **[http://127.0.0.1:8765](http://127.0.0.1:8765)** 即可进入功能强大的 Web 配置中心。
- **双击与快捷**：自定义双击判定时长及按键映射（如 `ii` ⮕ `ing`）。
- **开关矩阵**：自由开启/关闭 Shift筛选、CapsLock选词、数字键选词、自动上屏等。
- **视觉样式**：实时调整颜色球、字号、边距、阴影。
- **词库管理**：支持递归扫描子目录下的词典，一键重新编译。
- **词典编辑器**：快速搜索并修改任何词条的翻译/提示。
- **汉字学习**：挑选您的单词本，在屏幕角落开启背单词模式。