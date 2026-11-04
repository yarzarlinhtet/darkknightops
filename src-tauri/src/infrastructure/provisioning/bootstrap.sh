set -eu
export LC_ALL=C DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=l
apt-get -o DPkg::Lock::Timeout=120 -o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 update
set --
for p in ca-certificates curl gnupg; do
 if test "$(dpkg-query -W -f='${Status}' "$p" 2>/dev/null || true)" != 'install ok installed'; then set -- "$@" "$p"; fi
done
test "$#" -gt 0 || exit 0
simulation=$(apt-get --simulate --no-remove --no-install-recommends install "$@")
if printf '%s\n' "$simulation" | grep -E '^Remv |^Inst [^ ]+ \[' >/dev/null; then exit 44; fi
apt-get -y --no-remove --no-upgrade --no-install-recommends -o DPkg::Lock::Timeout=120 -o Dpkg::Options::=--force-confold install "$@"
