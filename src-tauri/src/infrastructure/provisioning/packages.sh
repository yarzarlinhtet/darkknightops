set -eu
export LC_ALL=C DEBIAN_FRONTEND=noninteractive NEEDRESTART_MODE=l
apt-get -o DPkg::Lock::Timeout=120 -o Acquire::Retries=2 -o Acquire::http::Timeout=30 -o Acquire::https::Timeout=30 update
# Only missing packages; never upgrade a healthy engine as a side effect.
set --
for p in docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin; do
 if test "$(dpkg-query -W -f='${Status}' "$p" 2>/dev/null || true)" != 'install ok installed'; then set -- "$@" "$p"; fi
done
test "$#" -gt 0 || exit 0
for p in "$@"; do
 candidate=$(apt-cache policy "$p" | awk '/Candidate:/ {print $2; exit}')
 test -n "$candidate" && test "$candidate" != '(none)'
 apt-cache madison "$p" | awk -F '|' -v version="$candidate" '
 {v=$2; gsub(/^[ \t]+|[ \t]+$/, "", v); if (v==version && $3 ~ /https:\/\/download.docker.com\/linux\/ubuntu +noble\/stable/) found=1}
 END {exit !found}'
done
# Reject transactions that remove packages or upgrade any installed package.
simulation=$(apt-get --simulate --no-remove --no-install-recommends install "$@")
if printf '%s\n' "$simulation" | grep -q '^Remv '; then exit 43; fi
if printf '%s\n' "$simulation" | grep -E '^Inst [^ ]+ \[' >/dev/null; then exit 44; fi
# Pin every package candidate in the reviewed transaction, closing upgrade drift.
pins=$(printf '%s\n' "$simulation" | awk '/^Inst / {v=$3; sub(/^\(/,"",v); print $2 "=" v}' | tr '\n' ' ')
test -n "$pins"
# Package names/versions originate from apt, but validate before word splitting.
case "$pins" in *[!a-zA-Z0-9.+:~=_\ -]*) exit 45;; esac
apt-get -y --no-remove --no-upgrade --no-install-recommends -o DPkg::Lock::Timeout=120 -o Dpkg::Options::=--force-confold install $pins
