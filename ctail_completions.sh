#compdef ctail

autoload -U is-at-least

_ctail() {
    typeset -A opt_args
    typeset -a _arguments_options
    local ret=1

    if is-at-least 5.2; then
        _arguments_options=(-s -S -C)
    else
        _arguments_options=(-s -C)
    fi

    local context curcontext="$curcontext" state line
    _arguments "${_arguments_options[@]}" : \
'*-r+[Add a coloring rule\: PATTERN\:STYLE\[\:SCOPE\]]:RULES:_default' \
'*--rule=[Add a coloring rule\: PATTERN\:STYLE\[\:SCOPE\]]:RULES:_default' \
'--color=[Control color output]:COLOR:(auto always never)' \
'--profile=[Select a named profile from the config file]:PROFILE:_default' \
'--config=[Path to a custom config file (overrides XDG discovery)]:CONFIG:_default' \
'-n+[Number of lines to show (e.g. "10", "+5" for from-line)]:LINES:_default' \
'*--trigger=[Trigger pattern(s) — suppress output until a match, then show context window]:TRIGGER:_default' \
'--before=[Context before trigger\: line count (e.g. "20") or regex boundary (e.g. "^===")]:BEFORE:_default' \
'--after=[Context after trigger\: line count (e.g. "20") or regex boundary (e.g. "^---")]:AFTER:_default' \
'*--include=[Only show lines matching PATTERN (can be repeated)]:INCLUDE:_default' \
'*--exclude=[Hide lines matching PATTERN (can be repeated)]:EXCLUDE:_default' \
'--strip-ansi=[Strip ANSI escape codes from input before pattern matching]:STRIP_ANSI:((auto\:"Auto-detect\: strip ANSI codes (default, safest)"
always\:"Always strip ANSI codes"
never\:"Never strip ANSI codes (match against raw input)"))' \
'--list-profiles[List available profiles from the config file]' \
'--list-colors[List available color names and styles]' \
'-f[Follow file by descriptor (reopen not attempted after rename/delete)]' \
'(-f)-F[Follow file by name (reopen on rename/truncate/recreate)]' \
'-h[Print help (see more with '\''--help'\'')]' \
'--help[Print help (see more with '\''--help'\'')]' \
'-V[Print version]' \
'--version[Print version]' \
'::file -- File to read (positional argument):_default' \
":: :_ctail_commands" \
"*::: :->ctail" \
&& ret=0
    case $state in
    (ctail)
        words=($line[2] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:ctail-command-$line[2]:"
        case $line[2] in
            (completions)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
':shell -- Shell to generate completions for (bash, zsh, fish, powershell, elvish):(bash elvish fish powershell zsh)' \
&& ret=0
;;
(profile)
_arguments "${_arguments_options[@]}" : \
'-h[Print help]' \
'--help[Print help]' \
":: :_ctail__profile_commands" \
"*::: :->profile" \
&& ret=0

    case $state in
    (profile)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:ctail-profile-command-$line[1]:"
        case $line[1] in
            (new)
_arguments "${_arguments_options[@]}" : \
'--config=[Path to a custom config file (overrides XDG discovery)]:CONFIG:_default' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(edit)
_arguments "${_arguments_options[@]}" : \
'--config=[Path to a custom config file (overrides XDG discovery)]:CONFIG:_default' \
'-h[Print help]' \
'--help[Print help]' \
'::name -- Profile name to edit:_default' \
&& ret=0
;;
(delete)
_arguments "${_arguments_options[@]}" : \
'--config=[Path to a custom config file (overrides XDG discovery)]:CONFIG:_default' \
'-h[Print help]' \
'--help[Print help]' \
'::name -- Profile name to delete:_default' \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
'--config=[Path to a custom config file (overrides XDG discovery)]:CONFIG:_default' \
'-h[Print help]' \
'--help[Print help]' \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_ctail__profile__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:ctail-profile-help-command-$line[1]:"
        case $line[1] in
            (new)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(edit)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(delete)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
;;
(help)
_arguments "${_arguments_options[@]}" : \
":: :_ctail__help_commands" \
"*::: :->help" \
&& ret=0

    case $state in
    (help)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:ctail-help-command-$line[1]:"
        case $line[1] in
            (completions)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(profile)
_arguments "${_arguments_options[@]}" : \
":: :_ctail__help__profile_commands" \
"*::: :->profile" \
&& ret=0

    case $state in
    (profile)
        words=($line[1] "${words[@]}")
        (( CURRENT += 1 ))
        curcontext="${curcontext%:*:*}:ctail-help-profile-command-$line[1]:"
        case $line[1] in
            (new)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(edit)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(delete)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
(list)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
(help)
_arguments "${_arguments_options[@]}" : \
&& ret=0
;;
        esac
    ;;
esac
;;
        esac
    ;;
esac
}

(( $+functions[_ctail_commands] )) ||
_ctail_commands() {
    local commands; commands=(
'completions:Generate shell completions' \
'profile:Manage config profiles (new, edit, delete, list)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'ctail commands' commands "$@"
}
(( $+functions[_ctail__completions_commands] )) ||
_ctail__completions_commands() {
    local commands; commands=()
    _describe -t commands 'ctail completions commands' commands "$@"
}
(( $+functions[_ctail__help_commands] )) ||
_ctail__help_commands() {
    local commands; commands=(
'completions:Generate shell completions' \
'profile:Manage config profiles (new, edit, delete, list)' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'ctail help commands' commands "$@"
}
(( $+functions[_ctail__help__completions_commands] )) ||
_ctail__help__completions_commands() {
    local commands; commands=()
    _describe -t commands 'ctail help completions commands' commands "$@"
}
(( $+functions[_ctail__help__help_commands] )) ||
_ctail__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'ctail help help commands' commands "$@"
}
(( $+functions[_ctail__help__profile_commands] )) ||
_ctail__help__profile_commands() {
    local commands; commands=(
'new:Create a new profile interactively' \
'edit:Edit an existing profile' \
'delete:Delete a profile' \
'list:List all profiles' \
    )
    _describe -t commands 'ctail help profile commands' commands "$@"
}
(( $+functions[_ctail__help__profile__delete_commands] )) ||
_ctail__help__profile__delete_commands() {
    local commands; commands=()
    _describe -t commands 'ctail help profile delete commands' commands "$@"
}
(( $+functions[_ctail__help__profile__edit_commands] )) ||
_ctail__help__profile__edit_commands() {
    local commands; commands=()
    _describe -t commands 'ctail help profile edit commands' commands "$@"
}
(( $+functions[_ctail__help__profile__list_commands] )) ||
_ctail__help__profile__list_commands() {
    local commands; commands=()
    _describe -t commands 'ctail help profile list commands' commands "$@"
}
(( $+functions[_ctail__help__profile__new_commands] )) ||
_ctail__help__profile__new_commands() {
    local commands; commands=()
    _describe -t commands 'ctail help profile new commands' commands "$@"
}
(( $+functions[_ctail__profile_commands] )) ||
_ctail__profile_commands() {
    local commands; commands=(
'new:Create a new profile interactively' \
'edit:Edit an existing profile' \
'delete:Delete a profile' \
'list:List all profiles' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'ctail profile commands' commands "$@"
}
(( $+functions[_ctail__profile__delete_commands] )) ||
_ctail__profile__delete_commands() {
    local commands; commands=()
    _describe -t commands 'ctail profile delete commands' commands "$@"
}
(( $+functions[_ctail__profile__edit_commands] )) ||
_ctail__profile__edit_commands() {
    local commands; commands=()
    _describe -t commands 'ctail profile edit commands' commands "$@"
}
(( $+functions[_ctail__profile__help_commands] )) ||
_ctail__profile__help_commands() {
    local commands; commands=(
'new:Create a new profile interactively' \
'edit:Edit an existing profile' \
'delete:Delete a profile' \
'list:List all profiles' \
'help:Print this message or the help of the given subcommand(s)' \
    )
    _describe -t commands 'ctail profile help commands' commands "$@"
}
(( $+functions[_ctail__profile__help__delete_commands] )) ||
_ctail__profile__help__delete_commands() {
    local commands; commands=()
    _describe -t commands 'ctail profile help delete commands' commands "$@"
}
(( $+functions[_ctail__profile__help__edit_commands] )) ||
_ctail__profile__help__edit_commands() {
    local commands; commands=()
    _describe -t commands 'ctail profile help edit commands' commands "$@"
}
(( $+functions[_ctail__profile__help__help_commands] )) ||
_ctail__profile__help__help_commands() {
    local commands; commands=()
    _describe -t commands 'ctail profile help help commands' commands "$@"
}
(( $+functions[_ctail__profile__help__list_commands] )) ||
_ctail__profile__help__list_commands() {
    local commands; commands=()
    _describe -t commands 'ctail profile help list commands' commands "$@"
}
(( $+functions[_ctail__profile__help__new_commands] )) ||
_ctail__profile__help__new_commands() {
    local commands; commands=()
    _describe -t commands 'ctail profile help new commands' commands "$@"
}
(( $+functions[_ctail__profile__list_commands] )) ||
_ctail__profile__list_commands() {
    local commands; commands=()
    _describe -t commands 'ctail profile list commands' commands "$@"
}
(( $+functions[_ctail__profile__new_commands] )) ||
_ctail__profile__new_commands() {
    local commands; commands=()
    _describe -t commands 'ctail profile new commands' commands "$@"
}

if [ "$funcstack[1]" = "_ctail" ]; then
    _ctail "$@"
else
    compdef _ctail ctail
fi
