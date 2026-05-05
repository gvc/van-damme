INSTALL_DIR := $(HOME)/bin
BINARY := vd

.PHONY: install build

build:
	cargo build --release

install: build
	cp target/release/$(BINARY) $(INSTALL_DIR)/$(BINARY)
	codesign --force --deep --sign - $(INSTALL_DIR)/$(BINARY)
	@echo "Installed and signed $(INSTALL_DIR)/$(BINARY)"
