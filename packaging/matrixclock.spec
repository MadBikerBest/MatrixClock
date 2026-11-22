# Пакет RPM (Fedora). Собирается packaging/build.sh rpm — в контейнере fedora:45
# (бинарник должен собираться против glibc той системы, где будет работать).
# Зависимости крейтов скачивает packaging/rpm.sh (cargo fetch --locked) до
# rpmbuild, сама сборка идёт без сети (--frozen), как в PKGBUILD и debian/rules.

# бинарник уже без символов (strip = true в Cargo.toml) — отладочный пакет пуст
%global debug_package %{nil}

Name:           matrixclock
Version:        0.1.0
Release:        1%{?dist}
Summary:        Matrix-style 3D clock screensaver and session locker
# MIT — программа, OFL — встроенные шрифты (Noto Sans JP, Russo One)
License:        MIT AND OFL-1.1
Source0:        %{name}-%{version}.tar.gz

BuildRequires:  cargo >= 1.88
BuildRequires:  rust >= 1.88
BuildRequires:  gcc
BuildRequires:  pkgconf-pkg-config
BuildRequires:  pam-devel
BuildRequires:  libX11-devel
BuildRequires:  libXrandr-devel
BuildRequires:  libXScrnSaver-devel
BuildRequires:  libglvnd-devel

%description
Glowing 3D digits filled with falling katakana rain. Under X11 it renders
with OpenGL (bloom, glass digits, depth layers); in a terminal or on the
bare Linux console it draws with text characters. Weather and date are
shown above and below the clock.

Optional session lock: password via PAM, the color shifts towards red
with each wrong attempt.

%prep
%autosetup

%build
export CARGO_TARGET_DIR=target
cargo build --release --frozen

%install
install -Dpm 0755 target/release/matrixclock %{buildroot}%{_bindir}/matrixclock
install -Dpm 0644 pam/matrixclock %{buildroot}%{_sysconfdir}/pam.d/matrixclock
install -Dpm 0644 assets/fonts/MatrixClockGlyphs.otf %{buildroot}%{_datadir}/fonts/matrixclock/MatrixClockGlyphs.otf

%check
export CARGO_TARGET_DIR=target
cargo test --frozen

%files
%license LICENSE assets/fonts/OFL.txt assets/fonts/OFL-Russo.txt
%doc config.example.toml
%{_bindir}/matrixclock
%config(noreplace) %{_sysconfdir}/pam.d/matrixclock
%{_datadir}/fonts/matrixclock/

%changelog
* Sun Nov 22 2026 Суслов П. В. <9114339+MadBikerBest@users.noreply.github.com> - 0.1.0-1
- Первый пакет.
