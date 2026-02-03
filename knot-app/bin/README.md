# llama-server 二进制文件目录

本目录用于存放各平台的 `llama-server` 可执行文件。这些文件会在编译时通过 `include_bytes!` 内嵌到 `rag-core` 库中。

## 所需文件

### Windows 平台

| 文件名 | Feature Flag | 说明 |
|--------|--------------|------|
| `llama-server-win-cuda.exe` | `cuda` | NVIDIA CUDA 加速版本 |
| `llama-server-win-vulkan.exe` | `vulkan` | Vulkan 加速版本（N卡/A卡/I卡通用） |
| `llama-server-win-cpu.exe` | (默认) | CPU 版本，无显卡加速 |

### macOS 平台

| 文件名 | 说明 |
|--------|------|
| `llama-server-mac-metal` | Universal Binary，支持 Intel 和 Apple Silicon，Metal 加速 |

### Linux 平台

| 文件名 | Feature Flag | 说明 |
|--------|--------------|------|
| `llama-server-linux-cuda` | `cuda` | NVIDIA CUDA 加速版本 |
| `llama-server-linux-vulkan` | `vulkan` | Vulkan 加速版本 |
| `llama-server-linux-cpu` | (默认) | CPU 版本 |

## 下载方式

### 官方发布

从 [llama.cpp releases](https://github.com/ggerganov/llama.cpp/releases) 下载对应平台的预编译版本。

建议使用带 `server` 后缀的版本，例如：
- `llama-b5460-bin-win-cuda-cu12.2.0-x64.zip`
- `llama-b5460-bin-macos-arm64.zip`
- `llama-b5460-bin-ubuntu-x64.zip`

### 手动编译

如需自定义编译，参考 [llama.cpp 官方文档](https://github.com/ggerganov/llama.cpp/blob/master/docs/build.md)。

## 版本管理

当更新二进制文件时，请同步更新 `embedded_binaries.rs` 中的 `LLAMA_SERVER_VERSION` 常量：

```rust
pub const LLAMA_SERVER_VERSION: &str = "b5460";  // 更新为新版本号
```

这会确保运行时自动释放新版本并清理旧版本。

## 占位文件

在没有实际二进制文件时，请创建空白占位文件以允许编译通过：

```bash
touch llama-server-mac-metal
# 或 Windows:
# type nul > llama-server-win-cpu.exe
```

> ⚠️ **注意**：使用占位文件编译的版本无法正常运行 LLM 功能。
