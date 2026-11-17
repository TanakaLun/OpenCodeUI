# 便携版（Portable）

便携版把 **所有可写数据** 放在 `OpenCode.exe` 同级的 `data\` 目录里，不往
`AppData` / `%LOCALAPPDATA%` / `~/.local` 写任何东西。整个文件夹可以放在 U 盘、
移动硬盘或任意目录，拷走即迁移，删掉 `data\` 即恢复出厂。

## 数据落点

| 原来的位置                                                                                      | 便携版位置              |
| ----------------------------------------------------------------------------------------------- | ----------------------- |
| `%APPDATA%\com.opencodeui.app\window-state.json`、`wsl-servers.json`                            | `data\config\`          |
| `%LOCALAPPDATA%\com.opencodeui.app\logs\`                                                       | `data\logs\`            |
| `%LOCALAPPDATA%\com.opencodeui.app\`（WebView2 配置：localStorage / IndexedDB，即全部 UI 设置） | `data\webview\`         |
| `%USERPROFILE%\.local\share\opencode\`（opencode 数据、认证、会话）                             | `data\opencode\data\`   |
| `%USERPROFILE%\.config\opencode\`（opencode 配置）                                              | `data\opencode\config\` |
| `%USERPROFILE%\.local\state\opencode\`（后台服务注册表 `service.json`）                         | `data\opencode\state\`  |
| `%USERPROFILE%\.cache\opencode\`                                                                | `data\opencode\cache\`  |

实现方式：

- Tauri 的 WebView 数据目录通过 `WebviewWindowBuilder::data_directory()` 指定
  （Tauri 默认会强制指向 `LocalAppData`，环境变量改不动，必须走代码）。
- 窗口状态、WSL 服务器列表、日志统一走 `app::app_config_dir()`。
- opencode 后端在 `src-tauri/src/app/portable.rs` 里通过 `XDG_DATA_HOME` /
  `XDG_CONFIG_HOME` / `XDG_STATE_HOME` / `XDG_CACHE_HOME` 定位（opencode 的
  [`global-roots.ts`](https://github.com/anomalyco/opencode/blob/dev/packages/util/src/global-roots.ts)
  在各平台都优先读这四个变量）。
- 同名 `opencode.exe` 会被优先当作后端（`OPENCODE_BIN` → exe 同目录 → `PATH`）。

## 产物结构

```
OpenCode\
├─ OpenCode.exe         Tauri 外壳（编译时带 portable feature）
├─ opencode.exe         后端（从 opencode Releases 下载）
├─ portable.txt         便携模式标记
├─ README.txt
└─ data\                首次运行自动创建
   ├─ config\
   ├─ logs\
   ├─ webview\
   └─ opencode\{data,config,state,cache}\
```

## 构建

### 方式一：本地 Windows

前置：Node 18+、Rust stable（MSVC 工具链）、Visual Studio Build Tools
（勾选 “使用 C++ 的桌面开发”）。WebView2 运行时在 Win10/11 上一般已预装。

```powershell
npm ci
pwsh scripts/build-portable.ps1
```

脚本会：

1. `tauri build --features portable --no-bundle`（不打安装包，只出 exe）；
2. 从 `anomalyco/opencode` 最新 release 下载 `windows-x64` 后端并放进产物目录；
3. 生成 `portable.txt` / `README.txt`；
4. 输出 `dist-portable\OpenCode\` 和 `dist-portable\OpenCodeUI-portable-windows-x64.zip`。

可选参数：

```powershell
# 已经构建过前端、只想重新打包
pwsh scripts/build-portable.ps1 -SkipFrontend

# 指定后端下载地址（内网/离线镜像）
pwsh scripts/build-portable.ps1 -OpencodeAsset "https://example.com/opencode-windows-x64.zip"

# 不下载后端（自己手动放 opencode.exe）
pwsh scripts/build-portable.ps1 -SkipBackend
```

### 方式二：GitHub Actions

`.github/workflows/portable.yml` 在 `windows-latest` 上跑同一条脚本，
手动触发（Actions → Portable (Windows) → Run workflow）或打 `v*` tag 时执行，
产物是 `OpenCodeUI-portable-windows-x64` artifact。

## 便携模式的开关

编译时带 `--features portable` 的产物 **默认就是便携模式**，同时还有两个运行时开关：

| 方式                            | 效果                                                              |
| ------------------------------- | ----------------------------------------------------------------- |
| exe 同目录放一个 `portable.txt` | 打开便携模式（对没编 feature 的构建也有用，前提是代码包含本改动） |
| `OPENCODEUI_PORTABLE=1`         | 强制打开                                                          |
| `OPENCODEUI_PORTABLE=0`         | 强制关闭（仍然写 `AppData`）                                      |

如果 `data\` 无法创建或不可写（只读介质、装在 `Program Files` 且没有权限），
便携模式会自动退化回用户目录，并在日志里打印一行 `[portable] ... falling back`，
不会因为目录问题启动失败。

> 带 `--features portable` 编出来的就是便携版；不带该 feature 的普通构建（例如
> `npm run tauri build` 出的安装包）仍然写 `AppData`，两者可以并存。

## 验证

启动后检查：

- `data\webview\EBWebView\` 出现，且 `%LOCALAPPDATA%\com.opencodeui.app\EBWebView` 不再更新；
- `data\logs\OpenCodeUI.log` 有内容；
- 设置页里改主题/语言后重启仍生效（说明 localStorage 落在 `data\webview`）；
- `data\opencode\data\opencode\` 里出现认证与会话数据。

## 已知限制

- **WSL 模式不可便携**：WSL 里的 `opencode` 跑在 Linux 发行版内部，数据仍在
  WSL 的 `$HOME`。便携只覆盖本机（Local）后端。
- opencode 的临时文件仍走 `%TEMP%`（`os.tmpdir()`），不影响配置持久化。
- 更新时替换 `OpenCode.exe` 和 `opencode.exe` 即可，`data\` 不用动。
- Windows 之外的平台：代码同样生效（Linux 用 `APPIMAGE` 的所在目录作为基准，
  macOS 用 `.app/Contents/MacOS`），但只在 Windows 上做了打包脚本和验证。
- 不要把整个目录放在网络共享（UNC，如 `\\server\share`）：WebView2 的用户数据
  目录要求本地路径；U 盘、移动硬盘、本地磁盘都没问题。

## 验证状态

- `cargo check --target x86_64-pc-windows-gnu --features portable --bins --lib`
  与不带 `--features portable` 的默认构建均通过，无 warning。
- `src-tauri/src/app/portable.rs` 的路径/开关逻辑有单元测试，`cargo test` 通过。
- 前端 `npm run build` 通过。
- Windows 便携 zip 的实际运行需要在 Windows 上构建后手测（CI 的
  `Portable (Windows)` workflow 产出 artifact）。
