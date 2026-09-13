//! LocalIp：本机地址——**默认路由用的那一个**。
//!
//! 零依赖、零子进程问出本机地址的办法是「UDP connect 探测路由」：
//! `UdpSocket::bind` 之后 `connect("1.1.1.1:53")`，内核照着路由表选一个源地址，
//! 再问 `local_addr()` 就知道了。**`connect` 一个字节都不发**——UDP 无连接，
//! `connect` 只是把默认对端记在 socket 上，好让以后的 `send` 不必带地址；
//! 这里连 `send` 都没有，所以这段代码不是网络请求。断网时它不会超时等待，
//! 只会立刻返回 `ENETUNREACH`，那是「没有出口路由」，属于**无数据**。
//!
//! 为什么不用别的办法：`getifaddrs` 要 libc（本 crate 的依赖只有 rustix，
//! 且没开 net feature）；`/proc/net/fib_trie` 是给人看的树，解析它比问内核
//! 一个问题脆弱得多；`/sys/class/net/*/address` 里只有 MAC，地址本身不在 sysfs。
//!
//! 诚实边界：报的是**默认路由那个地址**，不是「所有网卡的所有地址」。
//! 同时插着网线和 docker0 的机器上，upstream fastfetch 会把它们全列出来，
//! 这里只列出口的那一个。没有默认路由（断网、只有回环、纯离线）就是无数据，
//! **不猜**一个私网地址出来。
//!
//! 上游口径（fastfetch 2.68.1 的 `-s localip`）是
//! `Local IP (enp5s0f4u1u3c2): 192.168.1.101/24`：网卡名进键、值是 CIDR。
//! 它的默认配置只列 IPv4（本机有全局 IPv6 也没列），所以这里同样以 IPv4 为主，
//! 只有拿不到 IPv4 时才退到 IPv6——这是**唯一**会报 IPv6 的情况。

use std::net::{IpAddr, Ipv4Addr};

use crate::collectors::read;
use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// IPv4 路由表。默认路由（谁出口）与连接路由（网段多宽）都在这里。
const ROUTE4: &str = "/proc/net/route";
/// IPv6 地址表：地址、前缀长度、网卡名一应俱全，不必再探一次路由。
const IF_INET6: &str = "/proc/net/if_inet6";
/// 网卡属性目录：MAC 地址在这里。
const NET_CLASS: &str = "/sys/class/net";

/// IPv4 探测目标。只是让内核查一次路由，**不发包**。
const PROBE4: &str = "1.1.1.1:53";
/// IPv6 探测目标（Cloudflare DNS）。同样不发包。
const PROBE6: &str = "[2606:4700:4700::1111]:53";

/// 本机地址。
pub struct LocalIp;

impl Collector for LocalIp {
    fn name(&self) -> &'static str {
        "local-ip"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        if let Some(address) = probe("0.0.0.0:0", PROBE4)? {
            let IpAddr::V4(address) = address else {
                // `0.0.0.0` 的 socket 不可能探出一个 IPv6 地址。
                return Ok(Vec::new());
            };

            // 路由表读不到（非 Linux、容器里没挂 /proc）就只报地址，
            // 不写网卡名也不写前缀——少两个信息，总比编一个好。
            let route = read::text(ROUTE4)?;
            let default_route = route.as_deref().and_then(default_route);

            let interface = default_route.as_ref().map(|route| route.interface.clone());
            let prefix = route
                .as_deref()
                .zip(interface.as_deref())
                .and_then(|(text, interface)| connected_prefix(text, interface, address));

            return Ok(vec![describe(
                self.name(),
                address.to_string(),
                prefix,
                interface,
                "ipv4",
            )?]);
        }

        // IPv6 只在 IPv4 探不到时出场。地址表里同一地址那行带前缀长度与网卡名。
        if let Some(address @ IpAddr::V6(_)) = probe("[::]:0", PROBE6)? {
            let table = read::text(IF_INET6)?;
            let entry = table.as_deref().and_then(|text| inet6_entry(text, address));

            let (prefix, interface) = match entry {
                Some(entry) => (Some(entry.prefix), Some(entry.interface)),
                None => (None, None),
            };

            return Ok(vec![describe(
                self.name(),
                address.to_string(),
                prefix,
                interface,
                "ipv6",
            )?]);
        }

        Ok(Vec::new())
    }
}

/// 组装一条 `Info`。
///
/// 键带上网卡名（`Local IP (enp5s0f4u1u3c2)`），因为这正是 upstream 的写法，
/// 而且一台机器只报一条时，网卡名是唯一能说明「这条是哪个出口」的东西；
/// 网卡名拿不到就不写括号。值带上前缀长度（`192.168.1.101/24`），
/// 拿不到就只写地址——`/0` 这种假前缀比不写更坏。
fn describe(
    module: &'static str,
    address: String,
    prefix: Option<u8>,
    interface: Option<String>,
    family: &'static str,
) -> Result<Info, CollectError> {
    let key = match &interface {
        Some(interface) => format!("Local IP ({interface})"),
        None => "Local IP".to_owned(),
    };
    let value = match prefix {
        Some(prefix) => format!("{address}/{prefix}"),
        None => address,
    };

    let mut info = Info::new(module, key, value).with_variable("family", family);
    if let Some(interface) = &interface {
        info = info.with_variable("interface", interface.clone());
        // MAC 只是附带信息：拿不到不影响这条地址本身。
        if let Some(mac) = read::text(&format!("{NET_CLASS}/{interface}/address"))?
            .as_deref()
            .and_then(valid_mac)
        {
            info = info.with_variable("mac", mac.to_owned());
        }
    }

    Ok(info)
}

/// 用 UDP `connect` 探一次路由。
///
/// **不发包**：UDP 的 `connect` 只设置默认对端并让内核做一次路由查找（正是我们要的）。
/// 查不到路由时 `connect` 返回 `ENETUNREACH`，这是「这台机器没有出口」= 无数据，
/// 不是错误；地址是 `0.0.0.0`/`::`（没探到）同样算无数据。
///
/// 只有连本地 socket 都建不起来才算**真失败**：那和「没有路由」不是一回事，
/// 掩掉它就查不出「为什么每个网络模块都是空的」。
fn probe(bind: &str, target: &str) -> Result<Option<IpAddr>, CollectError> {
    let socket = std::net::UdpSocket::bind(bind).map_err(|source| {
        CollectError::caused_by(format!("创建 UDP socket（bind {bind}）失败"), source)
    })?;

    if socket.connect(target).is_err() {
        return Ok(None);
    }

    match socket.local_addr() {
        Ok(local) if !local.ip().is_unspecified() => Ok(Some(local.ip())),
        _ => Ok(None),
    }
}

/// 一条默认路由。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Route {
    /// 出口网卡名。
    interface: String,
}

/// 从 `/proc/net/route` 里找默认路由的出口网卡。
///
/// 文件是空白分隔的固定列，第一行是表头：
/// `Iface Destination Gateway Flags RefCnt Use Metric Mask MTU Window IRTT`。
/// 默认路由的判据两条缺一不可：
///
/// - `Destination` 是 `00000000`（0.0.0.0，「所有目的地」）；
/// - `Flags` 含 `0002`（RTF_GATEWAY）。只写 `0001`（RTF_UP）的是链路路由，
///   本机的 `docker0` 那行就是，它不是默认路由。
///
/// **不用这一行的 `Mask` 算前缀长度**：默认路由的掩码就是 `00000000`，
/// 拿它当 CIDR 会印出 `/0`——错得离谱。前缀长度另找连接路由，见 [`connected_prefix`]。
fn default_route(text: &str) -> Option<Route> {
    text.lines().skip(1).find_map(|line| {
        let mut fields = line.split_whitespace();
        let interface = fields.next()?;
        let destination = fields.next()?;
        let _gateway = fields.next()?;
        let flags = u32::from_str_radix(fields.next()?, 16).ok()?;

        if destination != "00000000" || flags & 0x0002 == 0 {
            return None;
        }

        Some(Route {
            interface: interface.to_owned(),
        })
    })
}

/// 求「地址所在网段」的前缀长度：同一张网卡上掩码最长的那条路由。
///
/// 本机 `/proc/net/route` 里真正带网段的那行是
/// `enp5s0f4u1u3c2 0001A8C0 00000000 0001 … 00FFFFFF`——内核为网卡装的连接路由，
/// 它的 `Mask` 就是接口的掩码，从这里才数得出 `/24`。
///
/// 比较时把地址按**本机字节序**读成整数：内核是用 `%08X` 打印那个整数的，
/// 所以解析和比较必须用同一套规则（`from_ne_bytes`），否则大端机器上会算反。
fn connected_prefix(text: &str, interface: &str, address: Ipv4Addr) -> Option<u8> {
    let word = u32::from_ne_bytes(address.octets());
    let mut best: Option<u8> = None;

    for line in text.lines().skip(1) {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else { continue };
        let Some(destination) = fields.next().and_then(parse_hex) else {
            continue;
        };
        let _gateway = fields.next();
        let _flags = fields.next();
        let _refcnt = fields.next();
        let _use = fields.next();
        let _metric = fields.next();
        let Some(mask) = fields.next().and_then(parse_hex) else {
            continue;
        };

        // 掩码为 0 的是默认路由，它盖住所有地址，不能拿来说明网段有多宽。
        if name != interface || mask == 0 || word & mask != destination {
            continue;
        }

        if let Some(prefix) = prefix_len(mask) {
            best = Some(best.map_or(prefix, |best| best.max(prefix)));
        }
    }

    best
}

/// 网络掩码 → 前缀长度。
///
/// 打印出来的掩码整数里，1 是**低位连续**的（/24 的 `00FFFFFF`、/8 的 `000000FF`）：
/// 网络序的 `FF…FF00…00` 按本机字节序读就成了这个样子。于是「数 1 的个数」
/// 就是前缀长度，而且与大小端无关——打印与解析用的是同一套字节序。
///
/// 形状不连续（这行不干净）就返回 `None`：宁可不写前缀，也不写一个错的。
fn prefix_len(mask: u32) -> Option<u8> {
    let contiguous = mask == 0
        || mask == u32::MAX
        // 低位连续的 1，加一刚好进位成一个 2 的幂；`wrapping` 是为了 /32。
        || mask.wrapping_add(1).is_power_of_two();

    contiguous.then(|| mask.count_ones() as u8)
}

/// 按十六进制读一个路由字段。
fn parse_hex(field: &str) -> Option<u32> {
    u32::from_str_radix(field, 16).ok()
}

/// `/proc/net/if_inet6` 里的一行。
#[derive(Debug, Clone, PartialEq, Eq)]
struct Inet6 {
    /// 前缀长度。
    prefix: u8,
    /// 网卡名。
    interface: String,
}

/// 在 `/proc/net/if_inet6` 里找某个地址那一行。
///
/// 行的形状是：`地址(32 位十六进制，没有冒号) 接口号 前缀长度 作用域 标志 网卡名`。
/// 地址列不带冒号，所以比较时把自己的地址也格式化成同样的 32 位十六进制。
fn inet6_entry(text: &str, address: IpAddr) -> Option<Inet6> {
    let IpAddr::V6(address) = address else {
        return None;
    };
    let wanted = format!("{:032x}", u128::from_be_bytes(address.octets()));

    text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        let candidate = fields.next()?;
        let _index = fields.next()?;
        let prefix = u8::from_str_radix(fields.next()?, 16).ok()?;
        let _scope = fields.next()?;
        let _flags = fields.next()?;
        let interface = fields.next()?;

        (candidate == wanted).then(|| Inet6 {
            prefix,
            interface: interface.to_owned(),
        })
    })
}

/// 读一个 MAC 地址；全零（回环、没接线的网卡）视为没有。
///
/// `00:00:00:00:00:00` 是内核的「这张卡没有地址」，报出去只会让人以为有个设备。
#[must_use]
fn valid_mac(text: &str) -> Option<&str> {
    let text = text.trim();

    let zeroed = text
        .split(':')
        .all(|group| group.chars().all(|digit| digit == '0'));

    (!text.is_empty() && !zeroed && text.contains(':')).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机 `/proc/net/route` 的原文（制表符照抄，表头也留着——解析要跳过它）。
    const ROUTE_TEXT: &str = "\
Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
enp5s0f4u1u3c2\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
docker0\t000011AC\t00000000\t0001\t0\t0\t0\t0000FFFF\t0\t0\t0\n\
enp5s0f4u1u3c2\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";

    /// 本机 `/proc/net/if_inet6` 的原文。
    const INET6_TEXT: &str = "\
fe8000000000000080c08afffef2388d 05 40 20 80  docker0\n\
24088221772a15d016683a55e3462b7a 02 40 00 00 enp5s0f4u1u3c2\n\
00000000000000000000000000000001 01 80 10 80       lo\n";

    #[test]
    fn finds_the_default_route_by_destination_and_gateway_flag() {
        let route = default_route(ROUTE_TEXT).expect("第二行是默认路由");

        assert_eq!(route.interface, "enp5s0f4u1u3c2");
    }

    #[test]
    fn a_link_route_is_not_a_default_route() {
        // `docker0` 那行 Destination 是 000011AC（172.17.0.0），不是默认路由；
        // 就算它写着 00000000，Flags 里没有 0002 也一样不算。
        let link_only = "\
Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
eth0\t00000000\t00000000\t0001\t0\t0\t0\t00000000\t0\t0\t0\n";
        assert_eq!(default_route(link_only), None);

        // 表头都没有、整张表是空的，也是无数据。
        assert_eq!(default_route(""), None);
        assert_eq!(default_route("Iface\tDestination\n"), None);
    }

    #[test]
    fn the_default_line_mask_is_not_the_prefix_length() {
        // 默认路由那行的 Mask 是 00000000。要是拿它算前缀，就会印出 /0 这个假信息。
        assert_eq!(prefix_len(0), Some(0));

        let address: Ipv4Addr = "192.168.1.101".parse().unwrap();
        // 前缀来自同一张网卡上的连接路由，不是默认路由那行。
        assert_eq!(
            connected_prefix(ROUTE_TEXT, "enp5s0f4u1u3c2", address),
            Some(24)
        );
        // 换一张网卡就找不到网段了（docker0 的 172.17.0.0/16 盖不住 192.168.1.101）。
        assert_eq!(connected_prefix(ROUTE_TEXT, "docker0", address), None);
    }

    #[test]
    fn the_longest_matching_route_wins() {
        // 同一张网卡上有多条覆盖同一地址的路由时，掩码最长的那个才说明它属于哪个网段。
        // 目的地址与掩码都按内核的打印方式写：它是 `%08X` 打出来的**本机字节序整数**
        // （10.1.2.0 的网络序字节 0A 01 02 00 读成 u32 就是 0x0002010A）。
        let text = "\
Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
eth0\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
eth0\t0000000A\t00000000\t0001\t0\t0\t100\t000000FF\t0\t0\t0\n\
eth0\t0002010A\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";

        // 10.0.0.0/8 与 10.1.2.0/24 都覆盖 10.1.2.3，取 /24。
        assert_eq!(
            connected_prefix(text, "eth0", "10.1.2.3".parse().unwrap()),
            Some(24)
        );
        // 10.0.0.5 只被 /8 覆盖。
        assert_eq!(
            connected_prefix(text, "eth0", "10.0.0.5".parse().unwrap()),
            Some(8)
        );
    }

    #[test]
    fn prefix_lengths_count_the_ones_however_the_mask_is_written() {
        assert_eq!(prefix_len(0x0000_0000), Some(0));
        assert_eq!(prefix_len(0x0000_00FF), Some(8));
        assert_eq!(prefix_len(0x0000_FFFF), Some(16));
        assert_eq!(prefix_len(0x00FF_FFFF), Some(24));
        assert_eq!(prefix_len(0xFFFF_FFFF), Some(32));
        // 不连续的掩码不是合法掩码：不给答案，别编一个。
        assert_eq!(prefix_len(0x0F0F_0F0F), None);
        assert_eq!(prefix_len(0x00FF_00FF), None);
    }

    #[test]
    fn reads_the_ipv6_table_by_address() {
        let address: IpAddr = "2408:8221:772a:15d0:1668:3a55:e346:2b7a".parse().unwrap();
        let entry = inet6_entry(INET6_TEXT, address).expect("全局地址在表里");

        assert_eq!(entry.prefix, 64);
        assert_eq!(entry.interface, "enp5s0f4u1u3c2");

        // 表里没有的地址（也别忘了大小写与前导零的写法差异）就是没有。
        let missing: IpAddr = "2001:db8::1".parse().unwrap();
        assert_eq!(inet6_entry(INET6_TEXT, missing), None);
        // IPv4 地址根本不该来查这张表。
        assert_eq!(
            inet6_entry(INET6_TEXT, "192.168.1.101".parse().unwrap()),
            None
        );
    }

    #[test]
    fn only_a_real_mac_counts() {
        assert_eq!(valid_mac("6c:1f:f7:20:b6:bb"), Some("6c:1f:f7:20:b6:bb"));
        assert_eq!(valid_mac("6c:1f:f7:20:b6:bb\n"), Some("6c:1f:f7:20:b6:bb"));
        // 回环与没接线的网卡都是全零，那是「没有地址」，不是地址。
        assert_eq!(valid_mac("00:00:00:00:00:00"), None);
        assert_eq!(valid_mac(""), None);
        assert_eq!(valid_mac("not-a-mac"), None);
    }

    #[test]
    fn the_probe_never_panics_and_never_needs_a_network() {
        // 两种结局都通过：真探到地址，或者这台机器/这个容器没有出口路由。
        // 这里**不假设**本机联网——断网时内核返回 ENETUNREACH，那是预期的无数据。
        for address in [
            probe("0.0.0.0:0", PROBE4).unwrap(),
            probe("[::]:0", PROBE6).unwrap(),
        ]
        .into_iter()
        .flatten()
        {
            assert!(!address.is_unspecified());
        }
    }

    #[test]
    fn collects_on_this_machine() {
        let entries = LocalIp.collect(&Context::for_tests()).unwrap();

        // 无数据是合法结局（离线机器），有数据就得形状正确。
        for info in &entries {
            assert_eq!(info.module, "local-ip");
            assert!(info.key.starts_with("Local IP"), "实际是 {}", info.key);
            assert!(!info.value.is_empty());

            let (address, prefix) = match info.value.split_once('/') {
                Some((address, prefix)) => (address, Some(prefix)),
                None => (info.value.as_str(), None),
            };
            address
                .parse::<IpAddr>()
                .unwrap_or_else(|_| panic!("值里的地址部分应当是地址：{}", info.value));
            if let Some(prefix) = prefix {
                assert!(
                    prefix.parse::<u8>().is_ok_and(|prefix| prefix <= 128),
                    "前缀长度不合法：{}",
                    info.value
                );
            }
        }
    }
}
