#!/bin/bash
# Сборка пакета Arch: arch.sh <архив.tar.gz> <PKGBUILD> <каталог-результата>
# Вызывается из build.sh в контейнере archlinux:base-devel — чистая среда, как
# у devtools: в пакет не попадают пути домашнего каталога (бинарник Rust хранит
# пути к исходникам крейтов) и список пакетов машины сборщика (.BUILDINFO).
set -euo pipefail
tarball=$(realpath "$1")
pkgbuild=$(realpath "$2")
out=$(realpath "$3")

if [ "${INSTALL_BUILD_DEPS:-0}" = 1 ]; then
    # shellcheck source=/dev/null
    source "$pkgbuild"
    pacman -Syu --noconfirm --needed "${depends[@]}" "${makedepends[@]}" >/dev/null
fi

# makepkg не работает от root — собирает отдельный пользователь
id builder >/dev/null 2>&1 || useradd -m builder
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp "$tarball" "$pkgbuild" "$work/"
chown -R builder: "$work"
runuser -u builder -- env -C "$work" PKGDEST="$work" makepkg -f
cp "$work"/matrixclock-*.pkg.tar.zst "$out/"
if [ -n "${HOST_UID:-}" ]; then
    chown "$HOST_UID:$HOST_GID" "$out"/matrixclock-*.pkg.tar.zst
fi
