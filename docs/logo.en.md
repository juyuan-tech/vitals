# Logo and Colors

**English** | [中文](logo.md)

## Choosing a Logo

The values of `--logo` (verbatim from `vitals --help`): `auto` picks automatically by distribution, `none` does not show it, or give a name directly;
the default is `auto`. Names are **case-insensitive** (`src/render/logo.rs:71`).

12 built in:

```
alpine  arch  centos  debian  fedora  gentoo  linux  manjaro  nixos  opensuse  ubuntu  void
```

The selection order is `os-release`'s `ID` → `ID_LIKE` → the generic art (`src/render/logo.rs:79`).
The `ID_LIKE` step cannot be skipped: derivatives such as `ID=cachyos`, `ID=endeavouros` usually do not have their own art,
but their `ID_LIKE=arch`, so they can land on Arch.

Giving a **name that does not exist does not error**; it falls back to the generic art: measured, `--logo definitely-not-a-logo` and
`--logo auto` produce different output — on this machine `auto` picks Arch, while an unknown name picks the generic art.

```console
$ vitals --logo arch                  # specify a distribution
$ vitals --logo none                  # no ASCII art
$ vitals --logo none --module os      # only want to see certain items
```

## Colors

- Colors follow the terminal. `--no-color` is equivalent to setting the `NO_COLOR` environment variable (verbatim from `--help`); both can turn colors off.
- `--json` automatically turns off colors and the Logo (`Settings::resolve` handles this when `--json` is given).

## The `colors` Module

`colors` is a layout primitive, used to confirm the terminal's colors:

```console
$ vitals --module colors
```

It does **not appear** in `--json`: `colors`'s key and value are both empty strings, and the JSON renderer filters out all
entries where "key and value are both empty" (the same category as `separator` and `break`). See [`json.md`](json.en.md).
