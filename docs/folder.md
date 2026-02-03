**pageindex-rs** 作为一个独立的 Rust 库，并在 **Knot** 这个 Tauri 项目中调用它，最推荐的组织方式是使用 **Cargo Workspace**。

这种结构允许你在一个大项目目录下管理多个 Crate，既保证了库的独立性（可以单独发布、测试），又保证了开发时的无缝集成（Knot 可以直接通过相对路径引用尚未发布的 pageindex-rs）。

---

## 推荐的目录结构

```text
knot-workspace/           # 根目录 (Workspace Root)
├── Cargo.toml            # 工作空间配置文件
├── pageindex-rs/         # 独立库：核心解析逻辑
│   ├── Cargo.toml
│   └── src/
│       ├── lib.rs        # 暴露 Parser Trait 和核心数据结构
│       ├── core/         # 树构建、Token 计算、Thinning 逻辑
│       ├── formats/      # 各类格式实现 (md.rs, office.rs, pdf.rs)
│       └── vision/       # 坐标聚合、表格重建算法 (与具体模型无关)
├── knot-app/             # Tauri 项目：UI 与 系统集成
│   ├── Cargo.toml
│   ├── src-tauri/        # Rust 后端逻辑
│   │   ├── src/
│   │   │   ├── main.rs   # Tauri 入口，初始化模型，注入引用
│   │   │   ├── vision/   # 具体模型实现 (Florence2/Qwen-VL 的 ort 调用)
│   │   │   └── agents/   # 闹钟、Todo 等指令解析执行器
│   │   └── Cargo.toml    # 这里会引用 ../../pageindex-rs
│   └── src/              # 前端 UI 代码 (Svelte/React)
└── models/               # 存放离线模型文件 (Florence-2, Llama-3.2 等)

```

---

## 关键配置文件实现

### 1. 根目录 `knot-workspace/Cargo.toml`

定义工作空间，管理公共依赖版本。

```toml
[workspace]
members = [
    "pageindex-rs",
    "knot-app/src-tauri",
]
resolver = "2"

[workspace.dependencies]
serde = { version = "1.0", features = ["derive"] }
tokio = { version = "1.0", features = ["full"] }

```

### 2. 库目录 `pageindex-rs/Cargo.toml`

保持轻量，通过 Feature 管理重型功能。

```toml
[package]
name = "pageindex-rs"
version = "0.1.0"
edition = "2021"

[dependencies]
serde.workspace = true
pulldown-cmark = "0.10" # 核心依赖
undoc = { version = "0.1", optional = true } # Office 支持

[features]
default = []
office = ["dep:undoc"]
# 只有开启 vision feature 才会编译坐标聚合等视觉相关算法
vision = [] 

```

### 3. 应用目录 `knot-app/src-tauri/Cargo.toml`

引用本地的工作空间库。

```toml
[package]
name = "knot-app"
version = "0.1.0"
edition = "2021"

[dependencies]
# 核心：通过路径引用本地库，开启所有特性
pageindex-rs = { path = "../../pageindex-rs", features = ["office", "vision"] }

# 应用层特有的推理引擎依赖
ort = { version = "1.16", features = ["load-dynamic"] }
tauri = { version = "2.0", features = [] }

```

---

## 开发流建议

1. **解耦开发**：你可以进入 `pageindex-rs` 目录运行 `cargo test`。由于它不依赖 `ort` 这种复杂的推理库，测试速度会极快。
2. **Mock 驱动**：在开发 `pageindex-rs` 的坐标聚合算法时，在 `tests/` 目录下准备一些固定的坐标 JSON 文件，注入一个返回这些数据的 `MockVisionProvider`。
3. **Tauri 调用**：在 `knot-app` 的 Tauri Command 中，只需导入 `pageindex_rs::PageIndex` 即可。

---