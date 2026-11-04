#!/bin/sh
# Run explicitly from the repository root. Creates disposable local test artifacts.
set -eu
mkdir -p .ssh-test
chmod 700 .ssh-test
for name in ed25519 slow drop; do
  if [ ! -f ".ssh-test/$name" ]; then ssh-keygen -q -t ed25519 -N '' -f ".ssh-test/$name"; fi
done
if [ ! -f .ssh-test/rsa ]; then ssh-keygen -q -t rsa -b 3072 -N fixture-passphrase -f .ssh-test/rsa; fi
cargo run --release --manifest-path src-tauri/Cargo.toml --example ssh_key_fixture
docker build -t remotedock-ssh-test src-tauri/tests/fixtures/ssh
docker run --detach --rm --name remotedock-ssh-test -p 127.0.0.1:22222:22 --mount "type=bind,src=$PWD/.ssh-test,dst=/testkeys,readonly" remotedock-ssh-test
