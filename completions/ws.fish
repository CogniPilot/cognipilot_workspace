function __cognipilot_workspace_ws_complete
    set -l arguments (commandline -opc)
    set -e arguments[1]
    set -l current (commandline -ct)
    if test -n "$current"
        ws _complete $arguments "$current" 2>/dev/null
    else
        ws _complete $arguments '' 2>/dev/null
    end
end

complete -c ws -f -a '(__cognipilot_workspace_ws_complete)'
