# vitals 的 fish 补全。不需要任何依赖。
#
# 安装（任选一种）：
#   install -Dm644 completions/vitals.fish ~/.config/fish/completions/vitals.fish
#   source completions/vitals.fish          # 只在当前 shell 生效
#
# 选项与模块名都不在这里硬编码：模块名现问 `vitals --list-modules`，
# 选项名由 tests/docs.rs 盯着（与 `vitals --help` 对不上就会红）。
#
# 说明文字与 zsh 那份一样是中文——帮助文本本身另有 `VITALS_LANG=zh|en`，
# 而补全的说明是它自己的一小片界面，翻译与否不改任何行为。

# 内置 Logo 的名字（与 `src/render/logos/` 下的一一对应，tests/docs.rs 会盯着）。
set -l __vitals_logos \
    auto none alpine arch centos debian fedora gentoo linux manjaro nixos opensuse ubuntu void

# `--module` 的候选：逗号分隔的列表，只补最后一段，前面的原样接回去；
# 已经点过的模块不再重复列出来。
function __vitals_modules
    set -l current (commandline -ct)
    set -l prefix ''
    set -l last $current

    # `--module=os,h` 这种等号写法：fish 会把候选接在 `=` 之后，所以先把
    # `--module=` 摘掉，候选里只留模块名与逗号前缀。
    if string match -q -- '--module=*' $current
        set current (string replace -- '--module=' '' $current)
        set last $current
    end

    if string match -q -- '*,*' $current
        set prefix (string replace -r ',[^,]*$' ',' -- $current)
        set last (string replace -r '^.*,' '' -- $current)
    end

    # 前面 `--module` 里点过的都算数：`--module os,host --module ` 不该再提 os。
    set -l used
    set -l tokens (commandline -opc)
    set -l index 1
    while test $index -le (count $tokens)
        set -l token $tokens[$index]
        if test "$token" = '--module'
            set -a used (string split , $tokens[(math $index + 1)])
        else if string match -q -- '--module=*' $token
            set -a used (string split , (string replace -- '--module=' '' $token))
        end
        set index (math $index + 1)
    end

    for module in (command vitals --list-modules 2>/dev/null)
        if contains -- $module $used
            continue
        end
        if string match -q -- "$last*" $module
            echo $prefix$module
        end
    end
end

complete -c vitals -f
complete -c vitals -s h -l help -d '打印帮助'
complete -c vitals -s V -l version -d '打印版本'
complete -c vitals -l config -r -F -d '指定配置文件'
complete -c vitals -l json -d '以 JSON 输出（自动关掉颜色与 Logo）'
complete -c vitals -l logo -x -a "$__vitals_logos" -d 'Logo：auto 按发行版自动选、none 不显示、或直接给名称'
complete -c vitals -l module -x -a '(__vitals_modules)' -d '只显示这些模块，逗号分隔'
complete -c vitals -l no-color -d '关闭颜色（等同设置 NO_COLOR）'
complete -c vitals -l list-modules -d '列出全部可用模块'
complete -c vitals -l gen-config -d '把内置默认配置打印到 stdout'
complete -c vitals -l verbose -d '把诊断信息写到 stderr'
complete -c vitals -l explain -d '逐个说明每个模块的状态：显示 / 空 / 跳过 / 失败'
complete -c vitals -l sources -d '说明每个模块实际读了哪些文件'
