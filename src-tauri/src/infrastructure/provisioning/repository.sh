set -eu
export LC_ALL=C
arch=$(dpkg --print-architecture)
case "$arch" in amd64|arm64) ;; *) exit 42;; esac
# Existing custom files must never be replaced, including after a partial attempt.
test ! -e /etc/apt/sources.list.d/docker.sources
test ! -e /etc/apt/keyrings/docker.asc
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
curl --proto '=https' --connect-timeout 10 --max-time 60 -fsS https://download.docker.com/linux/ubuntu/gpg -o "$work/docker.asc"
metadata=$(gpg --batch --no-options --homedir "$work" --show-keys --with-colons "$work/docker.asc")
test "$(printf '%s\n' "$metadata" | awk -F: '$1=="pub" {n++} END {print n+0}')" = 1
fp=$(printf '%s\n' "$metadata" | awk -F: '$1=="fpr" {print $10; exit}')
test "$fp" = 9DC858229FC7DD38854AE2D88D81803C0EBFCD88
install -d -m 0755 /etc/apt/keyrings
install -m 0644 "$work/docker.asc" /etc/apt/keyrings/docker.asc
printf 'Types: deb\nURIs: https://download.docker.com/linux/ubuntu\nSuites: noble\nComponents: stable\nArchitectures: %s\nSigned-By: /etc/apt/keyrings/docker.asc\n' "$arch" > "$work/docker.sources"
install -m 0644 "$work/docker.sources" /etc/apt/sources.list.d/docker.sources
