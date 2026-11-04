# Read only. Never source os-release or evaluate repository contents.
export LC_ALL=C
emit() { printf '%s=%s\n' "$1" "$2"; }
installed() { test "$(dpkg-query -W -f='${Status}' "$1" 2>/dev/null)" = 'install ok installed'; }
emit username "$(id -un)"
emit uid "$(id -u)"
emit architecture "$(dpkg --print-architecture 2>/dev/null)"
emit kernel "$(uname -r)"
emit systemd "$(test -d /run/systemd/system && command -v systemctl >/dev/null && systemctl show -p Version --value >/dev/null 2>&1 && echo yes)"
emit apt "$(command -v apt-get >/dev/null && echo yes)"
privileged() { if test "$(id -u)" = 0; then "$@"; else sudo -n "$@"; fi; }
admin=no
if test "$(id -u)" = 0; then admin=root
elif sudo -n /usr/bin/true </dev/null 2>/dev/null && sudo -n -l /usr/bin/systemd-run </dev/null >/dev/null 2>&1; then admin=passwordless
elif sudo -n -l </dev/null 2>&1 | grep -qi 'password'; then admin=interactive
fi
emit privilege "$admin"
emit dns "$(getent ahosts download.docker.com >/dev/null 2>&1 && echo yes)"
# curl or wget are optional preflight tools; missing both blocks with guidance.
fetch() {
 if command -v curl >/dev/null; then curl --proto '=https' --connect-timeout 5 --max-time 10 -fsS -o /dev/null "$1" 2>/dev/null
 elif command -v wget >/dev/null; then wget --timeout=10 --tries=1 -q -O /dev/null "$1" 2>/dev/null
 else return 1; fi
}
emit network "$(fetch https://download.docker.com/linux/ubuntu/dists/noble/Release && echo yes)"
case "$(dpkg --print-architecture 2>/dev/null)" in
 amd64) emit ubuntu_network "$(fetch https://archive.ubuntu.com/ubuntu/dists/noble/Release && fetch https://security.ubuntu.com/ubuntu/dists/noble-security/Release && echo yes)";;
 arm64) emit ubuntu_network "$(fetch https://ports.ubuntu.com/ubuntu-ports/dists/noble/Release && echo yes)";;
esac
for path in / /var /var/lib/docker; do
 if test -d "$path"; then emit disk "$(df -Pk "$path" | awk 'NR==2 {print $4}')"; fi
done
conflicts=no
for p in docker.io docker-compose docker-compose-v2 docker-doc docker-buildx podman-docker containerd runc; do
 if installed "$p"; then conflicts=yes; fi
done
emit conflicts "$conflicts"
emit source "$(if installed docker-ce; then echo official-package; elif installed docker.io; then echo distribution; else echo unknown; fi)"
emit known_packages "$(if installed docker-ce || installed docker-ce-cli || installed containerd.io; then echo yes; fi)"
emit cli "$(command -v docker >/dev/null && echo yes)"
emit engine "$(installed docker-ce && installed docker-ce-cli && installed containerd.io && echo yes)"
emit buildx "$(installed docker-buildx-plugin && echo yes)"
emit compose "$(docker compose version --short 2>/dev/null)"
emit client_version "$(docker --version 2>/dev/null)"
emit enabled "$(systemctl is-enabled docker.service 2>/dev/null)"
emit running "$(systemctl is-active docker.service 2>/dev/null)"
emit service "$(systemctl show docker.service -p LoadState --value 2>/dev/null)"
emit group "$(id -nG | tr ' ' '\n' | grep -x docker)"
emit account_group "$(id -nG "$(id -un)" | tr ' ' '\n' | grep -x docker)"
if docker --host unix:///var/run/docker.sock info --format '{{.ServerVersion}}' >/dev/null 2>&1; then
 emit access direct
 emit server_version "$(docker --host unix:///var/run/docker.sock version --format '{{.Server.Version}}' 2>/dev/null)"
 emit workloads "$(docker --host unix:///var/run/docker.sock ps -aq 2>/dev/null | wc -l)"
elif test "$admin" = root || test "$admin" = passwordless; then
 if privileged /usr/bin/docker --host unix:///var/run/docker.sock info >/dev/null 2>&1; then
 emit access administrative
 emit server_version "$(privileged /usr/bin/docker --host unix:///var/run/docker.sock version --format '{{.Server.Version}}' 2>/dev/null)"
 emit workloads "$(privileged /usr/bin/docker --host unix:///var/run/docker.sock ps -aq 2>/dev/null | wc -l)"
 else emit access unavailable; fi
else emit access unavailable; fi
emit audit "$(dpkg --audit 2>/dev/null | wc -c)"
emit active_units "$(systemctl list-units --no-legend --state=activating,running 'remotedock-provision-*.service' 2>/dev/null | wc -l)"
# Report repository contents only to the Rust validator; never execute them.
for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources; do
 test -f "$f" || continue
 if grep -q 'download.docker.com' "$f"; then
 emit repository_file "$f"
 sed 's/^/repository_line=/' "$f"
 fi
done
if test -e /etc/apt/sources.list.d/docker.sources; then emit reserved_source yes; fi
if test -e /etc/apt/keyrings/docker.asc; then
 emit reserved_key yes
 if command -v gpg >/dev/null; then
  metadata=$(gpg --batch --no-options --no-default-keyring --no-auto-check-trustdb --trustdb-name /dev/null --lock-never --no-random-seed-file --homedir /tmp --show-keys --with-colons /etc/apt/keyrings/docker.asc 2>/dev/null)
  emit key_count "$(printf '%s\n' "$metadata" | awk -F: '$1=="pub" {n++} END {print n+0}')"
  emit key_fingerprint "$(printf '%s\n' "$metadata" | awk -F: '$1=="fpr" {print $10; exit}')"
 fi
fi
