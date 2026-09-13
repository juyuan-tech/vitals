//! `/proc/net/route` 的解析：这台机器的默认出口是哪张网卡。
//!
//! 两个模块要同一份判断：`local-ip` 要写地址属于哪张网卡，`net-io` 要决定采样哪一张的
//! 计数器。以前这两处各有一份规则**一字不差**的拷贝——改一处忘一处两边就会对不上，
//! 而这种不一致在屏幕上很难看出来（只是某个网卡名看着怪）。所以解析放在这里，两边共用。

/// 从 `/proc/net/route` 里挑出默认路由那张网卡的**名字**。
///
/// 判据两条，都要满足：
///
/// - `Destination` 是 `00000000`（默认路由）
/// - `Flags` 含 `0x0002`（`RTF_UP`）
///
/// 本机原文里 `docker0` 那行 `Destination` 是 `000011AC`（172.17.0.0），
/// 而 `Flags` 只有 `0001`——两条都不满足，所以它不会当选。
pub(crate) fn default_route_interface(text: &str) -> Option<String> {
    text.lines().skip(1).find_map(|line| {
        let mut fields = line.split_whitespace();
        let interface = fields.next()?;
        let destination = fields.next()?;
        let _gateway = fields.next()?;
        let flags = fields.next()?;

        let up = (u32::from_str_radix(flags, 16).ok()? & 0x0002) != 0;
        (destination == "00000000" && up).then(|| interface.to_owned())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 本机 `/proc/net/route` 的原文（含那一列以空格结尾的 `Gateway `）。
    const ROUTE_TEXT: &str = "\
Iface\tDestination\tGateway \tFlags\tRefCnt\tUse\tMetric\tMask\t\tMTU\tWindow\tIRTT\n\
enp5s0f4u1u3c2\t00000000\t0101A8C0\t0003\t0\t0\t100\t00000000\t0\t0\t0\n\
docker0\t000011AC\t00000000\t0001\t0\t0\t0\t0000FFFF\t0\t0\t0\n\
enp5s0f4u1u3c2\t0001A8C0\t00000000\t0001\t0\t0\t100\t00FFFFFF\t0\t0\t0\n";

    #[test]
    fn finds_the_default_route_interface() {
        assert_eq!(
            default_route_interface(ROUTE_TEXT).as_deref(),
            Some("enp5s0f4u1u3c2")
        );
    }

    #[test]
    fn a_link_route_is_not_a_default_route() {
        // docker0 那行 Destination 是 000011AC，不是默认路由。
        // 就算写着 00000000，Flags 里没有 0002 也一样不算（下面这张表）。
        let link_only = "\
Iface\tDestination\tGateway\tFlags\tRefCnt\tUse\tMetric\tMask\tMTU\tWindow\tIRTT\n\
eth0\t00000000\t00000000\t0001\t0\t0\t0\t00000000\t0\t0\t0\n";
        assert_eq!(default_route_interface(link_only), None);

        // 表头都没有、整张表是空的，也是无数据。
        assert_eq!(default_route_interface(""), None);

        // 只有表头、一行数据都没有：也是无数据，不是「第一行是默认路由」。
        assert_eq!(default_route_interface("Iface\tDestination\n"), None);
    }
}
