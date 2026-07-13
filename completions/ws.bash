# shellcheck shell=bash

_cognipilot_workspace_ws_completion() {
	local -a candidates=()
	mapfile -t candidates < <(
		ws _complete "${COMP_WORDS[@]:1:COMP_CWORD}" 2>/dev/null
	)
	mapfile -t COMPREPLY < <(compgen -W "${candidates[*]}" -- "${COMP_WORDS[COMP_CWORD]}")
}

complete -o nosort -F _cognipilot_workspace_ws_completion ws
