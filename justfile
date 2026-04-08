# justfile for dispenser project

DISPENSER_VERSION := shell('grep "^version" Cargo.toml | head -n1 | cut -d \" -f 2')
TARGET_BIN := "target/x86_64-unknown-linux-gnu/release/dispenser"
USR_BIN_RPM := "rpm/usr/local/bin/dispenser"

version:
  echo "{{DISPENSER_VERSION}}"

# Default build (static glibc)
build:
  RUSTFLAGS="-C target-feature=+crt-static" CARGO_TARGET_DIR="./target" cargo build --release --target "x86_64-unknown-linux-gnu"

# Dynamic build (links against system glibc)
build-dynamic:
  CARGO_TARGET_DIR="./target" cargo build --release --target "x86_64-unknown-linux-gnu"

# Dockerized builds for specific OS
build-in-docker OS:
  docker build -t dispenser-build-{{OS}} -f build/{{OS}}/Dockerfile .
  docker run --rm -v $(pwd):/build dispenser-build-{{OS}} just build-dynamic

build-deb OS_NAME="":
  rm -rf target/deb_stage
  mkdir -p target/deb_stage
  cp -R deb/* target/deb_stage/
  mkdir -p target/deb_stage/usr/local/bin
  cp {{TARGET_BIN}} target/deb_stage/usr/local/bin/dispenser
  sed 's/VERSION_PLACEHOLDER/{{DISPENSER_VERSION}}/' deb/DEBIAN/control > target/deb_stage/DEBIAN/control
  if [ -n "{{OS_NAME}}" ]; then \
    dpkg-deb --build target/deb_stage "dispenser-{{DISPENSER_VERSION}}-0-{{OS_NAME}}.x86_64.deb"; \
  else \
    dpkg-deb --build target/deb_stage "dispenser-{{DISPENSER_VERSION}}-0.x86_64.deb"; \
  fi

build-rpm OS_NAME="":
  rm -rf rpmstage rpmout
  mkdir -p rpmstage/usr/local/bin
  mkdir -p rpmstage/usr/lib/systemd/system
  mkdir -p rpmstage/opt/dispenser
  mkdir -p rpmout
  cp {{TARGET_BIN}} rpmstage/usr/local/bin/dispenser
  cp rpm/usr/lib/systemd/system/dispenser.service rpmstage/usr/lib/systemd/system/
  rpmbuild --target=x86_64 --buildroot $(pwd)/rpmstage --define "_topdir $(pwd)/rpmout" --define "version {{DISPENSER_VERSION}}" -bb rpm/dispenser.spec --noclean
  if [ -n "{{OS_NAME}}" ]; then \
    mv rpmout/RPMS/x86_64/dispenser-{{DISPENSER_VERSION}}-0.x86_64.rpm "rpmout/RPMS/x86_64/dispenser-{{DISPENSER_VERSION}}-0.{{OS_NAME}}.x86_64.rpm"; \
  fi

# Matrix build recipes
build-debian-12:
  docker build -t dispenser-build-debian-12 -f build/debian-12/Dockerfile .
  docker run --rm -v $(pwd):/build dispenser-build-debian-12 bash -c "just build-dynamic && just build-deb debian-12"

build-debian-13:
  docker build -t dispenser-build-debian-13 -f build/debian-13/Dockerfile .
  docker run --rm -v $(pwd):/build dispenser-build-debian-13 bash -c "just build-dynamic && just build-deb debian-13"

build-ubuntu-24:
  docker build -t dispenser-build-ubuntu-24 -f build/ubuntu-24/Dockerfile .
  docker run --rm -v $(pwd):/build dispenser-build-ubuntu-24 bash -c "just build-dynamic && just build-deb ubuntu-24"

build-rhel-8:
  docker build -t dispenser-build-rhel-8 -f build/rhel-8/Dockerfile .
  docker run --rm -v $(pwd):/build dispenser-build-rhel-8 bash -c "just build-dynamic && just build-rpm rhel-8"

build-rhel-9:
  docker build -t dispenser-build-rhel-9 -f build/rhel-9/Dockerfile .
  docker run --rm -v $(pwd):/build dispenser-build-rhel-9 bash -c "just build-dynamic && just build-rpm rhel-9"

# Usage: just bump 0.8.0
bump NEW_VERSION:
  @echo "Bumping version to {{NEW_VERSION}}..."
  # Update Cargo.toml
  sed -i '' 's/^version = ".*"/version = "{{NEW_VERSION}}"/' Cargo.toml
  # Update Documentation (URLs and filenames)
  sed -i '' -E 's/v[0-9]+\.[0-9]+\.[0-9]+/v{{NEW_VERSION}}/g' README.md INSTALL*.md
  sed -i '' -E 's/dispenser-[0-9]+\.[0-9]+\.[0-9]+/dispenser-{{NEW_VERSION}}/g' README.md INSTALL*.md
  @echo "Done. Don't forget to commit and tag!"

mem_prof:
  docker rm -f nginx-service-1 nginx-service-2 hello-world-job
  cd example && MALLOC_CONF=prof:true,lg_prof_sample:0 _RJEM_MALLOC_CONF=prof:true,lg_prof_sample:0 cargo run --release --manifest-path ../Cargo.toml
