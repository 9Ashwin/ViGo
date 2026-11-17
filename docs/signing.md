# 发布签名与辅助功能

ViGo 的免费发布证书固定在 [release-signing.pem](../native/release-signing.pem)。应用和辅助程序使用相同的指定要求：应用标识 `com.vigo.desktop` 加该证书的指纹。要求没有被弱化为只检查应用名称或 Bundle ID。

macOS 使用指定要求判断更新后的程序是否属于原来授权的身份。旧版 ad-hoc 签名依赖每次构建变化的代码哈希，因此系统可能显示旧开关已开，但新程序依然没有权限。见 [Apple 的代码签名身份说明](https://developer.apple.com/documentation/technotes/tn3127-inside-code-signing-requirements/)。

## 使用与迁移

首次安装、从 0.8.0 或更早版本迁移、或从其他签名身份迁移，仍需要用户完成一次系统授权。若旧记录失效，在 ViGo 的“通用设置 → 修复授权…”操作即可清理本应用的 Accessibility 记录，然后重新开启。不会重置其他应用、其他权限，也不会直接修改 TCC 数据库。

此后使用同一发布证书覆盖更新，代码哈希可以改变，指定要求保持不变。自己重新签名会形成另一个身份。证书固定不能修复 macOS 自身的授权服务故障，也不能代替系统同意。

自签名证书不提供 Apple 开发者身份认证，发布包未公证；首次打开可能需要在“隐私与安全性”里确认。[Apple 文档](https://developer.apple.com/library/archive/documentation/Security/Conceptual/CodeSigningGuide/Procedures/Procedures.html)说明，自签名证书可用于验证签名程序的指定要求，无需把证书加入用户的全局信任根。

## 维护者构建

- 私有发布身份仅存储于维护者受保护的本机配置和 GitHub Actions Secrets，绝不提交到 Git，也不放入安装包。
- `VIGO_RELEASE_P12_BASE64`、`VIGO_RELEASE_P12_PASSWORD` 只在 `main` 的可信构建步骤导入临时钥匙串；PR 不读取。缺少秘密或证书不匹配时，`main` 构建失败，不回退临时签名。
- 构建结束删除临时钥匙串。没有导出或上传维护者个人 Apple Development 私钥。
- 有 `~/.config/ViGo/release-signing/release-identity.p12` 的维护者，本机构建自动使用同一身份；构建只把临时钥匙串位置和公开指纹交给签名步骤，不把 P12 密码传给编译器。
- 其他开发者可用自己的固定身份构建，或显式 `VIGO_SIGN_IDENTITY=- sh tools/native/bundle.sh` 生成一次性测试包。不同签名身份之间不能继承辅助功能授权。
- 不要在每次构建时重建发布证书。更换证书属于身份迁移，需要明确通知用户重新授权一次。

验证发布身份：

```sh
python3 tools/native/signing.py verify /Applications/ViGo.app --release
```

这项检查同时验证应用与辅助程序的完整性、固定证书指纹、指定要求和两者身份一致性；不能把签名检查通过冒充用户已经完成系统授权。
