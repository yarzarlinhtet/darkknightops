FROM ubuntu:24.04
RUN apt-get update && apt-get install -y --no-install-recommends openssh-server ca-certificates && rm -rf /var/lib/apt/lists/*
RUN useradd -m -s /bin/sh tester && echo 'tester:disabled-password-auth' | chpasswd && mkdir -p /run/sshd /home/tester/.ssh && chmod 700 /home/tester/.ssh
COPY entrypoint.sh /entrypoint.sh
ENTRYPOINT ["sh", "/entrypoint.sh"]
