//! Kernel：内核版本。
//!
//! 走 `uname(2)`（经 rustix 的安全封装）而不是读 `/proc/sys/kernel/osrelease`：
//! `uname` 在所有 Unix 上都有，这个模块因此不用绑死在 Linux 上。

use rustix::system::uname;

use crate::core::collector::{CollectError, Collector, Context};
use crate::core::info::Info;

/// 内核。
pub struct Kernel;

impl Collector for Kernel {
    fn name(&self) -> &'static str {
        "kernel"
    }

    fn collect(&self, _ctx: &Context) -> Result<Vec<Info>, CollectError> {
        // uname 没有失败路径：它在任何 Unix 上都是最基础的调用。
        let uts = uname();

        let release = uts.release().to_string_lossy().into_owned();
        let info = Info::new(self.name(), "Kernel", release.clone())
            .with_variable("release", release)
            .with_variable("sysname", uts.sysname().to_string_lossy().into_owned())
            .with_variable("machine", uts.machine().to_string_lossy().into_owned());

        Ok(vec![info])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_a_plausible_kernel_release() {
        let context = Context::for_tests();
        let entries = Kernel.collect(&context).expect("uname 不会失败");

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].module, "kernel");
        assert_eq!(entries[0].key, "Kernel");
        // 内核 release 一定有主次版本号，比如 7.2.4-arch1-2。
        assert!(
            entries[0].value.contains('.'),
            "不该是这个样子：{}",
            entries[0].value
        );
        assert_eq!(entries[0].value, entries[0].variable("release").unwrap());
    }

    #[test]
    fn sysname_identifies_the_platform() {
        let context = Context::for_tests();
        let entries = Kernel.collect(&context).unwrap();

        // 这个 crate 的 v0.1 只在 Linux 上跑，交叉验证一下数据源没串。
        assert_eq!(entries[0].variable("sysname"), Some("Linux"));
    }
}
