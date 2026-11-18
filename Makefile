CARGO ?= cargo
DEV_TOOLS_DIR := $(CURDIR)/target/dev-tools
CARGO_WATCH := $(DEV_TOOLS_DIR)/bin/cargo-watch

.PHONY: dev run build

$(CARGO_WATCH):
	$(CARGO) install cargo-watch --version 8.5.3 --locked --root "$(DEV_TOOLS_DIR)"

dev: $(CARGO_WATCH)
	"$(CARGO_WATCH)" -x 'run -p lure-desktop'

run:
	$(CARGO) run -p lure-desktop

build:
	$(CARGO) build --release -p lure-desktop
