.PHONY: build-deb build-rpm

WORK_DIR=$(shell pwd)
TARGET_BIN=target/x86_64-unknown-linux-musl/release/dispenser
USR_BIN_DEB=deb/usr/local/bin/dispenser
USR_BIN_RPM=rpm/usr/local/bin/dispenser

$(TARGET_BIN):
	CARGO_TARGET_DIR="./target" cargo build --release --target "x86_64-unknown-linux-musl"

$(USR_BIN_RPM): $(TARGET_BIN)
	mkdir -p deb/usr/local/bin/
	mv $(TARGET_BIN) $(USR_BIN_RPM)

$(USR_BIN_DEB): $(TARGET_BIN)
	mkdir -p deb/usr/local/bin/
	mv $(TARGET_BIN) $(USR_BIN_DEB)

build-deb: $(USR_BIN_DEB) 
	dpkg-deb --build deb
	mv deb.deb dispenser.deb

build-rpm: $(USR_BIN_RPM) 
	cp -r rpm/ rpmbuild
	mkdir -p rpmbuild/opt/dispenser
	rpmbuild --target=x86_64 --buildroot $(WORK_DIR)/rpmbuild \
         -bb rpmbuild/dispenser.spec
