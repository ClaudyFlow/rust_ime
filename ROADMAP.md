# Rust-IME 架构演进路线图 (Architecture Roadmap)

本项目目前已完成从 0 到 1 的原型开发，实现了基于 TSF (Windows) 和 evdev (Linux) 的输入法核心逻辑。为了将项目推向工业级水平，未来的工作重点将从“功能堆砌”转向“架构治理”。

---

## 📅 阶段一：基础加固与观测 (Foundation & Stability)
*目标：提升系统透明度，规范底层通信，减少低级同步 Bug。*

- [x] **UI 抽象层实现**：已引入 `CandidateDisplay` trait，解耦 Slint 窗口与 Linux 桌面通知。
- [ ] **结构化日志系统**：引入 `tracing` 框架替代 `println!`，支持日志持久化。
- [ ] **IPC 协议规范化**：将跨线程/跨进程通信改为强类型的 `IpcMessage` 枚举。
- [ ] **健壮的错误处理**：减少 `unwrap()`，建立统一的 `AppError` 及 Panic 恢复机制。

---

## 📅 阶段二：状态管理与并发重构 (State & Concurrency)
*目标：解决多线程状态同步冲突，提升 UI 响应速度。*

- [ ] **单一数据源 (SSoT) 架构**：建立全局唯一的 `AppState` 状态机，UI 改为“观察者模式”。
- [ ] **解耦主循环**：将 `main.rs` 职责拆分为独立的 Service（Ipc, Gui, Config, Tray）。
- [ ] **无锁输入流水线**：将 `Processor` 放入独立线程，移除频繁的 `Mutex` 锁定，通过消息驱动提高流畅度。

---

## 📅 阶段三：核心引擎流水线化 (Pipeline Architecture)
*目标：仿照 Rime 架构，将 God Object `Processor` 拆解为可插拔的流水线。*

- [ ] **三段式处理流程**：
    - **Preprocessor**: 处理按键映射、双拼转换、特殊快捷键。
    - **Translator**: 输入解析序列，输出候选词列表。支持 `TableTranslator` (本地), `LuaTranslator` (脚本), `CloudTranslator` (网络)。
    - **Filter**: 结果二次加工（去重、繁简转换、Emoji 过滤）。
- [ ] **Schema 驱动**：通过配置文件定义输入方案，而非在 Rust 代码中硬编码逻辑。

---

## 📅 阶段四：数据层性能优化 (Storage & Speed)
*目标：极速启动，支持百万级超大规模词库。*

- [ ] **静态词库 mmap 化**：使用 Memory Mapped File (如 `fst` 或自定义二进制格式) 加载系统词库，实现零延迟启动。
- [ ] **用户数据持久化**：引入 **SQLite** (或 `sled`) 存储用户词频和学习记录，确保数据一致性与事务安全。
- [ ] **冷热分离**：高频词保留在内存高速 Trie，低频词/长词按需从磁盘索引。

---

## 📅 阶段五：Linux 输入层进化 (Linux Input Evolution)
*目标：从“模拟按键”转向“标准输入协议适配”。*

- [ ] **InputHost 适配器化**：支持用户在设置中切换不同的后端：
    - `HardwareInterceptor` (现有基于 evdev/uinput 方案)。
    - `Fcitx5Frontend` (实现 Fcitx5 D-Bus 协议，支持原生 Wayland 与光标跟随)。
    - [ ] **WaylandProtocol**：直接实现 `text-input-v3` 和 `input-method-v2` 协议，解决免 Root 权限运行和像素级光标跟随问题（优先适配 KDE Plasma 6/KWin）。

    ---

    ## 📅 阶段八：移动端与多端同步 (Mobile & Sync)
    *目标：将 Rust 核心带入移动领域。*

    - [ ] **Android 核心移植**：使用 `uniffi-rs` 或 `jni-rs` 将核心引擎封装为安卓动态库（.so）。
    - [ ] **软键盘 UI 开发**：基于安卓原生系统实现高度自定义的键盘视图，适配触摸交互。
    - [ ] **云同步 (可选)**：建立基于 WebDAV 或私有云的用户词库同步机制，打通手机与电脑的输入习惯。

    ---

    > **架构师寄语**：好的代码是演化出来的，不是一次性设计出来的。先保持项目能跑，再通过局部的重构让它跑得更好。
