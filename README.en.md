# Vitals

[中文](README.md) | **English**

[![CI](https://github.com/juyuan-tech/vitals/actions/workflows/ci.yml/badge.svg)](https://github.com/juyuan-tech/vitals/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/vitals-rs.svg)](https://crates.io/crates/vitals-rs)
[![docs.rs](https://docs.rs/vitals-rs/badge.svg)](https://docs.rs/vitals-rs)

> Your system's vital signs, at a glance.

`vitals` is a system information tool (the fastfetch / neofetch class). It holds one extra line
by design: **every line it prints should be answerable with "where did this come from?"** — so
it shells out to nothing, reads `/proc`, `/sys`, `/etc` and system calls itself, and can report
which files it actually read.

- 61 modules (fastfetch 2.68.1 has 76, of which we cover 59)
- 4-6 ms for the default view; 221 ms with the four sampling modules (Ryzen 7 8845H)
- Zero subprocesses, zero `unsafe`, writes no files, sends no network packets
- MSRV 1.85 (edition 2024)
- Security audit: [`AUDIT.md`](AUDIT.md) (Chinese)

> **Documentation language.** Every per-topic reference exists in Chinese (`docs/<name>.md`) and
> in English (`docs/<name>.en.md`); the two link to each other at the top. The built-in help is
> bilingual too (`VITALS_LANG=zh|en`), and so is all runtime text — `--explain`, `--sources`,
> `--verbose`, errors and the `--gen-config` template. Without `VITALS_LANG`, `LC_ALL` /
> `LC_MESSAGES` / `LANG` decide, and anything uncertain means Chinese; the program output
> quoted in these English references is the output of an English run (`VITALS_LANG=en`).

## Install

```console
$ cargo install vitals-rs     # the installed command is `vitals`
```

From source:

```console
$ git clone https://github.com/juyuan-tech/vitals
$ cargo install --path vitals
```

Requirements: **Linux** (the data comes from `/proc` and `/sys`; the code contains no platform
branches, so other systems are unverified), Rust 1.85 or newer.

## Quick start

```console
$ vitals                      # default view
$ vitals --module os,cpu,memory
$ vitals --list-modules       # all 61 module names
$ vitals --gen-config > ~/.config/vitals/config.toml

$ vitals --json               # structured output for scripts
$ vitals --explain            # why each module appeared — or did not
$ vitals --sources            # the files each module actually read
```

The default view looks like this (**Example**: a real default view from a Linux laptop, including the release logo auto-selected by `--logo auto`. Only the user name, machine model, panel id, network interface and battery id are placeholders; everything else is verbatim. `--logo none` drops the art on the left.):

```console
                                       user@host
                                       ──────────────
                  -`                   OS: Arch Linux x86_64
                 .o+`                  Host: Example Laptop 14
                `ooo/                  Kernel: Linux 7.2.4-arch1-2
               `+oooo:                 Uptime: 1 day, 7 hours, 27 mins
              `+oooooo:                Packages: 3 (appimage), 3 (flatpak), 1042 (pacman)
              -+oooooo+:               Shell: zsh 5.9.2
            `/:-:++oooo+:              Display (eDP-1): 2880x1800 in 14", 120 Hz [Built-in]
           `/++++/+++++++:             Window Manager: Niri 26.04 (Wayland)
          `/++++++++++++++:            Cursor: breeze (30px)
         `/+++ooooooooooooo/`          Terminal: kitty 0.48.2
        ./ooosssso++osssssso+`         Terminal Font: JetBrainsMono Nerd Font 12pt
       .oossssso-````/ossssss+`        CPU: AMD Ryzen 7 8845H w/ Radeon(TM) 780M Graphics (8C/16T)
      -osssssso.      :ssssssso.       GPU: AMD Radeon 780M [HawkPoint1] (amdgpu)
     :osssssss/        osssso+++.      Memory: 22.55 GiB / 30.65 GiB (74%)
    /ossssssss/        +ssssooo/-      Swap: 7.88 GiB / 47.33 GiB (17%)
  `/ossssso+/:-        -:/+osssso+-    Disk: 53.63 GiB / 920.87 GiB (6%)
 `+sso+:-`                 `.-/+oso:   Local IP (wlan0): 192.168.1.10/24
`++:.                           `-/+/  Battery (BAT0): 100% [AC Connected]
.`                                 `/  Locale: zh_CN.UTF-8
```

The repository also ships a man page (`doc/vitals.1`) and four example configs (`presets/`):

```console
$ vitals --config presets/headless.toml   # headless: no sampling, no desktop/hardware modules
$ vitals --config presets/all.toml        # every one of the 61 modules
```

## Every number can be traced

These three flags are the reason this project exists:

```console
$ vitals --sources --module os,host,wm
os    /etc/os-release
host  /sys/devices/virtual/dmi/id/sys_vendor, /sys/devices/virtual/dmi/id/product_name, /sys/devices/virtual/dmi/id/product_version, /sys/devices/virtual/dmi/id/board_name
wm    no files read  (data comes from environment variables or system calls)
```

- `--sources` reports what was **observed at runtime**, not a hand-written table of origins.
- `--explain` reports **state**: shown / empty / skipped / failed, each with a reason.
- `--verbose` reports the **effective settings** (to stderr).

```console
$ vitals --explain --module os,gamepad
os       shown  1 item
gamepad  empty  nothing to show on this machine
```


The other two states are easy to produce yourself (both runs below were done on this machine):

```console
$ printf 'config_version = 1\n\n[[modules]]\ntype = "camera"\nwhen-file-exists = "/definitely/not/here"\n' > /tmp/cond.toml
$ vitals --config /tmp/cond.toml --explain --logo none
camera  skipped  the path /definitely/not/here does not exist

$ TZ=/etc/shadow vitals --explain --module datetime --logo none
vitals: the datetime module failed: failed to open /etc/shadow
datetime  failed  failed to open /etc/shadow
```

Note that **a module that fails still leaves the exit code at 0**: the failure also prints one
warning line to stderr, but one module's problem does not hide the other modules' results.

## Modules

61 of them; the full per-module reference (including the files each one reads) is in
**[`docs/modules.md`](docs/modules.en.md)** (Chinese).

**System** (13)　`os` `host` `kernel` `bios` `board` `chassis` `uptime` `datetime` `version`
`init-system` `bootmgr` `processes` `loadavg`

**Hardware** (21)　`cpu` `cpu-cache` `gpu` `memory` `swap` `disk` `physical-disk` `btrfs`
`battery` `power-adapter` `brightness` `lm` `tpm` `sound` `wifi` `camera` `keyboard`
`mouse` `gamepad` `monitor` `display`

**Session and desktop** (16)　`user` `users` `shell` `terminal` `terminal-size` `terminal-font`
`locale` `editor` `de` `wm` `wmtheme` `theme` `icons` `font` `cursor` `colors`

**Network and throughput** (6)　`local-ip` `dns` `net-io` `disk-io` `cpu-usage` `top`

**Software** (2)　`packages` `rust`

**Layout** (3)　`title` `separator` `break`

## Configuration

`~/.config/vitals/config.toml` (honours `XDG_CONFIG_HOME`), or point `--config` elsewhere.
`--gen-config` prints a commented template, in Chinese or English to match the language
(`config/default.toml` / `config/default.en.toml`; the two are structurally identical). The file
**replaces the built-in default wholesale**: once you list `modules`, the built-in list no longer
applies. Per-key reference:
**[`docs/configuration.md`](docs/configuration.en.md)** (Chinese).

Each module may declare conditions; an unmet condition skips it (skipping is not an error):

```toml
config_version = 1

[[modules]]
type = "os"
when-file-exists = "/etc/os-release"

[[modules]]
type = "camera"
when-file-exists = "/dev/video0"
```

`when-command-exists` is the only field that looks like running a command: it merely looks for a
file of that name along `PATH`. It never `fork`s or `exec`s.

## JSON

`vitals --json` prints `{"schema_version": 1, "entries": [...], "failures": [...]}`; the field
names are a contract. Shape, compatibility policy and `jq` examples:
**[`docs/json.md`](docs/json.en.md)**; a machine-readable schema:
**[`docs/vitals.schema.json`](docs/vitals.schema.json)**.

## Performance

Measured on the same machine (Ryzen 7 8845H):

| Scenario | vitals | fastfetch 2.68.1 |
| --- | --- | --- |
| Default view | 4-6 ms | 20-37 ms |
| `net-io,disk-io,cpu-usage,top` | 221 ms | —— |

Each of the four sampling modules waits out a 200 ms window. Run in sequence that is 821 ms of
mostly idle waiting; they now collect in parallel (a shared work counter, at most 16 threads),
so the total is about one window.

## Security and privacy

- **No subprocesses.** Nothing is `fork`ed or `exec`ed; `when-command-exists` only looks at `PATH`.
- **No `unsafe`.** `#![forbid(unsafe_code)]`; the system calls (`uname`, `statvfs`,
  `tcgetwinsize`) go through `rustix`'s safe wrappers.
- **Writes no files.** No `File::create` / `fs::write` / `remove_*` outside tests.
- **Sends no network packets.** The only socket is in `local-ip`: a UDP socket `connect`ed to a
  destination purely so the kernel performs a route lookup. UDP `connect` transmits nothing; no
  route means "no data".
- **Terminal control characters are stripped before printing** (C0/C1/DEL and the bidi controls;
  CJK and emoji survive) — volume labels, `utmp` user names, EDID model strings and
  environment-derived paths can all carry ESC.
- **Single-file reads are capped at 8 MiB** and error out rather than truncate (`$TZ`/`$TZDIR`
  take part in path building).
- **Time zone names are path-checked**: a relative name containing `..` is refused, so an
  environment variable cannot walk out of the zoneinfo directory.

The full audit (8 findings, each with evidence and a fix) is [`AUDIT.md`](AUDIT.md) (Chinese).
How to report a security issue: [`SECURITY.md`](SECURITY.md).

## Documentation

Every per-topic reference exists in both languages: the files under `docs/` are the Chinese
originals, and each has an English twin (`docs/<name>.en.md`). The two files link to each
other at the top. Quoted program output is verbatim in whatever language the program
printed it; the output quoted in these English references comes from English runs
(`VITALS_LANG=en`).

| File | Contents |
| --- | --- |
| [`docs/modules.en.md`](docs/modules.en.md) | all 61 modules: what each shows and which files it reads |
| [`docs/configuration.en.md`](docs/configuration.en.md) | per-key config reference, conditions, examples |
| [`docs/cli.en.md`](docs/cli.en.md) | every option, exit codes, environment variables |
| [`docs/json.en.md`](docs/json.en.md) | the `--json` shape and how to consume it |
| [`docs/vitals.schema.json`](docs/vitals.schema.json) | JSON Schema (draft 2020-12) |
| [`docs/faq.en.md`](docs/faq.en.md) | FAQ, every answer measured |
| [`docs/logo.en.md`](docs/logo.en.md) | logos and colours |
| [`doc/vitals.en.1`](doc/vitals.en.1) | man page, English (`man ./doc/vitals.en.1`; the Chinese one is `doc/vitals.1`) |
| [`presets/`](presets) | example configs: minimal / desktop / headless / all |
| [`completions/`](completions) | bash, zsh and fish completions (all three really loaded and verified) |
| [`AUDIT.md`](AUDIT.md) | security and quality audit (Chinese) |
| [`CHANGELOG.md`](CHANGELOG.md) | release history |

## Known gaps

Modules fastfetch 2.68.1 has that we do not: `Bluetooth`, `OpenGL`, `Vulkan`, `OpenCL`, `Codec`,
`PublicIp`, `Weather`, `Player`, `Media`, `Command`, `Wallpaper`, `TerminalTheme`, `Zpool` and
friends — most need a linked system library (at odds with "no `unsafe`, no extra dependencies"),
a network request, or an external command. `Custom` is not implemented either: it would place
arbitrary command output into the layout, which collides with the no-subprocess stance.

Other known gaps: the completion descriptions are Chinese (the help and the runtime text
themselves follow `VITALS_LANG`; this little completion surface was not translated, and the
bash one has no descriptions at all) — all three completions were really loaded and run.

## Development

```console
$ cargo test
$ cargo clippy --all-targets -- -D warnings
$ cargo fmt --all -- --check
```

All three must be green (CI also has a job that builds with 1.85). See
[`CONTRIBUTING.md`](CONTRIBUTING.md) (Chinese).

## License

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
