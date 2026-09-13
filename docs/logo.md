# Logo 与配色

## 选 Logo

`--logo` 的取值（`vitals --help` 原文）：`auto` 按发行版自动选、`none` 不显示、或直接给名称；
默认是 `auto`。名称**大小写不敏感**（`src/render/logo.rs:71`）。

内置 12 个：

```
alpine  arch  centos  debian  fedora  gentoo  linux  manjaro  nixos  opensuse  ubuntu  void
```

选择次序是 `os-release` 的 `ID` → `ID_LIKE` → 通用图（`src/render/logo.rs:79`）。
`ID_LIKE` 那一步不能省：`ID=cachyos`、`ID=endeavouros` 这类衍生版通常没有自己那张图，
但它们的 `ID_LIKE=arch`，于是能落到 Arch 上。

给一个**不存在的名字不会报错**，会回退到通用图：实测 `--logo definitely-not-a-logo` 与
`--logo auto` 的输出不同——本机 `auto` 选到 Arch，未知名字选到通用图。

```console
$ vitals --logo arch                  # 指定发行版
$ vitals --logo none                  # 不要艺术字
$ vitals --logo none --module os      # 只想看某几项
```

## 配色

- 颜色跟着终端走。`--no-color` 等同于设置 `NO_COLOR` 环境变量（`--help` 原文），两者都能关掉颜色。
- `--json` 会自动关掉颜色与 Logo（`Settings::resolve` 在 `--json` 时处理）。

## `colors` 模块

`colors` 是一个版式原语，用于确认终端配色：

```console
$ vitals --module colors
```

它在 `--json` 里**不出现**：`colors` 的键和值都是空字符串，而 JSON 渲染会过滤掉所有
「键与值皆空」的条目（与 `separator`、`break` 同一类）。见 [`json.md`](json.md)。
