# justfile for dispenser project

DISPENSER_VERSION := "0.7"
TARGET_BIN := "target/x86_64-unknown-linux-musl/release/dispenser"
USR_BIN_DEB := "deb/usr/local/bin/dispenser"
USR_BIN_RPM := "rpm/usr/local/bin/dispenser"

build:
  CARGO_TARGET_DIR="./target" cargo build --release --target "x86_64-unknown-linux-musl"

build-deb: build
  mkdir -p deb/usr/local/bin/
  rm -f {{USR_BIN_DEB}}
  mv {{TARGET_BIN}} {{USR_BIN_DEB}}
  dpkg-deb --build deb
  rm -f dispenser.deb
  mv deb.deb dispenser-{{DISPENSER_VERSION}}-0.x86_64.deb

build-rpm: build
  rm -rf rpmstage rpmout
  mkdir -p rpmstage/usr/local/bin
  mkdir -p rpmstage/usr/lib/systemd/system
  mkdir -p rpmstage/opt/dispenser
  mkdir -p rpmout
  cp {{TARGET_BIN}} rpmstage/usr/local/bin/dispenser
  cp rpm/usr/lib/systemd/system/dispenser.service rpmstage/usr/lib/systemd/system/
  rpmbuild --target=x86_64 --buildroot $(pwd)/rpmstage --define "_topdir $(pwd)/rpmout" -bb rpm/dispenser.spec --noclean
