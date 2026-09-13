# JSON Output

**English** | [中文](json.md)

`vitals --json` prints this screen as one JSON document, for scripts to read. It turns on no colors and draws no Logo — `Settings::resolve`
turns both off when `--json` is given.

The shape is a **contract** (the exact words of the comment at the top of `src/render/json.rs`): under the same `schema_version` key names do not drift,
so scripts dare to `jq` it directly.

## Shape

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

| Field | Type | Description |
| --- | --- | --- |
| `schema_version` | integer | Shape version, currently `1`. Only incompatible changes bump it. |
| `entries[]` | array | One item per line of output, in display order. |
| `entries[].type` | string | Module name, for example `os`. The title line is `title`. |
| `entries[].key` | string | The label before the colon; the title line's `key` is an empty string. |
| `entries[].value` | string | The value printed as-is (**it is text for humans to read**, see below). |
| `failures[]` | array | Modules that failed to collect; `[]` when everything is fine. |
| `failures[].type` | string | Module name, **exactly as the user wrote it** — even if that name does not exist. |
| `failures[].error` | string | The failure reason, the same sentence printed on the terminal. |

## Things That Are Omitted

Layout primitives (`separator`, `break`, `colors`) are entries whose "key and value are both empty": they are meaningful only in the text layout,
and to a program reading JSON they are a pair of empty strings, so they do **not appear** in `entries`
(the filter condition for `entries` in `src/render/json.rs`, `key.is_empty() && value.is_empty()`).

The title line is kept; its `key` is an empty string. Measured:

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

## Compatibility Policy

> Adding a field does not break compatibility — readers just ignore keys they do not recognize; only incompatible changes bump `SCHEMA_VERSION`.

Therefore: **ignore keys you do not recognize**, and do not error out just because there is one more key. There is no `variables` field at the moment; if one is added in the future,
it counts as "adding a field", and `schema_version` is still `1`.

## `value` Is Human-Facing Text and Not Guaranteed Stable

`value` is just that line on the screen, and it changes with language, locale, sampling window, and machine state:

- `uptime`, `memory`, `cpu`, `net-io`, `disk-io`, `top` and the like differ every time;
- numbers carry human-readable units (`22.25 GiB`, `73%`, `1 day, 7 hours, 12 mins`).

**This JSON does not provide raw numeric fields**, so do not use it for precise calculations. When you need computable numbers, use
`vitals --sources` to find out which files that module actually read, and read those files directly — those are the authoritative source.

## Consumption Examples

Each of the following has been run on `jq` 1.8.2:

```console
# Reconstruct the "key: value" as shown on screen
$ vitals --json --logo none | jq -r '.entries[] | "\(.key): \(.value)"'

# Only the memory line
$ vitals --json --logo none | jq -r '.entries[] | select(.type == "memory") | .value'

# How many lines
$ vitals --json --logo none | jq '.entries | length'

# Did any module fail to collect (no output when normal)
$ vitals --json --logo none | jq -r '.failures[]?.error'

# Look at the shape version first, then decide how to parse
$ vitals --json --logo none | jq -r '.schema_version'
```

## JSON Schema

The shape is also written as a JSON Schema: [`vitals.schema.json`](vitals.schema.json) (draft 2020-12).
Usage and editor hints:

```console
# Editors / validators can use it directly
$ python3 -c "import json; json.load(open('docs/vitals.schema.json'))" && echo ok
```

```jsonc
// Declare it for tools that support JSON Schema
{
  "$schema": "https://raw.githubusercontent.com/juyuan-tech/vitals/master/docs/vitals.schema.json"
}
```

`entries[].value` is text rather than a number, and in the schema too it is declared only as `string` — this is intentional, see the previous section.
