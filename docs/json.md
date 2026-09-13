# JSON 输出

`vitals --json` 把这一屏打成一份 JSON，给脚本读。它不开颜色也不画 Logo——`Settings::resolve`
在 `--json` 时就把这两样关掉了。

形状是**契约**（`src/render/json.rs` 顶部注释的原话）：同一个 `schema_version` 下键名不会漂移，
脚本才敢直接 `jq`。

## 形状

```console
$ vitals --json --module os,memory --logo none
{
  "schema_version": 1,
  "entries": [
    {
      "type": "os",
      "key": "OS",
      "value": "Arch Linux x86_64"
    },
    {
      "type": "memory",
      "key": "Memory",
      "value": "22.25 GiB / 30.65 GiB (73%)"
    }
  ],
  "failures": []
}
```

| 字段 | 类型 | 说明 |
| --- | --- | --- |
| `schema_version` | 整数 | 形状版本，当前是 `1`。只有不兼容的变化才会动它。 |
| `entries[]` | 数组 | 一行输出一项，顺序就是显示顺序。 |
| `entries[].type` | 字符串 | 模块名，例如 `os`。标题行是 `title`。 |
| `entries[].key` | 字符串 | 冒号前的标签；标题行的 `key` 是空字符串。 |
| `entries[].value` | 字符串 | 原样打印出来的值（**是给人看的文本**，见下）。 |
| `failures[]` | 数组 | 采集失败的模块；全部正常时是 `[]`。 |
| `failures[].type` | 字符串 | 模块名，**按用户写的原样**——哪怕这个名字并不存在。 |
| `failures[].error` | 字符串 | 失败原因，与终端上印的那句话相同。 |

## 会被省略的东西

排版原语（`separator`、`break`、`colors`）是「键和值都为空」的条目：它们只在文本版式里有意义，
对读 JSON 的程序来说是一对空字符串，所以**不出现**在 `entries` 里
（`src/render/json.rs` 里 `entries` 的过滤条件，`key.is_empty() && value.is_empty()`）。

标题行保留，它的 `key` 是空字符串。实测：

```console
$ vitals --json --module title,separator,break,colors,os --logo none
{
  "schema_version": 1,
  "entries": [
    {
      "type": "title",
      "key": "",
      "value": "gxyarch@MyArch"
    },
    {
      "type": "os",
      "key": "OS",
      "value": "Arch Linux x86_64"
    }
  ],
  "failures": []
}
```

## 兼容策略

> 加字段不算破坏兼容——读者忽略不认识的键就行；只有不兼容的变化才动 `SCHEMA_VERSION`。

所以：**忽略你不认识的键**，不要因为多了一个键就报错。眼下没有 `variables` 字段；将来若加，
它属于「加字段」，`schema_version` 仍是 `1`。

## `value` 是人看的文本，不保证稳定

`value` 就是屏幕上那一行，跟着语言、locale、采样窗口和机器状态变：

- `uptime`、`memory`、`cpu`、`net-io`、`disk-io`、`top` 这类每次都不同；
- 数字带了给人读的单位（`22.25 GiB`、`73%`、`1 day, 7 hours, 12 mins`）。

**这份 JSON 不提供原始数值字段**，所以别拿它做精确计算。需要可计算的数字时，用
`vitals --sources` 查出该模块真正读了哪些文件，直接读那些文件——那才是权威来源。

## 消费示例

以下每一条都在 `jq` 1.8.2 上跑过：

```console
# 还原成屏幕上的「键: 值」
$ vitals --json --logo none | jq -r '.entries[] | "\(.key): \(.value)"'

# 只要内存那一行
$ vitals --json --logo none | jq -r '.entries[] | select(.type == "memory") | .value'

# 有多少行
$ vitals --json --logo none | jq '.entries | length'

# 有没有模块采集失败（正常时无输出）
$ vitals --json --logo none | jq -r '.failures[]?.error'

# 先看形状版本，再决定怎么解析
$ vitals --json --logo none | jq -r '.schema_version'
```

## JSON Schema

形状也写成了一份 JSON Schema：[`vitals.schema.json`](vitals.schema.json)（draft 2020-12）。
用法与编辑器提示：

```console
# 编辑器 / 校验器可以直接取用
$ python3 -c "import json; json.load(open('docs/vitals.schema.json'))" && echo ok
```

```jsonc
// 给支持 JSON Schema 的工具声明一下
{
  "$schema": "https://raw.githubusercontent.com/juyuan-tech/vitals/master/docs/vitals.schema.json"
}
```

`entries[].value` 是文本而非数字，schema 里也只把它声明成 `string`——这是有意的，见上一节。
