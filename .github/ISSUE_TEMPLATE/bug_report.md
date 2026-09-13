---
name: Bug 报告
about: 行为不对、输出错了、程序崩了
title: "[bug] "
labels: bug
---

<!--
先看 docs/faq.md：不少「不对」其实是取数口径差异（例如内存与 free 的对比）。
安全问题请不要开公开 issue，见 SECURITY.md。
-->

**版本**（`vitals --version` 的输出）：

**发行版与内核**（`/etc/os-release` 的 NAME/ID，`uname -sr`）：

**做了什么**

```console

```

**期望看到**

**实际看到**

**`vitals --verbose` 的 stderr 输出**（含最终生效的设置）：

```

```

**相关模块的诊断**（有的话，这两条命令的输出）：

```console
$ vitals --explain --module <模块名>
$ vitals --sources --module <模块名>
```
