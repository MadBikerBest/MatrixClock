#!/bin/bash
# Сборка .deb из архива исходников: deb.sh <архив.tar.gz> <каталог-результата>
# Вызывается из build.sh только в контейнере debian:trixie-slim. Нужен rustc/cargo
# ≥ 1.88: в Debian 13 штатный 1.85, поэтому берутся из официального trixie-backports
# (только для сборки, готовому пакету Rust не нужен).
# В контейнере (INSTALL_BUILD_DEPS=1) всё это делается само.
set -euo pipefail
tarball=$(realpath "$1")
out=$(realpath "$2")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
tar -C "$work" -xzf "$tarball"
src=$(echo "$work"/matrixclock-*)

if [ "${INSTALL_BUILD_DEPS:-0}" = 1 ]; then
    export DEBIAN_FRONTEND=noninteractive
    cat > /etc/apt/sources.list.d/backports.sources <<'SRC'
Types: deb
URIs: http://deb.debian.org/debian
Suites: trixie-backports
Components: main
Signed-By: /usr/share/keyrings/debian-archive-keyring.pgp
SRC
    apt-get update -qq
    apt-get install -y -qq --no-install-recommends ca-certificates dpkg-dev >/dev/null
    apt-get build-dep -y -qq -t trixie-backports "$src" >/dev/null
fi

cd "$src"
# сеть — только здесь; сама сборка (debian/rules) идёт с --frozen, без сети
cargo fetch --locked --target "$(rustc -vV | sed -n 's/^host: //p')"
dpkg-buildpackage -b -us -uc
cp "$work"/matrixclock_*.deb "$out/"
if [ -n "${HOST_UID:-}" ]; then
    chown "$HOST_UID:$HOST_GID" "$out"/matrixclock_*.deb
fi
