//! 信息：采集器与渲染器之间**唯一**的数据形态。

use crate::core::collector::ModuleName;

/// 一条信息。
///
/// 一条信息 = 终端里的一行 = JSON 里的一项。
/// 采集器产出它，渲染器消费它，中间没有任何别的中间表示。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Info {
    /// 产生它的模块名，也就是 JSON 输出里的 `type`。
    ///
    /// 由采集器填自己 `name()` 的返回值；阶段 2 之后调度器会校验二者一致。
    pub module: ModuleName,
    /// 显示键（终端左侧那一列）。
    pub key: String,
    /// 显示值（终端右侧那一列）。
    pub value: String,
    /// 模板变量表。
    ///
    /// 用 `Vec` 而不是 `HashMap`：**顺序是输出的一部分**（JSON 里按这个顺序，
    /// 模板里也不该因为哈希而换序），而且变量通常只有几个，线性查找更快。
    pub variables: Vec<(String, String)>,
}

impl Info {
    /// 新建一条信息，变量表为空。
    pub fn new(module: ModuleName, key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            module,
            key: key.into(),
            value: value.into(),
            variables: Vec::new(),
        }
    }

    /// 追加一个模板变量，链式调用。
    ///
    /// 例：`Info::new("os", "OS", "Arch Linux").with_variable("id", "arch")`。
    #[must_use]
    pub fn with_variable(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.variables.push((name.into(), value.into()));
        self
    }

    /// 取模板变量。
    pub fn variable(&self, name: &str) -> Option<&str> {
        self.variables
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}
