# Configuration Reference

**English** | [中文](configuration.md)

This document explains the `vitals` TOML configuration file: where it lives, how to write it, and what each key means.

Every conclusion in this document comes from three places: the repository source (repository root), the built-in default config
`src/config/default.toml`, and real runs of `target/release/vitals`. Every key is given a
`file:line`; every behavior is annotated with how it was verified. Version: `vitals 0.1.2` (`vitals --version`).

> Output quoted below is verbatim. Runtime text follows `VITALS_LANG` too (`zh` Chinese /
> `en` English); when it is unset, `LC_ALL` / `LC_MESSAGES` / `LANG` decide, and anything
> uncertain means Chinese. The output quoted below therefore comes from English runs
> (`VITALS_LANG=en`).

There is only one path for parsing a config file: `load` in `src/config.rs`, finally deserialized into
`ConfigFile` in `src/config/schema.rs`. Other than that there is no other source of configuration.

---

The repository also ships four example configs that can be run as-is (`presets/`): `minimal`, `desktop`, `headless`, `all` —
`tests/docs.rs` guarantees that they really run.

## 1. Where the config file is

### 1.1 Specifying a path: `--config`

```sh
vitals --config /path/to/config.toml
```

`--config <FILE>` is defined by `src/cli.rs:32-34`, and `src/main.rs:50-56` hands it to
`config::load` (`src/config.rs:105-119`). **An explicitly specified file must be readable**: if the file does not exist,
you have no permission, or it is not UTF-8, an error is reported and it exits with exit code 1 (reading and the size cap
`src/config.rs:61-64`, `71-96`; parsing and the version check `src/config.rs:122-136` → `src/main.rs:52-55`).

Measured:

```console
$ vitals --config /tmp/vitals-docs-tests/nope.toml --logo none
vitals: failed to read config file /tmp/vitals-docs-tests/nope.toml: No such file or directory (os error 2)
[exit=1]
```

### 1.2 Not specifying it: `$XDG_CONFIG_HOME/vitals/config.toml`

When `--config` is not written, the path is computed in the following order (`src/config/path.rs:25-45`):

1. The environment variable `XDG_CONFIG_HOME` exists **and is an absolute path** → `$XDG_CONFIG_HOME/vitals/config.toml`
   (`src/config/path.rs:40-42`; a relative path is treated as invalid per the XDG spec — not an error, but it continues to the next case).
2. Otherwise, the environment variable `HOME` exists → `$HOME/.config/vitals/config.toml`
   (`src/config/path.rs:44`).
3. If neither exists → there is no default path, and `config_path()` returns `None`
   (`src/config/path.rs:22-23`, `src/config.rs:110-112`).

The relative path constant `vitals/config.toml` is at `src/config/path.rs:18`.

When the path that was found **does not exist, no error is reported**; it quietly uses the built-in default (`src/config.rs:114-118`) — someone
running it for the first time should not see an error. This is the only behavioral difference between the explicit path and the default path (`src/config.rs:98-104`).

Measured (one real run per line):

| Environment | Config file contents | Result |
| --- | --- | --- |
| `XDG_CONFIG_HOME=/tmp/…/xdghome` | `type = "kernel"` | Only `Kernel: Linux 7.2.4-arch1-2` is shown |
| `XDG_CONFIG_HOME` unset, `HOME=/tmp/…/homefallback` | `type = "uptime"` | Only `Uptime: …` is shown |
| `XDG_CONFIG_HOME=relative/dir`, `HOME=/tmp/…/homefallback` | same as above | the relative XDG is ignored, and `$HOME/.config/vitals/config.toml` is still read |
| `XDG_CONFIG_HOME=/tmp/…/xdgempty` (there is no `vitals/config.toml` in the directory) | — | the built-in default view, exit code 0 |
| Neither `XDG_CONFIG_HOME` nor `HOME` is set | — | the built-in default view, exit code 0 |

### 1.3 Platform scope

`src/config/path.rs:11-12` states that the Windows / macOS branches are left until v1.0, and currently there is only one
Unix implementation. Therefore this document only describes the Unix path rules above and does not describe the behavior of Windows / macOS.

**Note**: only `XDG_CONFIG_HOME` is recognized here; other variables such as `XDG_CONFIG_DIRS` are not read;
`src/config.rs:9` also states that at the current stage there are only two layers, "the built-in default + the user file", with no system-level
config merging.

---

## 2. Overall semantics: replaces the whole list, not item-by-item merging

**Once `modules` is written, it replaces the whole list of built-in defaults, and is not appended to the default list.**
If you want to show fewer modules, you have to list all the ones you want to keep. This is verbatim from the `--gen-config` output
(`src/config/default.toml:3-4`), and the source agrees with it: `ConfigFile.modules` is an `Option`, and it
only overrides when it is `Some` (`src/config/schema.rs:31-36`, `48-50`); the comment gives the reason —
item-by-item merging of lists has no defensible semantics (how items are ordered, whose a duplicate is, `src/config/schema.rs:33-35`).

All `ConfigFile` fields are `Option`, in order to distinguish "not written" from "written with a value equal to the default"
(`src/config/schema.rs:5-6`). Therefore:

| What you write | Result |
| --- | --- |
| Do not write `modules` at all (you wrote other keys, or an empty file, or there is no config file) | uses the built-in default view (24 modules, see §3) |
| Write `[[modules]] ...` | only the ones you list are shown, and the built-in default list no longer applies |
| Write `modules = []` | 0 modules, no output at all, exit code 0 |

`config_version` and the module list are independent of each other: if it is written in the file, the value in the file is used; if not, the current version is assumed
(`src/config/schema.rs:42-46`).

Measured (writing only half the modules):

```console
$ printf '[[modules]]\ntype = "memory"\n' > t1-replace.toml
$ vitals --config t1-replace.toml --logo none --no-color
Memory: 22.78 GiB / 30.65 GiB (74%)
[exit=0]
```

Only `memory` was written, and `OS` / `Kernel` / `Disk` etc. all disappeared — it is not a merge. The other three cases were each
run once too: writing only `config_version = 1`, an empty file, and not passing `--config` at all
(with neither `XDG_CONFIG_HOME` nor `HOME` set); all three list the same modules, all are the built-in default view,
exit code 0; while `modules = []` gives empty output and exit code 0.

(The exact number of output lines is **not a conclusion**: some modules in the default view depend on the environment; for example, after also clearing
`HOME`, measurement shows that the appimage entry of `Packages` and the `Terminal Font` line disappear. All that is guaranteed here is that "the module list
returns to the built-in default".)

### What the built-in default view is

The built-in default list is `ModuleType::DEFAULT`, 24 entries, and the order is the display order
(`src/config/schema.rs:429-455`), matching `src/config/default.toml:41-111` item by item:

```text
title, separator, os, host, kernel, uptime, packages, shell, display, de, wm,
cursor, terminal, terminal-font, cpu, gpu, memory, swap, disk, local-ip,
battery, locale, break, colors
```

In `src/config.rs:37-39`–`46-51`, `default_toml_for()` picks a template by language: Chinese
`include_str!("config/default.toml")`, English `include_str!("config/default.en.toml")`; both parse back into the same `Config`.
`--gen-config` prints the selected one as-is (`src/main.rs:38-41`). In testing, the output of `vitals --gen-config` is byte-for-byte identical to
`/tmp/vitals-facts/gen-config.toml` (`diff` shows no difference).

---

## 3. Per-key reference

There are **only 6 keys** in the config file: 2 at the top level, and 4 in each `[[modules]]` entry. There is hard
source evidence for this set: both structs carry `deny_unknown_fields` (`src/config/schema.rs:25`, `:90`), so
when a key name is misspelled serde lists the legal set. The verbatim error from an actual run:

```text
unknown field `moduless`, expected `config_version` or `modules`
unknown field `foo`, expected one of `type`, `platforms`, `when-command-exists`, `when-file-exists`
```

### 3.1 Top level

| Key | Type | Default | Purpose | Location |
| --- | --- | --- | --- | --- |
| `config_version` | Integer (u32) | `1` (when omitted, `CURRENT_CONFIG_VERSION` is taken) | Declares which config version this file targets. When it is **higher** than the version the program supports, startup is refused | Field `src/config/schema.rs:28-29`; default value source `src/config/schema.rs:19-21`, `src/config.rs:29`; validation `src/config.rs:128-133` |
| `modules` | `[[modules]]` array table | omitted = the built-in default 24 entries | The modules to be shown, **the order is the display order**; once written, it replaces the whole list of built-in defaults | Field `src/config/schema.rs:36`; override logic `src/config/schema.rs:48-50`; default list `src/config/schema.rs:429-455` |

Boundary behavior of `config_version` (all measured):

- Omitted → treated as `1`, normal.
- Equal to `1` (`CURRENT_CONFIG_VERSION`) → normal.
- Less than `1`, for example `0` → **no error**, renders normally. Validation has only the "higher" rule
  (`src/config.rs:128`).
- Greater than `1`, for example `2` → exit code 1, verbatim: `vitals: config version 2 is newer than version 1, the newest this build supports; upgrade vitals`.
- The type is not an integer, for example `config_version = "1"` → TOML parse failure, exit code 1
  (`invalid type: string "1", expected u32`).

Boundary behavior of `modules`: order = the order you wrote; **duplicates are allowed**, and a duplicate entry renders twice
(measured: two consecutive `type = "kernel"` output two `Kernel: …` lines). There is no deduplication logic.

### 3.2 Each `[[modules]]` entry

| Key | Type | Default | Purpose | Location |
| --- | --- | --- | --- | --- |
| `type` | String (enum, kebab-case) | **required, no default** | Module name. For values see §3.3 | `src/config/schema.rs:93-94`; enum definition `src/config/schema.rs:210-345` |
| `platforms` | Array of strings | `[]` (empty = no platform restriction) | Collect only on these platforms, otherwise the module is skipped | `src/config/schema.rs:97-98`; value enum `src/config/schema.rs:129-150` |
| `when-command-exists` | String | unset = no check | If the command is not in `PATH`, the module is skipped; **only PATH is checked, the command is not executed** | `src/config/schema.rs:100-102`; evaluation `src/conditions.rs:113-145` |
| `when-file-exists` | String (path) | unset = no check | If the path does not exist, the module is skipped; a leading `~/` expands to `$HOME` | `src/config/schema.rs:104-106`; evaluation and expansion `src/conditions.rs:95-99`, `172-187` |

When `type` is missing, parsing fails outright (measured): `missing field \`type\``, exit code 1.

**Spelling rules for `type`**: `type` in the config file goes through serde enum matching, and must be our own
kebab-case name; **neither the case nor `-`/`_` is normalized** (`src/config/schema.rs:541-547`).
Measured: `"LocalIp"`, `"local_ip"`, and `"WMTheme"` are all parse errors with exit code 1, and only
`"local-ip"` is normal; yet these same spellings all work in the command-line `--module`
(`src/cli.rs:118-127`, `src/config/schema.rs:530-556`) — the two paths are intentionally different.

Two names do not follow the mechanical kebab-case conversion and are pinned down by an explicit `serde(rename)`
(`src/config/schema.rs:281-287`, `:317-322`); they are written `wmtheme` and `datetime`
(rather than `wm-theme`, `date-time`).

### 3.3 Legal values of `type`

**61** in total (`ModuleType::ALL` at `src/config/schema.rs:349-411`, `name()` at
`:463-527`). In testing, `vitals --list-modules` outputs 61 lines, and it is exactly this sequence, in this order:

```text
os, host, kernel, bios, board, chassis, uptime, loadavg, processes, cpu, memory,
swap, disk, user, shell, terminal, terminal-size, locale, editor, version,
init-system, title, separator, break, rust, battery, power-adapter, brightness,
dns, tpm, packages, display, de, wm, wmtheme, theme, icons, font, cursor, gpu,
terminal-font, local-ip, users, physical-disk, bootmgr, sound, cpu-cache, lm,
btrfs, datetime, wifi, camera, keyboard, mouse, gamepad, net-io, disk-io,
cpu-usage, top, colors, monitor
```

When you misspell one letter, serde lists all the legal values in the error, for example
`unknown variant \`cpuu\`, expected one of \`os\`, \`host\`, …` (measured, exit code 1).

`type` has no configurable options such as "color" or "layout"; `title` / `separator` / `break` /
`colors` are rendering primitives and have no extra fields (`src/config/schema.rs:255-260`, `:333-334`).

### 3.4 Legal values of `platforms`

9 (`src/config/schema.rs:129-150`, the names are at `:168-180`):

```text
linux, macos, windows, freebsd, openbsd, netbsd, android, solaris, illumos
```

Writing an illegal value causes a parse failure, exit code 1 (measured with `platforms = ["linx"]`:
``unknown variant `linx`, expected one of `linux`, `macos`, …``).
`platforms = []` means no platform restriction, and the module is shown as usual (`src/conditions.rs:109-111`, measured passing).

---

## 4. Conditions

Conditions hang off each `[[modules]]` entry, using any combination of the three fields above. **When they are not satisfied the module is skipped,
and no error is reported** — "this machine has no battery" is not an error, there is simply nothing to show
(`src/conditions.rs:3-4`). When skipped, the exit code is still 0, and that line simply disappears from the output.

The relationship of the three conditions is **AND**: if any one is not satisfied, it is skipped (`src/conditions.rs:82-83`). The evaluation order is
`platforms` → `when-command-exists` → `when-file-exists`, and **the one that fails first** determines the skip reason (`src/conditions.rs:84-102`).

Conditions are evaluated uniformly before collection, and the scheduler only sees the final list (`src/conditions.rs:10`, `64-78`;
`src/main.rs:90`).

### 4.1 How each of the three conditions is judged

**`platforms`** — an empty array means no restriction; otherwise the current platform must be in the list
(`src/conditions.rs:109-111`). The current platform is taken from `std::env::consts::OS`, which is a compile-time constant
(`src/config/schema.rs:191-199`). If the compilation target is not among those 9 names, any non-empty
`platforms` is not satisfied — this is intentional (`src/config/schema.rs:194-195`,
`src/conditions.rs:104-108`).

**`when-command-exists`** — only `PATH` is checked, **the command is never executed** (`src/conditions.rs:5-8`,
`113-119`). Details:

- An empty string never matches (`src/conditions.rs:129-131`). Measured: `when-command-exists = ""`
  is skipped, and `--verbose` displays ``the command `` is not on PATH``.
- If the name contains `/`, no PATH lookup is done and it is treated directly as a path (`src/conditions.rs:133-136`).
  Measured: `/bin/sh` passes, `/bin/vitals-does-not-exist` is skipped.
- It must be a **regular file with the execute bit set**: if a file with the same name exists but has no execute bit, it still counts as "not existing"
  (`src/conditions.rs:147-164`). Measured: putting a file with no execute bit into `PATH` still results in a skip.
- If `PATH` is not set, nothing matches (`src/conditions.rs:138-140`).

The "does not execute the command" rule was directly verified: a script that would write a marker file was put into `PATH`, with
`when-command-exists = "vitals-marker-cmd"`; the module was shown normally, while
`/tmp/vitals-docs-tests/EXECUTED-MARKER`, which the script should have written, **did not appear**.

**`when-file-exists`** — it simply calls `exists()` on the expanded path (`src/conditions.rs:95-99`).

- A directory also counts as existing (the test semantics of `src/conditions.rs:260-268`; measured: `/proc` passes).
- A leading `~/` expands to `$HOME` (`src/conditions.rs:172-187`). Only a leading `~/` is recognized:
  a lone `~`, or a `~` appearing in the middle, is not expanded (`src/conditions.rs:332-338`).
  When there is no `HOME` either, it is returned as-is and the home directory is not guessed (`src/conditions.rs:182-186`).
  Measured: `~/.config` passes, `~/.config/definitely-not-here-vitals` is skipped.
- Note: what `--verbose` prints is **the path exactly as written in the config**, unexpanded. The measured output is
  `skipped memory: the path ~/.config/definitely-not-here-vitals does not exist`.

### 4.2 What you see when skipped

By default nothing is printed (`src/main.rs:92-93`). Adding `--verbose` prints one line to **stderr**
(`src/main.rs:94-102`), and the messages come from `SkipReason::describe()` (`src/conditions.rs:50-59`;
the wording of the four reasons lives in `src/i18n.rs:214-244`):

| Reason | Message | Measured output |
| --- | --- | --- |
| Platform mismatch | `the current platform is <name>` | `vitals: skipped memory: the current platform is linux` |
| Command not in PATH | `` the command `<name>` is not on PATH `` | ``vitals: skipped memory: the command `vitals-does-not-exist` is not on PATH`` |
| Path does not exist | `the path <path> does not exist` | `vitals: skipped memory: the path /nonexistent/vitals does not exist` |

To distinguish "skipped" (blocked by a condition, not collected at all) from "empty" (collected but there is no data), use `--explain`
(`src/main.rs:108-114`, `194-228`).

### 4.3 Relationship to `--module`

`--module` is a **selection**, not a filter: a named module always appears, and it **drops the conditions from the config** —
explicitly naming it is the stronger intent (`src/cli.rs:124-134` for the check and the value names, `src/i18n.rs:204` for the wording, `159-171`). Measured: with
`platforms = ["windows"]` and `when-file-exists = "/nope"` attached to `memory` in the config, it is skipped when `--module` is not passed;
with `--module memory` it is shown as usual.

---

## 5. Complete example

The config below was actually run, and its output follows (all six keys — `config_version`, `type`, `platforms`,
`when-command-exists`, `when-file-exists`, `modules` — are used):

```toml
config_version = 1

# title line + one separator line
[[modules]]
type = "title"

[[modules]]
type = "separator"

[[modules]]
type = "os"

# collect only on Linux
[[modules]]
type = "kernel"
platforms = ["linux"]

[[modules]]
type = "uptime"

# skip if sh is not in PATH (only PATH is checked, it is not executed)
[[modules]]
type = "memory"
when-command-exists = "sh"

# skip if the path does not exist; a leading ~/ expands to $HOME
[[modules]]
type = "disk"
when-file-exists = "~/"

[[modules]]
type = "swap"

# finishing: an empty line + color blocks
[[modules]]
type = "break"

[[modules]]
type = "colors"
```

Run command and result (`--logo none` turns off the Logo, `--no-color` turns off colors so it can be pasted):

```console
$ vitals --config example-full.toml --logo none --no-color
gxyarch@MyArch
──────────────
OS: Arch Linux x86_64
Kernel: Linux 7.2.4-arch1-2
Uptime: 1 day, 7 hours, 15 mins
Memory: 22.92 GiB / 30.65 GiB (75%)
Disk: 53.37 GiB / 920.87 GiB (6%)
Swap: 7.88 GiB / 47.33 GiB (17%)
[exit=0]
```

(`break` and `colors` output an empty line and color blocks; they are invisible in a plain-text paste.)

Adding `--verbose` to the same config confirms on stderr that all 10 modules pass the conditions, with no skips at all:

```console
$ vitals --config example-full.toml --logo none --no-color --verbose
vitals: 10 modules: title, separator, os, kernel, uptime, memory, disk, swap, break, colors
vitals: logo=none color=off json=off verbose=on
```

---

## 6. Verification record

At least one real run per key (the repository's binary `target/release/vitals`):

| Key | Form verified | Observed result |
| --- | --- | --- |
| `config_version` | `1` / `0` / omitted | renders normally, exit code 0 |
| whole file | > 8 MiB | `over the read limit of 8388608 bytes; not loading it into memory`, exit code 1 |
| `config_version` | `2` | `config version 2 is newer than version 1, the newest this build supports; upgrade vitals`, exit code 1 |
| `config_version` | `"1"` | TOML parse failure, exit code 1 |
| `modules` | write only `[[modules]] type = "memory"` | only one Memory line is output |
| `modules` | not written / empty file / no config file | the built-in default view, exit code 0 |
| `modules` | `modules = []` | no output, exit code 0 |
| `modules` | two consecutive `type = "kernel"` | outputs two Kernel lines (duplicates allowed) |
| `type` | `"local-ip"` | shown normally |
| `type` | `"LocalIp"` / `"local_ip"` / `"WMTheme"` | parse failure, exit code 1 |
| `type` | missing | `missing field \`type\``, exit code 1 |
| `type` | `"cpuu"` | `unknown variant \`cpuu\``, exit code 1 |
| `platforms` | `["linux"]` / `[]` | the module is shown |
| `platforms` | `["windows"]` | skipped, exit code 0, `--verbose` reports the platform |
| `platforms` | `["linx"]` | parse failure, exit code 1 |
| `when-command-exists` | `"sh"` / `"/bin/sh"` | the module is shown |
| `when-command-exists` | a nonexistent command / `""` / a file with no execute bit | skipped, exit code 0 |
| `when-command-exists` | a script that would write a marker file | the module is shown, the marker file was not created (not executed) |
| `when-file-exists` | `"/proc"` / `"~/.config"` | the module is shown |
| `when-file-exists` | `"/nonexistent/vitals"` / a nonexistent `~/` path | skipped, exit code 0 |
| all three conditions combined | all satisfied / the third fails / the first fails | shown / reports the path / reports the platform |

## 7. Not covered

- The path rules for Windows / macOS: not yet implemented in the source (`src/config/path.rs:11-12`),
  so this document does not describe them, nor speculate about them.
- System-level config directories (`XDG_CONFIG_DIRS` etc.): the current load chain has only two layers
  (`src/config.rs:9` notes "stage 2 only goes as far as the user file overriding the defaults"), and there are no writable keys.
- Other config file formats (JSONC etc.): currently only TOML is supported (`src/config.rs:11-13`).
