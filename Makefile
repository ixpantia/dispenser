.PHONY: build-deb build-rpm build

WORK_DIR=$(shell pwd)
TARGET_BIN=target/x86_64-unknown-linux-musl/release/dispenser
USR_BIN_DEB=deb/usr/local/bin/dispenser
USR_BIN_RPM=rpm/usr/local/bin/dispenser

build:
	CARGO_TARGET_DIR="./target" cargo build --release --target "x86_64-unknown-linux-musl"

build-deb: build
	mkdir -p deb/usr/local/bin/
	rm -f $(USR_BIN_DEB)
	mv $(TARGET_BIN) $(USR_BIN_DEB)
	dpkg-deb --build deb
	rm -f dispenser.deb
	mv deb.deb dispenser.deb

build-rpm: build
	mkdir -p deb/usr/local/bin/
	rm -f $(USR_BIN_RPM)
	mv $(TARGET_BIN) $(USR_BIN_RPM)
	cp -r rpm/ rpmbuild
	mkdir -p rpmbuild/opt/dispenser
	rpmbuild --target=x86_64 --buildroot $(WORK_DIR)/rpmbuild \
         -bb rpmbuild/dispenser.spec
