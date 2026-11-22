#!/bin/bash
# Сборка .rpm: rpm.sh <архив.tar.gz> <spec> <каталог-результата>
# Вызывается из build.sh только в контейнере fedora:45; зависимости сборки
# (INSTALL_BUILD_DEPS=1) ставятся там сами.
set -euo pipefail
tarball=$(realpath "$1")
spec=$(realpath "$2")
out=$(realpath "$3")
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if [ "${INSTALL_BUILD_DEPS:-0}" = 1 ]; then
    dnf -y -q install rpm-build >/dev/null
    dnf -y -q builddep "$spec" >/dev/null
fi

# сеть — только здесь; сама сборка (%build в spec) идёт с --frozen, без сети
tar -C "$work" -xzf "$tarball"
(cd "$work"/matrixclock-* && cargo fetch --locked --target "$(rustc -vV | sed -n 's/^host: //p')")

mkdir -p "$work/rpmbuild/SOURCES"
cp "$tarball" "$work/rpmbuild/SOURCES/"
rpmbuild -bb --define "_topdir $work/rpmbuild" "$spec"
cp "$work"/rpmbuild/RPMS/*/matrixclock-*.rpm "$out/"
if [ -n "${HOST_UID:-}" ]; then
    chown "$HOST_UID:$HOST_GID" "$out"/matrixclock-*.rpm
fi
