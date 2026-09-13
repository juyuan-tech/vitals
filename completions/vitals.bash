# vitals 的 bash 补全。不需要任何依赖。
#
# 安装（任选一种）：
#   install -Dm644 completions/vitals.bash ~/.local/share/bash-completion/completions/vitals
#   source completions/vitals.bash          # 只在当前 shell 生效
#
# 选项与模块名都不在这里硬编码：模块名现问 `vitals --list-modules`，
# 选项名由 tests/docs.rs 盯着（与 `vitals --help` 对不上就会红）。

_vitals_modules()
{
    command vitals --list-modules 2>/dev/null
}

_vitals_logos()
{
    printf '%s' "auto none alpine arch centos debian fedora gentoo linux manjaro nixos opensuse ubuntu void"
}

_vitals()
{
    local cur prev
    cur=${COMP_WORDS[COMP_CWORD]}
    prev=${COMP_WORDS[COMP_CWORD - 1]}

    case $prev in
        --config)
            COMPREPLY=($(compgen -f -- "$cur"))
            return
            ;;
        --logo)
            COMPREPLY=($(compgen -W "$(_vitals_logos)" -- "$cur"))
            return
            ;;
        --module)
            # 逗号分隔的列表：只补最后一段，前面的原样接回去；
            # 已经点过的模块不再重复列出来。
            local head="" last=$cur used=" "
            if [[ $cur == *,* ]]; then
                head=${cur%,*},
                last=${cur##*,}
                used=" ${cur%,*},"
                used=${used//,/ }
            fi
            COMPREPLY=()
            local m
            for m in $(_vitals_modules); do
                [[ $used == *" $m "* ]] && continue
                [[ $m == "$last"* ]] && COMPREPLY+=("$head$m")
            done
            return
            ;;
    esac

    COMPREPLY=($(compgen -W "--config --explain --gen-config --help --json --list-modules --logo --module --no-color --sources --verbose --version -h -V" -- "$cur"))
}

complete -F _vitals vitals
