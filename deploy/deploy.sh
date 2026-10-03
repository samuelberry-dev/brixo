#!/usr/bin/env bash
# Puts the latest Brixo live. Run as root on the server:
#   bash /opt/brixo/deploy/deploy.sh
# Safe to run again: it pulls, builds, installs and restarts.
set -euo pipefail
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env"

if [ -z "${BRIXO_DEPLOY_FRESH:-}" ]; then
    echo "== Getting the latest code"
    # A build can touch Cargo.lock here; the copy in git always wins.
    git checkout -- Cargo.lock 2>/dev/null || true
    git pull --ff-only
    # The pull may have changed this very script, and bash would carry on
    # running the old one it already started reading: start over with the new.
    BRIXO_DEPLOY_FRESH=1 exec bash "$0" "$@"
fi

echo "== Building (a few minutes; longer the first time)"
cargo build --release -p brixo-web

echo "== Installing"
id brixo >/dev/null 2>&1 || useradd --system --user-group --home-dir /var/lib/brixo --shell /usr/sbin/nologin brixo

# A backup before anything changes: if this deploy goes wrong, put it back
# with: brixo-backup restore (see brixo-backup list). Made with the new
# brixo-web (the old one may not know "backup"), without upgrading the
# database first, so it's exactly what the running version left.
if [ -f /var/lib/brixo/brixo-web.sqlite ] && [ -z "${BRIXO_SKIP_BACKUP:-}" ]; then
    echo "== Backing up the database first"
    install -m 755 target/release/brixo-web /usr/local/bin/brixo-web.new
    BRIXO_BIN=/usr/local/bin/brixo-web.new bash deploy/brixo-backup pre-deploy \
        || { echo "== The backup failed, so nothing was deployed. (BRIXO_SKIP_BACKUP=1 to deploy anyway.)"; exit 1; }
    rm -f /usr/local/bin/brixo-web.new
fi
install -d -o brixo -g brixo -m 750 /var/lib/brixo
install -d -m 755 /etc/brixo
[ -f /etc/brixo/brixo.env ] || install -m 640 -g brixo deploy/brixo.env /etc/brixo/brixo.env
# Settings added since the first deploy (the file is yours after that).
grep -q '^BRIXO_DOWNLOADS=' /etc/brixo/brixo.env || echo 'BRIXO_DOWNLOADS=/var/www/brixo-downloads' >> /etc/brixo/brixo.env
# Downloads: uploaded by tools/release.ps1, served by Caddy.
install -d -m 755 /var/www/brixo-downloads
install -m 755 target/release/brixo-web /usr/local/bin/brixo-web
install -m 755 deploy/brixo-admin /usr/local/bin/brixo-admin
install -m 755 deploy/brixo-backup /usr/local/bin/brixo-backup
install -m 644 deploy/brixo-backup.service /etc/systemd/system/brixo-backup.service
install -m 644 deploy/brixo-backup.timer /etc/systemd/system/brixo-backup.timer
install -m 644 deploy/brixo-web.service /etc/systemd/system/brixo-web.service
install -m 644 deploy/Caddyfile /etc/caddy/Caddyfile

echo "== Starting"
systemctl daemon-reload
systemctl enable brixo-web >/dev/null 2>&1
systemctl enable --now brixo-backup.timer >/dev/null 2>&1
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
