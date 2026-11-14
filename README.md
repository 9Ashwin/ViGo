# ViGo

**按住 V，把你的 Dock 放到手边。**

Rust + AppKit 原生 macOS 轮盘，直接跟随系统 Dock，不维护另一套软件列表。

<p align="center">
  <img src="docs/images/vigo-demo.gif" alt="ViGo：Dock 软件、原生玻璃轮盘与小猫反馈" width="520" />
</p>

- Dock 的固定、运行中及最近使用的软件按系统顺序自动更新，不需要点同步；在 Dock 中添加、移除或排序即可。
- 所有 Dock 软件一次显示，不限 24 个，也不分组翻页。主轮盘保持紧凑，前 8 项在内环，其余顺序排到外圈，必要时增加外圈；边缘自动避让。
- 选中目标按软件身份保持；Dock 实时变化时重排，目标被删除则取消，避免松手切错。
- 轮盘只显示软件，松手直接启动或切换；支持台前调度。
- 轻按 V 正常输入，⌘V 等组合键保持原行为；长按约 150 毫秒呼出，回到中心、原呼出位置附近、Esc 或右键取消。
- 悬停以图标放大、轻微抬起和柔和阴影反馈选择；玻璃随系统明暗适配，没有彩色扇区或方框底板。
- 中心小猫使用原始多帧图集，带方向、待机及轻微跟随反馈；停留时放缓，隐藏后停止绘制。
- 原生菜单栏下拉提供启用/暂停、呼出键、尺寸、明暗外观、登录启动、权限和退出。没有独立设置页。

外圈只承接放不下的软件。文件夹、文件、废纸篓和分隔线不进入轮盘；不读取浏览器标签，不展示窗口或标签列表。

## 权限与更新

仅需要辅助功能识别全局按键、读取 Dock 和切换软件。不捕获窗口画面，不请求屏幕录制。

权限请求只在主动点击菜单中的权限入口时发生。固定签名、应用 ID `com.vigo.desktop` 和安装路径 `/Applications/ViGo.app`，保留后续更新的授权身份。

旧自定义列表不再导入，迁移前保存在 `~/Library/Application Support/ViGo/settings.before-dock.json`。新配置只存呼出和显示偏好，Dock 始终只读。

## GitHub Actions

[Native builds](https://github.com/9Ashwin/ViGo/actions/workflows/build.yml) 在 push、PR 和手动触发时执行格式、测试、Clippy，并生成 macOS Apple Silicon ZIP 与 SHA-256 校验值。产物在运行页的 Artifacts 保留 14 天。

CI macOS 包使用 ad-hoc 签名，未公证，用于测试分发；macOS 本机覆盖更新继续使用固定 Apple Development 身份，CI 不上传本机私钥。旧 Electron 版本保留在 [`v0.2.1-electron`](https://github.com/9Ashwin/ViGo/tree/v0.2.1-electron)，`main` 为 Rust 原生版本。

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

正常构建不包含开发诊断入口，运行不依赖 Electron、Node 或 WebView。Dock 查询在独立工作中执行，空闲不绘制轮盘。

## 占用与平台边界

Apple Silicon / macOS 27 本机实测：0.5.1 本机压缩包约 5.4 MiB，安装约 6.0 MiB；冷待机物理 footprint 约 16.3 MiB，8 个软件的轮盘打开时约 103.7 MiB，关闭后稳定约 44.4 MiB（按主进程及其子进程合计抽样）。图标、原生玻璃和绘图缓存会使占用变化，以上不是最大值承诺。旧 macOS 及多显示器尚需对应设备验证。


## 许可

代码采用 MIT 许可，保留第三方版权见 [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md)。Rust 依赖许可随包附带，美术素材说明见 [assets/README.md](assets/README.md)。
