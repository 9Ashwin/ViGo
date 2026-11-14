# ViGo

**按住 V，把你的 Dock 放到手边。**

Rust 原生桌面轮盘。macOS 使用 AppKit 并直接跟随系统 Dock；新增 Windows 11 x64 测试版，使用 Win32 和系统任务栏，不维护另一套软件列表。

![ViGo：全量 Dock、原生玻璃轮盘与小猫反馈](docs/images/vigo-demo.gif)

- Dock 的固定、运行中及最近项目按系统顺序自动更新，不需要点同步；在 Dock 中添加、移除或排序即可。
- 所有 Dock 项目一次显示，不限 24 个，也不分组翻页。主轮盘保持紧凑，前 8 项在内环，其余顺序排到外圈，必要时增加外圈；边缘自动避让。
- 选中目标按软件身份保持；Dock 实时变化时重排，目标被删除则取消，避免松手切错。
- 软件对应的窗口和浏览器标签自动显示在旁边，可直接松手切换。支持台前调度及豆包浏览器。
- 轻按 V 正常输入，⌘V 等组合键保持原行为；长按约 150 毫秒呼出，回到中心、原呼出位置附近、Esc 或右键取消。
- 中心小猫使用原始多帧图集，带方向、待机及轻微跟随反馈；停留时放缓，隐藏后停止绘制。
- 原生菜单栏下拉提供启用/暂停、呼出键、尺寸、明暗外观、登录启动、权限和退出。没有独立设置页。

软件外圈承接溢出项目，悬停后的附加列表展示真实窗口与标签；不提供自定义文件夹、网址或截图预览。Dock 自身的文件夹项目仍按系统来源显示。废纸篓和分隔线不作为启动项目。

## 权限与更新

仅需要辅助功能识别全局按键、读取 Dock 和切换窗口/标签。不捕获窗口画面，不请求屏幕录制。

权限请求只在主动点击菜单中的权限入口时发生。固定签名、应用 ID `com.vigo.desktop` 和安装路径 `/Applications/ViGo.app`，保留后续更新的授权身份。

旧自定义列表不再导入，迁移前保存在 `~/Library/Application Support/ViGo/settings.before-dock.json`。新配置只存呼出和显示偏好，Dock 始终只读。

## Windows 11 x64 测试版

下载 Actions 中的 `ViGo-Windows-x64`，解压里面的 ZIP，运行 `ViGo/ViGo.exe`。无需安装器、WebView 或管理员权限。按住 V、移动、松开选择；Esc 或右键取消，托盘小猫提供暂停、任务栏状态和退出。

通过 UI Automation 读取 Explorer 的实际应用按钮与屏幕顺序，每秒自动更新。应用固定、启动、关闭和重排不需要手动同步；识别失败会保留最后一次有效列表，并在“任务栏状态”显示原因。没有用快捷方式目录或窗口枚举假冒任务栏顺序。外圈窗口和标签依赖应用提供 UI Automation；应用 ID 与 exe 路径分别识别，标签切换前核验对象与窗口身份。

Windows x64 测试包 ZIP 约 1.69 MiB，EXE 约 1.75 MiB，素材已内嵌，解压即用；运行内存尚未在真实 Windows 11 上测量。

当前是未签名测试包。Windows 的半透明轮盘使用原生分层窗口；尚未实现与 macOS 完全相同的玻璃材质。已覆盖远端 Windows Server runner 的编译和解压后 GUI 绘制冒烟；**真实 Windows 11 的任务栏、输入法、全局 V 手势及多屏体验仍待实机验证**。系统不允许向更高权限程序重放按键，因此管理员程序前台会直接透传 V。

```powershell
cargo test --locked
cargo clippy --locked --all-features -- -D warnings
./tools/windows/package.ps1
./tools/windows/smoke.ps1
```

## GitHub Actions

[Native builds](https://github.com/9Ashwin/ViGo/actions/workflows/build.yml) 在 push、PR 和手动触发时执行格式、测试、Clippy，并分别生成 macOS Apple Silicon 和 Windows x64 ZIP、SHA-256 与验证报告。产物在运行页的 Artifacts 保留 14 天；Windows 会解压打好的 ZIP，再启动其中的 EXE 验证窗口、透明绘图和内嵌小猫素材。

CI macOS 包使用 ad-hoc 签名，未公证；Windows 包未签名。它们用于测试分发，macOS 本机覆盖更新继续使用固定 Apple Development 身份，CI 不上传本机私钥。旧 Electron 版本保留在 [`v0.2.1-electron`](https://github.com/9Ashwin/ViGo/tree/v0.2.1-electron)，`main` 为 Rust 原生版本。

## macOS 开发

需要 macOS 13+、Rust、Xcode 命令行工具和本机有效的 Apple Development 签名身份；本机已验证 Apple Silicon。macOS 26+ 使用系统玻璃，旧系统回退原生毛玻璃。

```sh
cargo test
cargo fmt --check
cargo clippy --all-features -- -D warnings
sh tools/native/bundle.sh
sh tools/native/install.sh
```

产物为 `release/native/ViGo.app`。安装脚本退出旧进程、保留旧包备份后替换，重新登记应用并刷新该应用的 Spotlight 元数据。图标资源名随内容哈希更新，避免复用旧图标缓存。可用 `VIGO_SIGN_IDENTITY` 指定固定签名身份。

正常构建不包含开发诊断入口，运行不依赖 Electron、Node 或 WebView。窗口及 Dock 查询在独立工作中执行，空闲不绘制轮盘。

## 占用与平台边界

Apple Silicon / macOS 27 本机实测：0.5.0 本机压缩包约 5.6 MiB，安装约 6.6 MiB；冷待机物理 footprint 约 16.3 MiB，17 项双环打开时约 114.4 MiB，关闭后稳定约 47.4 MiB（按主进程及其子进程合计抽样）。图标、原生玻璃和绘图缓存会使占用变化，以上不是最大值承诺。旧 macOS 及多显示器尚需对应设备验证。

浏览器未提供稳定标签标识时，只接受唯一标题与原位置核验；身份变化会取消，避免切错。

## 许可

代码采用 MIT 许可，保留第三方版权见 [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)。Rust 依赖许可随包附带，美术素材说明见 [assets/README.md](assets/README.md)。
