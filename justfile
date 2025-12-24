# justfile for dispenser project

DISPENSER_VERSION := shell('grep "^version" Cargo.toml | head -n1 | cut -d \" -f 2')
TARGET_BIN := "target/x86_64-unknown-linux-musl/release/dispenser"
USR_BIN_RPM := "rpm/usr/local/bin/dispenser"

version:
  echo "{{DISPENSER_VERSION}}"

build:
  CARGO_TARGET_DIR="./target" cargo build --release --target "x86_64-unknown-linux-musl"

build-deb: build
  rm -rf target/deb_stage
  mkdir -p target/deb_stage
  cp -R deb/* target/deb_stage/
  mkdir -p target/deb_stage/usr/local/bin
  cp {{TARGET_BIN}} target/deb_stage/usr/local/bin/dispenser
  sed 's/VERSION_PLACEHOLDER/{{DISPENSER_VERSION}}/' deb/DEBIAN/control > target/deb_stage/DEBIAN/control
  dpkg-deb --build target/deb_stage dispenser-{{DISPENSER_VERSION}}-0.x86_64.deb

build-rpm: build
  rm -rf rpmstage rpmout
  mkdir -p rpmstage/usr/local/bin
  mkdir -p rpmstage/usr/lib/systemd/system
  mkdir -p rpmstage/opt/dispenser
  mkdir -p rpmout
  cp {{TARGET_BIN}} rpmstage/usr/local/bin/dispenser
  cp rpm/usr/lib/systemd/system/dispenser.service rpmstage/usr/lib/systemd/system/
  rpmbuild --target=x86_64 --buildroot $(pwd)/rpmstage --define "_topdir $(pwd)/rpmout" --define "version {{DISPENSER_VERSION}}" -bb rpm/dispenser.spec --noclean

# Usage: just bump 0.8.0
bump NEW_VERSION:
  @echo "Bumping version to {{NEW_VERSION}}..."
  # Update Cargo.toml
  sed -i '' 's/^version = ".*"/version = "{{NEW_VERSION}}"/' Cargo.toml
  # Update Documentation (URLs and filenames)
  sed -i '' -E 's/v[0-9]+\.[0-9]+\.[0-9]+/v{{NEW_VERSION}}/g' README.md INSTALL*.md
  sed -i '' -E 's/dispenser-[0-9]+\.[0-9]+/dispenser-{{NEW_VERSION}}/g' README.md INSTALL*.md
  @echo "Done. Don't forget to commit and tag!"
