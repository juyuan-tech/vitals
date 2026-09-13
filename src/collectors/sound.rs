//! Sound：正在用的声音服务（PipeWire / PulseAudio / ALSA）。
//!
//! fastfetch 这一行形如 `Sound: Ryzen HD Audio Controller Speaker (70%)`——那是它通过
//! PipeWire/PulseAudio 问出来的「正在出声的那个设备 + 音量」。**零子进程拿不到那两个值**：
//! 音量在守护进程里，要拿就得实现 PipeWire 的原生 socket 协议或 PulseAudio 的 D-Bus 接口，
//! 为一个百分比不值当。所以这一行报的是**服务本身**外加版本（版本查包数据库），
//! 声卡名字放进 `variables` 给 JSON 用，默认视图不占位置。
//!
//! 认服务不跑 `pactl` / `pw-cli`，只看**会话目录里有没有它的 socket**。
//!
//! 顺序要紧：`pipewire-pulse` 会同时提供 `pulse/native`（本机就是——`pipewire-0` 与
//! `pulse/` 并存），所以 PipeWire 的标记必须排在 PulseAudio 前面，否则装了
//! pipewire-pulse 的机器会被报成 PulseAudio。
//!
//! 会话里没有声音服务（比如 ssh 进来、或者根本没装）时退回 `ALSA`：那种环境下
//! 声卡直出是唯一能出声的路，报它比什么都不报有用。既没有服务也没有声卡 → 无数据。

use std::path::Path;

use crate::collectors::{env, pkgdb, read};
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 声卡列表。`/proc/asound` 不存在时读不到，那就是没有声卡。
const CARDS: &str = "/proc/asound/cards";

/// 声音服务。
pub struct Sound;

impl Collector for Sound {
    fn name(&self) -> &'static str {
        "sound"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        let cards = match read::text(CARDS)? {
            Some(text) => card_names(&text),
            None => Vec::new(),
        };

        let Some(server) = server(&runtime_dir()) else {
            // 没有会话服务：有声卡就报 ALSA，没有就无数据。
            if cards.is_empty() {
                return Ok(Vec::new());
            }

            return Ok(vec![self.line("ALSA", cards)]);
        };

        let value = match pkgdb::version_of(server.package())? {
            Some(version) => format!("{} {version}", server.label()),
            None => server.label().to_owned(),
        };

        Ok(vec![self.line(&value, cards)])
    }
}

impl Sound {
    /// 组装这一行。声卡名字进 `variables`，不挤进值里。
    fn line(&self, value: &str, cards: Vec<String>) -> Info {
        let info = Info::new(self.name(), "Sound", value);

        if cards.is_empty() {
            info
        } else {
            info.with_variable("cards", cards.join(", "))
        }
    }
}

/// 会话目录里在跑哪个声音服务。
///
/// 参数是 `$XDG_RUNTIME_DIR` 的值，做成参数是为了能测——真正读环境变量的在 [`runtime_dir`]。
/// 目录不存在、或者里面的东西读不了，都只是「没有」，不是错误。
fn server(runtime: &str) -> Option<Server> {
    let runtime = Path::new(runtime);

    // PipeWire 先判：pipewire-pulse 也让 `pulse/native` 存在，反过来就会被误报。
    if runtime.join("pipewire-0").exists() {
        return Some(Server::PipeWire);
    }

    // `PULSE_SERVER` 指向的可能是不在会话目录里的远端服务。
    if env::var("PULSE_SERVER").is_some() || runtime.join("pulse/native").exists() {
        return Some(Server::PulseAudio);
    }

    None
}

/// `$XDG_RUNTIME_DIR`，没有就返回空串（**不去猜** `/run/user/<uid>`：拿 uid 要 libc）。
fn runtime_dir() -> String {
    env::var("XDG_RUNTIME_DIR").unwrap_or_default()
}

/// 声音服务。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Server {
    PipeWire,
    PulseAudio,
}

impl Server {
    /// 给人看的名字。
    fn label(self) -> &'static str {
        match self {
            Self::PipeWire => "PipeWire",
            Self::PulseAudio => "PulseAudio",
        }
    }

    /// 查版本用的包名。两者在主流发行版里都叫这个。
    fn package(self) -> &'static str {
        match self {
            Self::PipeWire => "pipewire",
            Self::PulseAudio => "pulseaudio",
        }
    }
}

/// `/proc/asound/cards` 里的声卡短名。
///
/// 每张卡的第一行长这样（前面有个位次、方括号里是卡的 id）：
///
/// ```text
///  0 [Generic        ]: HDA-Intel - HD-Audio Generic
///                       HD-Audio Generic at 0x605c8000 irq 85
/// ```
///
/// 只有带 `]: ` 的那行才是卡；它的第二行是长名字，靠没有 `]: ` 自然被跳过。
/// 短名取 `驱动 - 名字` 里 `" - "` 之后的部分（名字里也可能有 `" - "`，所以只切第一处）；
/// 没有 `" - "` 就用整段。**同名去重**：本机两块 HDMI 编解码器都叫 `HD-Audio Generic`，
/// 报两遍没有信息量。
fn card_names(text: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for line in text.lines() {
        let Some((_, rest)) = line.split_once("]: ") else {
            continue;
        };

        let name = match rest.split_once(" - ") {
            Some((_, name)) => name,
            None => rest,
        };
        let name = name.trim();
        if name.is_empty() || names.iter().any(|seen| seen == name) {
            continue;
        }

        names.push(name.to_owned());
    }

    names
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARDS: &str = "\
 0 [Generic        ]: HDA-Intel - HD-Audio Generic
                      HD-Audio Generic at 0x605c8000 irq 85
 1 [Generic_1      ]: HDA-Intel - HD-Audio Generic
                      HD-Audio Generic at 0x605c0000 irq 86
 2 [acp63          ]: acp63 - acp63
                      HP-HPPavilionPlusLaptop14_ey1xxx-Type1ProductConfigId-8C6B
";

    #[test]
    fn reads_card_names_and_dedupes_them() {
        assert_eq!(card_names(CARDS), ["HD-Audio Generic", "acp63"]);
    }

    #[test]
    fn a_card_without_a_driver_separator_keeps_the_whole_name() {
        assert_eq!(card_names(" 0 [foo ]: bar\n"), ["bar"]);
    }

    #[test]
    fn junk_and_empty_input_are_no_cards() {
        assert!(card_names("").is_empty());
        assert!(card_names("no cards here\n").is_empty());
        assert!(card_names(" 0 [Empty ]: \n").is_empty(), "空名字不算卡");
    }

    /// 造一个只有指定文件的临时目录，用完删掉。
    fn runtime_with(files: &[&str]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vitals-sound-test-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);

        for file in files {
            let path = dir.join(file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
        }

        dir
    }

    #[test]
    fn finds_pipewire() {
        let dir = runtime_with(&["pipewire-0"]);
        assert_eq!(server(dir.to_str().unwrap()), Some(Server::PipeWire));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn finds_pulseaudio() {
        let dir = runtime_with(&["pulse/native"]);
        assert_eq!(server(dir.to_str().unwrap()), Some(Server::PulseAudio));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_sockets_means_pipewire() {
        // 本机的真实情况：pipewire-pulse 同时提供 pulse/native。
        // 判反了就会把 PipeWire 的机器报成 PulseAudio。
        let dir = runtime_with(&["pipewire-0", "pulse/native"]);
        assert_eq!(server(dir.to_str().unwrap()), Some(Server::PipeWire));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_empty_or_missing_runtime_dir_is_no_server() {
        let dir = runtime_with(&[]);
        assert_eq!(server(dir.to_str().unwrap()), None);
        assert_eq!(server(""), None);
        assert_eq!(server("/definitely/not/here"), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn labels_and_packages_match() {
        assert_eq!(Server::PipeWire.label(), "PipeWire");
        assert_eq!(Server::PipeWire.package(), "pipewire");
        assert_eq!(Server::PulseAudio.label(), "PulseAudio");
        assert_eq!(Server::PulseAudio.package(), "pulseaudio");
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = Sound.collect(&Context::for_tests()).unwrap();

        match entries.first() {
            None => {}
            Some(info) => {
                assert_eq!(info.key, "Sound");
                assert!(!info.value.is_empty());
            }
        }
    }
}
