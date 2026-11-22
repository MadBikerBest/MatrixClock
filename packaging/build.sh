#!/bin/bash
# Сборка пакетов в каталог build/ проекта:
#   packaging/build.sh            — все пакеты
#   packaging/build.sh arch       — только Arch (.pkg.tar.zst)
#   packaging/build.sh deb        — только Debian (.deb)
#   packaging/build.sh rpm        — только Fedora (.rpm)
# Все пакеты собираются только в контейнерах Docker (arch.sh, deb.sh, rpm.sh),
# несколько целей — параллельно (контейнеры независимы):
#   arch — archlinux:base-devel, deb — debian:trixie-slim, rpm — fedora:45.
# Чистая среда: бинарник собран против glibc целевой системы, а в пакет не
# попадают пути домашнего каталога и список пакетов машины сборщика.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(dirname "$here")
out="$root/build"
ver=$(sed -n 's/^version = "\(.*\)"$/\1/p' "$root/Cargo.toml" | head -1)

# версия задана в четырёх местах — проверяем, что они совпадают
grep -qx "pkgver=$ver" "$here/PKGBUILD" || { echo "PKGBUILD: pkgver не $ver (из Cargo.toml)" >&2; exit 1; }
grep -qE "^Version: +$ver$" "$here/matrixclock.spec" || { echo "matrixclock.spec: Version не $ver" >&2; exit 1; }
head -1 "$root/debian/changelog" | grep -q "($ver)" || { echo "debian/changelog: версия не $ver" >&2; exit 1; }

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir -p "$out"
tarball="$work/matrixclock-$ver.tar.gz"
tar -C "$root" --transform "s,^,matrixclock-$ver/," -czf "$tarball" \
    Cargo.toml Cargo.lock LICENSE config.example.toml src assets pam debian

[ $# -gt 0 ] || set -- arch deb rpm

build() {
    case $1 in
    arch)
        # флаги — системные из образа (-march=x86-64), ~/.makepkg.conf сборщика
        # (там бывает target-cpu=native → SIGILL на других процессорах) не участвует
        docker run --rm -e INSTALL_BUILD_DEPS=1 -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
            -v "$here/arch.sh:/arch.sh:ro" -v "$here/PKGBUILD:/PKGBUILD:ro" \
            -v "$work:/src:ro" -v "$out:/out" \
            archlinux:base-devel bash /arch.sh "/src/${tarball##*/}" /PKGBUILD /out
        ;;
    deb)
        docker run --rm -e INSTALL_BUILD_DEPS=1 -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
            -v "$here/deb.sh:/deb.sh:ro" -v "$work:/src:ro" -v "$out:/out" \
            debian:trixie-slim bash /deb.sh "/src/${tarball##*/}" /out
        ;;
    rpm)
        docker run --rm -e INSTALL_BUILD_DEPS=1 -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
            -v "$here/rpm.sh:/rpm.sh:ro" -v "$here/matrixclock.spec:/matrixclock.spec:ro" \
            -v "$work:/src:ro" -v "$out:/out" \
            fedora:45 bash /rpm.sh "/src/${tarball##*/}" /matrixclock.spec /out
        ;;
    esac
}

for t in "$@"; do
    case $t in arch | deb | rpm) ;; *) echo "неизвестная цель: $t (arch | deb | rpm)" >&2; exit 1 ;; esac
done

# контейнеры независимы — собираются одновременно; вывод каждого с меткой [цель]
declare -A pid
for t in "$@"; do
    (set -o pipefail; build "$t" 2>&1 | sed -u "s/^/[$t] /") &
    pid[$t]=$!
done
failed=()
for t in "$@"; do
    wait "${pid[$t]}" || failed+=("$t")
done
if [ ${#failed[@]} -gt 0 ]; then
    echo "сборка не удалась: ${failed[*]}" >&2
    exit 1
fi
echo "готово: $* → $out"
