# Command-line reference

**English** | [中文](cli.md)

This document describes the command-line behavior of `vitals` (version 0.1.2). The command line has only options and no positional arguments; giving one extra positional argument ends with exit code 2.

Every command in this document was really executed on this machine, and the output excerpts are captured verbatim (truncated where too long, with no values rewritten).

```
Usage: vitals [OPTIONS]
```

`-h` prints a summary, and `--help` prints the full description; both start with the same tagline (`Your system's vitals, all at a glance.` in English), which — like the rest of the help — follows the language rules below. Both list the same set of options.

## Options at a glance

| Option | Effect |
| --- | --- |
| `--config <FILE>` | Specifies the config file; if omitted, it looks for `$XDG_CONFIG_HOME/vitals/config.toml` |
| `--json` | Outputs JSON (automatically turns off colors and the logo) |
| `--logo <auto\|none\|NAME>` | Logo: `auto` picks automatically by distribution, `none` does not show it, or give a name directly (default `auto`) |
| `--module <LIST>` | Only shows these modules, comma-separated. The order is the order you wrote; ones not in the config can be named too |
| `--no-color` | Turns off colors (equivalent to setting `NO_COLOR`) |
| `--list-modules` | Lists all available modules |
| `--gen-config` | Prints the built-in default config to stdout |
| `--verbose` | Writes diagnostics (including the settings that end up in effect) to stderr |
| `--explain` | Explains item by item why each module appears, or why it does not appear |
| `--sources` | **Which files each module actually read** (recorded at runtime, not a hand-written source table) |
| `-h, --help` | Prints help |
| `-V, --version` | Prints the version |

---

## `--config <FILE>`

Specifies the config file. When omitted, it looks at `$XDG_CONFIG_HOME/vitals/config.toml`.

When a path is given explicitly, failing to read the file is an **error**: it writes one line `vitals: ...` to stderr, exit code 1, and renders nothing.
Config files also have an 8 MiB read cap (the same number the collectors use): pointing `--config` at something like `/dev/zero` will not read the whole thing into memory.

```
$ vitals --config /tmp/vxdg/vitals/config.toml --logo none
OS: Arch Linux x86_64
Memory: 22.81 GiB / 30.65 GiB (74%)
```

The config file does not exist:

```
$ vitals --config /tmp/definitely-missing-vitals.toml ; echo $?
vitals: failed to read config file /tmp/definitely-missing-vitals.toml: No such file or directory (os error 2)
1
```

The config file exists but fails to parse (TOML syntax error, unknown module type, version higher than the `1` this program supports) is likewise exit code 1:

```
$ vitals --config /tmp/vbad.toml ; echo $?
vitals: failed to parse /tmp/vbad.toml
TOML parse error at line 4, column 8
  |
4 | type = "not-a-module"
  |        ^^^^^^^^^^^^^^
unknown variant `not-a-module`, expected one of `os`, `host`, ...(truncated)
1
```

```
$ vitals --config /tmp/vver.toml ; echo $?
vitals: config version 99 is newer than version 1, the newest this build supports; upgrade vitals
1
```

An empty config file (zero bytes only) is not an error, and is equivalent to the built-in default:

```
$ vitals --config /tmp/vempty.toml --logo none | head -4
gxyarch@MyArch
──────────────
OS: Arch Linux x86_64
Host: HP Pavilion Plus Laptop 14-ey1xxx
```

## `--json`

Outputs JSON; automatically turns off colors and the logo. The output is one object containing `schema_version`, `entries`, `failures`:

```
$ vitals --module memory --json
{
  "schema_version": 1,
  "entries": [
    {
      "type": "memory",
      "key": "Memory",
      "value": "22.17 GiB / 30.65 GiB (72%)"
    }
  ],
  "failures": []
}
```

When `--json` is given together with `--explain` or `--sources`, the JSON does not take effect: `--explain` and `--sources` end the process before rendering.

```
$ vitals --module memory --explain --json
memory  shown  1 item
```

## `--logo <auto|none|NAME>`

The default is `auto`: it picks the logo automatically by distribution, and uses the generic one when nothing matches. `none` does not show a logo. Giving a name uses that name directly.

A name that **does not exist will not be an error either**: the value parser for `--logo` succeeds for any string, and a non-match merely falls back to the generic logo at render time. The machine below is Arch, yet specifying `ubuntu` still draws the Ubuntu figure:

```
$ vitals --module os --logo ubuntu
                             ....            OS: Arch Linux x86_64
              .',:clooo:  .:looooo:.
           .;looooooooc  .oooooooooo'
        .;looooool:,''.  :ooooooooooc
(truncated)
```

A non-existent name likewise ends with exit code 0:

```
$ vitals --module os --logo definitelynotalogo --json >/dev/null ; echo $?
0
```

## `--module <LIST>`

Only shows the named modules, comma-separated, **in the order you wrote them**. A named module always appears, regardless of whether the config has it (the built-in default has no `bios`, yet it can be named just the same):

```
$ vitals --module gpu,os,memory --logo none
GPU: AMD Radeon 780M [HawkPoint1] (amdgpu)
OS: Arch Linux x86_64
Memory: 22.31 GiB / 30.65 GiB (73%)

$ vitals --module bios --logo none
BIOS (UEFI): Insyde F.09 (12/05/2025)
```

When comparing module names, case is ignored, and `-` and `_` are ignored; the four spellings below all point to the same module:

```
$ for n in localip Local-IP LOCAL_IP local-ip; do vitals --module "$n" --json --logo none | grep -o '"type": "[^"]*"' | head -1; done
"type": "local-ip"
"type": "local-ip"
"type": "local-ip"
"type": "local-ip"
```

An unknown module is rejected already at the argument-parsing stage, exit code 2, stderr prints all available modules; stdout is empty:

```
$ vitals --module nope ; echo $?
error: invalid value 'nope' for '--module <LIST>': unknown module `nope`; available: os, host, kernel, bios, ...(truncated)
2
```

`--module` is a **selection**, not a filter: it replaces the whole module list in the config file (it does not pick from within it), and the named modules lose the conditions attached in the config — in the config `battery` is blocked by `when-file-exists`, yet `--module battery` will still collect it:

```
$ vitals --config /tmp/vcond.toml --module battery --logo none --explain
battery  shown  1 item

$ vitals --config /tmp/vcond.toml --logo none --explain
os       shown  1 item
battery  skipped  the path /definitely/not/here does not exist
camera   shown  2 items
```

## `--no-color`

Turns off colors (equivalent to setting `NO_COLOR`). In a real terminal, keys carry ANSI sequences by default; with this option added they do not:

```
# Under a terminal (pty), without --no-color; here \x1b denotes the ESC byte
\x1b[1m\x1b[36mOS\x1b[0m: Arch Linux x86_64

# With --no-color added, or after NO_COLOR=1
OS: Arch Linux x86_64
```

When the output is a pipe there are no colors to begin with, so in that case the option makes no visible difference.

## `--list-modules`

Prints all available modules line by line to stdout in a fixed order, 61 in total:

```
$ vitals --list-modules | head -6
os
host
kernel
bios
board
chassis
```

It is an "ask and leave" option: it does not read the config file and does not collect. Even if `--config` gives a non-existent path it still succeeds:

```
$ vitals --list-modules --config /tmp/nope-vitals.toml >/dev/null ; echo $?
0
```

(But the arguments themselves must still pass parsing first: `--list-modules --module nope` is still exit code 2.)

## `--gen-config`

Prints the built-in default config to stdout (**the template follows the language**: 2422 bytes for Chinese, 2655 for English, both ending with a newline), does not read the config file and does not collect; given together with other options it still succeeds as usual:

```
$ vitals --gen-config | head -6
# Vitals config
#
# Printed by `vitals --gen-config`. This file **replaces the whole list** of built-in
# defaults: once modules is written, the built-in list no longer takes effect, so to
# show fewer modules you have to list all the ones you want to keep.
#
```

```
$ vitals --gen-config --config /tmp/nope-vitals.toml >/dev/null ; echo $?
0
```

## `--verbose`

Writes diagnostic information to **stderr**, each line carrying the `vitals: ` prefix. It contains at least the settings that end up in effect, and the modules skipped by a condition:

```
$ vitals --module os --verbose --logo none 2>&1 >/dev/null
vitals: 1 module: os
vitals: logo=none color=on json=off verbose=on
```

When there is a conditional skip it appends:

```
vitals: skipped battery: the path /definitely/not/here does not exist
```

When a module fails to collect it prints a failure line; only `--verbose` goes on to lay out the underlying cause chain:

```
vitals: the datetime module failed: failed to open /tmp/nozone/tz
vitals:   because: Permission denied (os error 13)
```

`--verbose` does not affect the exit code, nor does it change the contents of stdout.

## `--explain`

Explains item by item **why each module appears, or why it does not appear**. It speaks to the result, and each of the four states has its own reason; telling "empty" apart from "shown" requires really running a collection once, so it takes the time of one collection.

The report is written to stdout, exit code 0 — even if an item is a "failure". What the four states really look like:

```
$ vitals --module memory --explain
memory  shown  1 item

$ vitals --config /tmp/vcond.toml --explain
os       shown  1 item
battery  skipped  the path /definitely/not/here does not exist
camera   shown  2 items

$ vitals --module gamepad,battery --explain
gamepad  empty  nothing to show on this machine
battery  shown  1 item

$ TZDIR=/tmp/nozone TZ=tz vitals --module datetime --explain
vitals: the datetime module failed: failed to open /tmp/nozone/tz    # stderr
datetime  failed  failed to open /tmp/nozone/tz                      # stdout
```

"Empty" and "skipped" are two different things: the former was collected, and this machine really has no data; the latter is a condition not being met, so it was never collected at all.

When `--explain` is given together with `--json`, only this report is printed, and no JSON is output.

## `--sources`

**Which files each module actually read** (recorded at runtime, not a hand-written source table). The report is written to stdout, exit code 0. Skipped modules show the reason for skipping; a module that read nothing will say so outright, rather than making up a source:

```
$ vitals --module memory,gpu --sources
memory  /proc/meminfo
gpu     /sys/class/drm/card1/device/vendor, /sys/class/drm/card1/device/device, /sys/class/drm/card1/device/uevent, ...(truncated)
```

```
$ vitals --config /tmp/vcond.toml --sources
os       /etc/os-release
battery  skipped  the path /definitely/not/here does not exist
camera   /sys/class/video4linux/video0/name, /sys/class/video4linux/video1/name, /sys/class/video4linux/video2/name, /sys/class/video4linux/video3/name
```

A module that read nothing:

```
$ vitals --module wm --sources
wm  no files read  (data comes from environment variables or system calls)
```

> Note: in 0.1.0 the `--explain` branch returned directly, and `--sources` was not executed — this
> did not match the original help text "give both and it prints the state first, then the basis". **Fixed as of 0.1.1**; when both are given, both reports are printed:
>
> ```console
> $ vitals --module memory --explain --sources
> memory  shown  1 item
> memory  /proc/meminfo
> ```

When only `--sources` is given it works normally.

## `-h, --help` and `-V, --version`

Both end with exit code 0, and their contents are written to stdout.

```
$ vitals --version
vitals 0.1.2
```

`-h` is the short help (one line per option), `--help` is the long help (with multiple paragraphs of explanation); both languages keep this distinction.

## Language

The help text has two sets, Chinese and English, both shipped with the binary, and no language pack is needed. Field names (`OS:`, `Memory:`)
are English in both languages; **runtime text** has two sets as well — the four states of `--explain`, the "no files read" of `--sources`, the
settings line of `--verbose`, error messages, and the commented template printed by `--gen-config` (Chinese template `config/default.toml`,
English template `config/default.en.toml`, with the same structure) — all selected by the rules below.

Which set is chosen follows this order, first hit wins:

| Order | Source | Description |
| --- | --- | --- |
| 1 | `VITALS_LANG` | Only `zh*` and `en*` are recognized; writing anything else (including a typo) is **treated as not written**, and it continues down the list |
| 2 | `LC_ALL` | |
| 3 | `LC_MESSAGES` | |
| 4 | `LANG` | |

The locale takes only the language body: `zh_CN.UTF-8` → `zh`, `en-US` → `en`. **When in doubt, Chinese** —
an unset variable, a value of `C` / `POSIX`, or something that is not a language tag at all all count as in doubt, so "setting nothing at all"
is exactly as before, and will not change face just because you switched machines. Languages other than Chinese (for example `de_DE.UTF-8`)
have no translation, and fall back to English.

```
$ VITALS_LANG=en vitals --module nope
error: invalid value 'nope' for '--module <LIST>': unknown module `nope`; available: os, host, kernel, …(remaining module names omitted)

$ LANG=en_US.UTF-8 vitals -h | head -1
Your system's vitals, all at a glance.
```

## Output stream conventions

| Stream | Contents |
| --- | --- |
| stdout | The default rendering result (text or the JSON of `--json`); the module names of `--list-modules`; the TOML of `--gen-config`; the `--explain` report; the `--sources` report; `--help` / `--version` |
| stderr | All `vitals: `-prefixed diagnostics: config read/parse/version errors, module collection failures, the final settings and skip reasons of `--verbose`, and render write-out errors. Plus clap's argument errors (`error: ...`) |

Diagnostics going to stderr is deliberate: with `vitals > file` the result file does not get polluted.

Exit codes (three kinds, all actually tested):

| Exit code | When it occurs |
| --- | --- |
| `0` | Normal termination: successful render, `--help`, `--version`, `--list-modules`, `--gen-config`, `--explain`, `--sources`. **A module that fails to collect but renders successfully is still 0** (the failure is only written to stderr). A downstream that closes the pipe early (e.g. `vitals \| head -1`) also returns 0. |
| `1` | Runtime failure: config file read/parse/version errors; failing to write out output (e.g. `vitals > /dev/full`, with stderr `vitals: failed to write output`). |
| `2` | Argument error: unknown option, unknown module, one extra positional argument. Produced by clap. |

```
$ vitals --module os --logo none >/dev/full ; echo $?
vitals: failed to write output
1

$ vitals 2>/dev/null | head -1 >/dev/null ; echo ${PIPESTATUS[0]}   # vitals's own exit code
0

$ vitals --nope ; echo $?
error: unexpected argument '--nope' found
2
```

## Environment variables

For the variables below, the read locations have all been confirmed in the `src/` code (`file:line`); among them `NO_COLOR` and `CLICOLOR_FORCE` are read by the anstream dependency, are not called directly by this repository, and their behavior has been confirmed by actual testing.

### Affecting the program itself (config, colors, layout)

| Variable | Effect | Read location |
| --- | --- | --- |
| `XDG_CONFIG_HOME` | The default config directory; it must be an absolute path, a relative path is ignored and falls back to `$HOME/.config` | `src/config/path.rs:27`; see also `src/collectors/ini.rs:357`, `src/collectors/terminal_font.rs:126` |
| `HOME` | The fallback when `XDG_CONFIG_HOME` is unset (or is not an absolute path); expansion of `~/` in `when-file-exists`; a candidate path for several modules | `src/config/path.rs:28`, `src/conditions.rs:182`, `src/collectors/ini.rs:358`, `src/collectors/ini.rs:491`, `src/collectors/terminal_font.rs:130`, `src/collectors/pkgdb.rs:240`, `src/collectors/pkgdb.rs:259`, `src/collectors/rust.rs:37` |
| `PATH` | The lookup scope of the `when-command-exists` condition (it only searches PATH, it does not execute the command) | `src/conditions.rs:118` |
| `NO_COLOR` | Setting it to any value disables colors (equivalent to `--no-color`) | read by anstream; `src/main.rs:145-152` constructs the output stream, see `src/render/theme.rs:4` for the explanation |
| `CLICOLOR_FORCE` | When non-empty, it forces colors to be kept even if the output is a pipe | same as above; tested `CLICOLOR_FORCE=1 vitals --module os --logo none \| cat -v` prints `^[[1m^[[36mOS^[[0m: ...` |
| `COLUMNS` | When the tty size cannot be obtained, it is used as the terminal column count; when the column count is unknown the logo is not hidden (`COLUMNS=40 vitals` will take the logo away) | `src/render/text.rs:320` |

### Affecting module collection

| Variable | Effect | Read location |
| --- | --- | --- |
| `LC_ALL` / `LC_CTYPE` / `LANG` | The locale setting of the `locale` module, taking the first non-empty value in this priority order; when all are empty it falls back to reading `LANG=` from `/etc/locale.conf` | `src/collectors/locale.rs:12` (declaration), `src/collectors/locale.rs:27` (read); `src/collectors/locale.rs:14,16` |
| `TZ` | The time zone of the `datetime` module (it points to a zoneinfo file under `$TZDIR`; if unset, `/etc/localtime` is read); the `users` module also uses it | `src/collectors/date_time.rs:123`, `src/collectors/users.rs:276` |
| `TZDIR` | The lookup root directory for `$TZ`, default `/usr/share/zoneinfo` | `src/collectors/date_time.rs:130` |
| `SHELL` | The `shell` module prefers it; if unset it falls back to the login shell from `/etc/passwd` | `src/collectors/shell.rs:11`, `src/collectors/shell.rs:47` |
| `USER` / `LOGNAME` | The username fallback for the `title` and `user` modules (it checks `/etc/passwd` first) | `src/collectors/accounts.rs:102`, `src/collectors/accounts.rs:113`, `src/collectors/user.rs:23` |
| `VISUAL` / `EDITOR` | The `editor` module | `src/collectors/editor.rs:14`, `src/collectors/editor.rs:25` |
| `XCURSOR_THEME` / `XCURSOR_SIZE` | The theme and size of the current session for the `cursor` module | `src/collectors/cursor.rs:42`, `src/collectors/cursor.rs:52`; `src/collectors/cursor.rs:44`, `src/collectors/cursor.rs:69` |
| `XDG_SESSION_TYPE` | The `wm` module determines the session type | `src/collectors/wm.rs:35` |
| `XDG_CURRENT_DESKTOP` / `XDG_SESSION_DESKTOP` / `DESKTOP_SESSION` | The `de` module recognizes the desktop environment | `src/collectors/session.rs:52-56`, `src/collectors/session.rs:88` |
| `TERM` | The `terminal` module and the `terminal-font` module recognize the terminal | `src/collectors/terminal.rs:58`, `src/collectors/terminal_font.rs:81` |
| `TERM_PROGRAM` / `TERM_PROGRAM_VERSION` | The terminal name and version of the `terminal` module; `terminal-font` also uses it to recognize ghostty / Alacritty | `src/collectors/terminal.rs:84-85`, `src/collectors/terminal_font.rs:94,105` |
| `KITTY_WINDOW_ID` / `KITTY_PID` | Recognizes kitty | `src/collectors/terminal.rs:20`, `src/collectors/terminal_font.rs:83-84` |
| `WEZTERM_EXECUTABLE` | Recognizes WezTerm | `src/collectors/terminal.rs:21` |
| `ALACRITTY_SOCKET` / `ALACRITTY_LOG` | Recognizes Alacritty | `src/collectors/terminal.rs:22`, `src/collectors/terminal_font.rs:103-104` |
| `WT_SESSION` | Recognizes Windows Terminal | `src/collectors/terminal.rs:23` |
| `VTE_VERSION` | Recognizes VTE | `src/collectors/terminal.rs:24` |
| `GHOSTTY_RESOURCES_DIR` | Recognizes ghostty | `src/collectors/terminal_font.rs:93` |
| `XDG_CONFIG_HOME` / `HOME` | `terminal-font` looks for the kitty config; `theme` / `icons` / `font` / `wmtheme` look for GTK and KDE configs | `src/collectors/terminal_font.rs:126,130`, `src/collectors/ini.rs:357,358` |
| `XDG_CONFIG_DIRS` / `XDG_DATA_DIRS` | System-level candidate directories for GTK / icons / cursor themes | `src/collectors/ini.rs:390`, `src/collectors/ini.rs:399` |
| `XDG_RUNTIME_DIR` / `PULSE_SERVER` | The `sound` module determines whether audio is present | `src/collectors/sound.rs:94`, `src/collectors/sound.rs:85` |
| `RUSTUP_TOOLCHAIN` / `RUSTUP_HOME` / `HOME` | The `rust` module (it reads rustup's `settings.toml`) | `src/collectors/rust.rs:32`, `src/collectors/rust.rs:36`, `src/collectors/rust.rs:37` |

Three tested examples:

```
$ XCURSOR_THEME=FooBar XCURSOR_SIZE=48 vitals --module cursor --json --logo none
      "value": "FooBar (48px)"

$ TERM_PROGRAM=ghostty TERM_PROGRAM_VERSION=1.2 vitals --module terminal --json --logo none
      "value": "ghostty 1.2"

$ env -u LC_ALL -u LC_CTYPE LANG=fr_FR.UTF-8 vitals --module locale --json --logo none
      "value": "fr_FR.UTF-8"
```

No other variables that would affect behavior were found. `PROGRAM`, `TAGLINE` and the like are compile-time constants, not environment variables.

## Common combinations

**1. Look at just a few modules** (the order is the order you wrote)

```
$ vitals --module os,kernel,memory --logo none
OS: Arch Linux x86_64
Kernel: Linux 7.2.4-arch1-2
Memory: 22.17 GiB / 30.65 GiB (72%)
```

**2. JSON output for scripts**

```
$ vitals --module memory --json
{
  "schema_version": 1,
  "entries": [
    {
      "type": "memory",
      "key": "Memory",
      "value": "22.17 GiB / 30.65 GiB (72%)"
    }
  ],
  "failures": []
}
```

**3. Generate a config and then modify it**

```
$ vitals --gen-config | head -6
# Vitals config
#
# Printed by `vitals --gen-config`. This file **replaces the whole list** of built-in
# defaults: once modules is written, the built-in list no longer takes effect, so to
# show fewer modules you have to list all the ones you want to keep.
#
```

**4. Check the data sources of a module**

```
$ vitals --module memory,gpu --sources
memory  /proc/meminfo
gpu     /sys/class/drm/card1/device/vendor, /sys/class/drm/card1/device/device, /sys/class/drm/card1/device/uevent, ...(truncated)
```

**5. Check why a module has not a single line**

```
$ vitals --module gamepad,battery --explain
gamepad  empty  nothing to show on this machine
battery  shown  1 item
```

## Relationship to the config file

**With no arguments it renders according to the config file**; using `--module` keeps only a few of them.

The stacking order is built-in default → user config file → command-line arguments. The command line only makes selections on top of the result of the first two layers (`--module` replaces the whole module list, `--json` / `--no-color` tighten the output, `--logo` overrides the logo).

How the config file is located:

- When `--config` is not given it looks for `$XDG_CONFIG_HOME/vitals/config.toml`; when `XDG_CONFIG_HOME` is unset or is not an absolute path it falls back to `$HOME/.config/vitals/config.toml`; when even `HOME` is absent it uses the built-in default directly, without error.
- A file at this path that **does not exist is not an error**, and the built-in default is used quietly — someone running it for the first time should not see an error.
- Only a path given explicitly by `--config` failing to be read is an error (exit code 1).

As for the `modules` list, the file printed by `--gen-config` says it very clearly:

> Printed by `vitals --gen-config`. This file **replaces the whole list** of built-in defaults: once modules is written, the built-in list no longer takes effect, so to show fewer modules you have to list all the ones you want to keep.

That is to say:

- **Not writing** `[[modules]]` in the config file: the built-in default list is used (24 modules, in order `title`, `separator`, `os`, `host`, `kernel`, `uptime`, `packages`, `shell`, `display`, `de`, `wm`, `cursor`, `terminal`, `terminal-font`, `cpu`, `gpu`, `memory`, `swap`, `disk`, `local-ip`, `battery`, `locale`, `break`, `colors`).
- **Writing** `[[modules]]` in the config file: the built-in list no longer takes effect, and only the ones you list are shown — to show fewer modules you have to list all the ones you want to keep.
- Every module can carry conditions (`platforms`, `when-command-exists`, `when-file-exists`); when a condition is not met it is skipped, and skipping is not an error. But modules named by `--module` lose these conditions.
- `config_version` must not be higher than the `1` this program supports, otherwise it errors and exits 1.
