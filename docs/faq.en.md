# Frequently Asked Questions

**English** | [中文](faq.md)

Every answer here is backed by real measurements; where code is referenced, a `file:line` is given, and where a command is referenced, the real output is pasted.

## Why are the memory numbers different from `free`?

**It is actually the same measurement**; the difference is only units and rounding.

vitals's algorithm is `MemTotal - MemAvailable` (`src/collectors/memory.rs` +
`src/collectors/meminfo.rs:58`), while the "used" of modern `free` (procps 4.x) is also
`total - available`, where "available" comes from the same `MemAvailable`. Measured at the same moment:

```console
$ free -h | head -2
               total        used        free      shared  buff/cache   available
Mem:            30Gi       5.4Gi        23Gi       152Mi       1.8Gi        25Gi

$ vitals --json --module memory --logo none | jq -r '.entries[].value'
5.41 GiB / 30.65 GiB (18%)

$ grep -E '^MemTotal|^MemAvailable' /proc/meminfo
MemTotal:       32140256 kB     # 30.65 GiB
MemAvailable:   26470216 kB     #  25.24 GiB
```

32140256 − 26470216 = 5670040 kB = 5.41 GiB — exactly what vitals prints. `free` prints `5.4Gi`
because it rounds by GiB.

## Can the numbers in `--json` be used for calculations?

No. `value` is exactly that line on the screen, with units meant for humans to read (`5.41 GiB`, `18%`, `1 day, 7 hours`),
and `uptime`, `memory`, `cpu`, `net-io`, `disk-io`, `top` are different every time. **This JSON has no raw
numeric field.**

For computable numbers, use `vitals --sources` to find out which files the module actually read, and read those files directly:

```console
$ vitals --sources --module memory
memory  /proc/meminfo
```

See [`json.md`](json.en.md) for details.

## Why is there no BIOS, motherboard, or chassis in the default view?

Because the default view is aligned to the output of fastfetch **actually** run with no arguments (the two sides were compared side by side on a real machine); it does not print these.
To have them shown, name them explicitly or write them into the config file:

```console
$ vitals --module os,bios,board,chassis
```

A comment in the template printed by `--gen-config` explains this trade-off, and why these modules were listed earlier.

## Why does `--sources` write "no files read"?

Because this module's data does not come from files. What `--sources` records is **file reads**,
and modules like `kernel` (system calls),
`wm` (environment variables), `terminal` (process chain) do not read files at all, so it writes exactly that:

```console
$ vitals --sources --module kernel,wm,terminal
kernel    no files read  (data comes from environment variables or system calls)
wm        no files read  (data comes from environment variables or system calls)
terminal  no files read  (data comes from environment variables or system calls)
```

## Why do modules with a sampling window wait 200 ms?

CPU usage, network throughput, disk throughput, and the process leaderboard are **differences**; you must take two samples with a gap in between to know the rate.
The window is defined in each of the four collectors' own `SAMPLE_WINDOW`:

- `src/collectors/net_io.rs:59`
- `src/collectors/disk_io.rs:42`
- `src/collectors/cpu_usage.rs:56`
- `src/collectors/top.rs:69`

Chained together, the four come to 821 ms, all of it spent idling. It has now been changed to parallel collection dispatched by a shared counter (thread cap 16),
so the total duration ≈ one window, measured 221 ms.

## My volume label / hostname has strange characters — why are they eaten?

Because they are filtered before output. Disk volume labels, usernames in `utmp`, EDID model names, and paths
assembled from environment variables can all carry ESC sequences, and printing them to a terminal as they are can move the cursor, change the title, or even forge other output. So
C0/C1/DEL and bidirectional text control characters (U+202A–202E, U+2066–2069) are removed, while Chinese and emoji are kept.

The implementation is in `src/render/sanitize.rs`, at the **only** place where `Info` becomes an output line, so the text layout and the
width calculation use the same text. Audit finding F1 (see [`../AUDIT.md`](../AUDIT.md)).

## Does `local-ip` secretly send packets?

No. It creates a UDP socket and `connect`s to a destination address, only to make the kernel do one route lookup, so it can ask
which source address this machine would use; UDP's `connect` sends no data. If no route is found, that is "no egress" = no data.

This is the only place in the whole project that touches a socket; the audit lists it as informational item F7 (see [`../AUDIT.md`](../AUDIT.md)).

## Is macOS / Windows supported?

There is no platform branching anywhere in the code, and all data comes from `/proc`, `/sys`, and Linux system calls, so it has **only been
verified on Linux**, and other systems are expected to be unusable. This is a deliberate trade-off: in order not to "introduce dependencies that need to link system libraries",
some modules are simply not done (see "Known gaps" in the README).

## Why is `camera` "skipped" instead of an error?

Because it is conditional: when the condition is not met it is skipped, and skipped is not an error. The four states of `--explain` are
"shown / empty / skipped / failed", and only the last one, "failed", becomes a warning on stderr:

```console
$ vitals --explain --module os,gamepad
os       shown  1 item
gamepad  empty  nothing to show on this machine
```

## Can the help be switched to English?

Yes — both the Chinese and English help are in the binary, no language pack needed:

```
$ VITALS_LANG=en vitals -h      # English for this one command only
$ export VITALS_LANG=en         # or keep using English
```

When `VITALS_LANG` is not set, `LC_ALL` / `LC_MESSAGES` / `LANG` are consulted; **when in doubt, Chinese is used**,
so "setting nothing" is still the original behavior. The priority table and the detailed rules are in [`cli.md`](cli.en.md#language).

The help **and the runtime text** both follow it: `--explain`, `--sources`, error messages, and
even the commented template `--gen-config` prints (one Chinese, one English, identical in
content). The field names (`OS:`, `Memory:`) were English to begin with, so they look the same
in both languages.

## How do I add a module?

See [`../CONTRIBUTING.md`](../CONTRIBUTING.md). The collector contract has only two methods
(`name` and `collect`), and after adding one, remember to register it in the registry — the order of `ModuleType::ALL` is an invariant, and
both the config validation and `--list-modules` depend on it.
