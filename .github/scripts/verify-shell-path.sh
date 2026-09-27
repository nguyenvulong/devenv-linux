#!/usr/bin/env bash
# verify-shell-path.sh -- check that mise and mise-managed tools are on PATH
# in every common way a user opens a shell, after devenv ran for that user.
#
# Regression guard for installs that select mise tools but not the optional
# "Bash Configuration": devenv must still make them reachable.
#
# Usage (as root):
#   verify-shell-path.sh <user> <command>...
#
#   <user>     account devenv was run for (may be root)
#   <command>  commands that must resolve, e.g. mise node
#
# Shell entry points checked:
#   su <user>            non-login interactive bash; keeps the caller's
#                        environment and reads only ~/.bashrc
#   su - <user>          login shell running a command; reads the login
#                        profile (~/.bash_profile or ~/.profile)
#   su - <user>, bash -i login environment plus an interactive shell
#
# Exits non-zero, naming the entry point and command, on the first miss.
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "usage: $0 <user> <command>..." >&2
  exit 2
fi

user=$1
shift

# Runs inside the target shell: resolve each command or fail loudly.
check="for c in $*; do command -v \"\$c\" >/dev/null || { echo \"missing: \$c\"; exit 1; }; echo \"\$c -> \$(command -v \"\$c\")\"; done"

run() {
  local label=$1
  shift
  echo "== ${label}"
  # Interactive bash without a terminal warns about job control; hide that.
  if ! "$@" 2> >(grep -v -e 'no job control' -e 'cannot set terminal process group' >&2); then
    echo "FAIL: ${label}" >&2
    exit 1
  fi
}

run "su ${user} (non-login, interactive)" su "$user" -s /bin/bash -c "bash -ic '${check}'"
run "su - ${user} (login)" su - "$user" -s /bin/bash -c "${check}"
run "su - ${user} + bash -i (login, interactive)" su - "$user" -s /bin/bash -c "bash -ic '${check}'"
echo "All shell entry points resolve: $*"
