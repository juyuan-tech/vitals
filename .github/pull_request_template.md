## 做了什么

<!-- 一句话说清这个 PR 解决什么问题 -->

## 检查清单

- [ ] `cargo test` 通过
- [ ] `cargo clippy --all-targets -- -D warnings` 通过
- [ ] `cargo fmt --all -- --check` 通过
- [ ] 没有新增依赖
- [ ] 没有引入 `unsafe`
- [ ] 没有执行外部命令、没有发网络包、没有写文件
- [ ] 测试不依赖宿主状态（headless CI 上也能过；真实样本用了 guard）
- [ ] 同步了文档（`README.md`、`docs/` 里受影响的部分）
- [ ] 用户可见的变化已补 `CHANGELOG.md`

## 文档里贴的输出

<!-- 如果 PR 里改动了文档中的示例输出，请说明这些输出是从哪条命令、在什么环境下跑出来的 -->
