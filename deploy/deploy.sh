#!/usr/bin/env bash
# Puts the latest Brixo live. Run as root on the server:
#   bash /opt/brixo/deploy/deploy.sh
# Safe to run again: it pulls, builds, installs and restarts.
set -euo pipefail
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env"

echo "== Getting the latest code"
# A build can touch Cargo.lock here; the copy in git always wins.
git checkout -- Cargo.lock 2>/dev/null || true
git pull --ff-only

echo "== Building (a few minutes; longer the first time)"
cargo build --release -p brixo-web

echo "== Installing"
id brixo >/dev/null 2>&1 || useradd --system --user-group --home-dir /var/lib/brixo --shell /usr/sbin/nologin brixo
install -d -o brixo -g brixo -m 750 /var/lib/brixo
install -d -m 755 /etc/brixo
[ -f /etc/brixo/brixo.env ] || install -m 640 -g brixo deploy/brixo.env /etc/brixo/brixo.env
install -m 755 target/release/brixo-web /usr/local/bin/brixo-web
install -m 755 deploy/brixo-admin /usr/local/bin/brixo-admin
install -m 644 deploy/brixo-web.service /etc/systemd/system/brixo-web.service
install -m 644 deploy/Caddyfile /etc/caddy/Caddyfile

echo "== Starting"
systemctl daemon-reload
systemctl enable brixo-web >/dev/null 2>&1
systemctl restart brixo-web
systemctl reload caddy 2>/dev/null || systemctl restart caddy
sleep 2
if systemctl is-active --quiet brixo-web; then
    echo "== Brixo is running. Recent log:"
    journalctl -u brixo-web -n 6 --no-pager
else
    echo "== Brixo didn't start. Log:"
    journalctl -u brixo-web -n 30 --no-pager
    exit 1
fi
