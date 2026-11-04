#!/bin/sh
set -eu
cp /testkeys/ed25519.pub /home/tester/.ssh/authorized_keys
cat /testkeys/rsa.pub >> /home/tester/.ssh/authorized_keys
printf 'command="sleep 30" ' >> /home/tester/.ssh/authorized_keys
cat /testkeys/slow.pub >> /home/tester/.ssh/authorized_keys
printf 'command="kill -9 $PPID" ' >> /home/tester/.ssh/authorized_keys
cat /testkeys/drop.pub >> /home/tester/.ssh/authorized_keys
for key in /testkeys/generated-*.pub; do cat "$key" >> /home/tester/.ssh/authorized_keys; printf '\n' >> /home/tester/.ssh/authorized_keys; done
chown -R tester:tester /home/tester/.ssh
chmod 600 /home/tester/.ssh/authorized_keys
ssh-keygen -A
exec /usr/sbin/sshd -D -e -o PasswordAuthentication=no -o PermitRootLogin=no -o HostKey=/etc/ssh/ssh_host_ed25519_key
