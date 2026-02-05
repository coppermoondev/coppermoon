# CopperMoon — Build & Release
# Usage:
#   make build                    Debug build
#   make release                  Release build
#   make archive                  Release build + create distributable archive
#   make publish VERSION=0.1.0    Tag + push → triggers GitHub Actions multi-platform build & release
#   make clean                    Remove build artifacts

VERSION   ?= 0.1.0
REPO      := coppermoondev/coppermoon
BINARIES  := coppermoon harbor shipyard

# Detect current platform
UNAME_S   := $(shell uname -s 2>/dev/null || echo Windows)
UNAME_M   := $(shell uname -m 2>/dev/null || echo x86_64)

ifeq ($(UNAME_S),Linux)
  HOST_TARGET := $(UNAME_M)-unknown-linux-gnu
  EXT        :=
  ARCHIVE_EXT := tar.gz
endif
ifeq ($(UNAME_S),Darwin)
  HOST_TARGET := $(UNAME_M)-apple-darwin
  EXT        :=
  ARCHIVE_EXT := tar.gz
endif
ifeq ($(UNAME_S),Windows)
  HOST_TARGET := x86_64-pc-windows-msvc
  EXT        := .exe
  ARCHIVE_EXT := zip
endif
ifneq (,$(findstring MINGW,$(UNAME_S)))
  HOST_TARGET := x86_64-pc-windows-msvc
  EXT        := .exe
  ARCHIVE_EXT := zip
endif
ifneq (,$(findstring MSYS,$(UNAME_S)))
  HOST_TARGET := x86_64-pc-windows-msvc
  EXT        := .exe
  ARCHIVE_EXT := zip
endif

TARGET    ?= $(HOST_TARGET)
DIST_DIR  := dist
ARCHIVE   := coppermoon-$(TARGET)

# ─── Build ────────────────────────────────────────────────────────────

.PHONY: build release archive clean publish tag help

build:
	cargo build

release:
	cargo build --release

# ─── Archive (local platform only) ───────────────────────────────────

archive: release
	@echo "Creating archive for $(TARGET)..."
	@mkdir -p $(DIST_DIR)/$(ARCHIVE)
	@for bin in $(BINARIES); do \
		cp target/release/$$bin$(EXT) $(DIST_DIR)/$(ARCHIVE)/$$bin$(EXT) 2>/dev/null || true; \
	done
ifeq ($(ARCHIVE_EXT),zip)
	@cd $(DIST_DIR) && powershell -Command "Compress-Archive -Path '$(ARCHIVE)/*' -DestinationPath '$(ARCHIVE).zip' -Force" 2>/dev/null || \
		(cd $(DIST_DIR) && zip -r $(ARCHIVE).zip $(ARCHIVE))
else
	@cd $(DIST_DIR) && tar czf $(ARCHIVE).tar.gz $(ARCHIVE)
endif
	@rm -rf $(DIST_DIR)/$(ARCHIVE)
	@echo "Done: $(DIST_DIR)/$(ARCHIVE).$(ARCHIVE_EXT)"

# ─── Publish ─────────────────────────────────────────────────────────
# Creates a git tag and pushes it. GitHub Actions handles:
#   - Building for all 5 targets (Linux x86/arm, macOS x86/arm, Windows)
#   - Creating the GitHub Release with all archives
#
# Usage: make publish VERSION=0.1.0

publish:
ifndef VERSION
	$(error VERSION is required. Usage: make publish VERSION=0.1.0)
endif
	@echo "Publishing CopperMoon v$(VERSION)..."
	@echo ""
	@echo "  1. Creating tag v$(VERSION)"
	git tag -a v$(VERSION) -m "Release v$(VERSION)"
	@echo "  2. Pushing tag to origin"
	git push origin v$(VERSION)
	@echo ""
	@echo "Done! GitHub Actions will now build and create the release."
	@echo "  -> https://github.com/$(REPO)/actions"
	@echo "  -> https://github.com/$(REPO)/releases/tag/v$(VERSION)"

# ─── Clean ────────────────────────────────────────────────────────────

clean:
	cargo clean
	rm -rf $(DIST_DIR)

# ─── Help ─────────────────────────────────────────────────────────────

help:
	@echo "CopperMoon Build System"
	@echo ""
	@echo "  make build                    Debug build"
	@echo "  make release                  Release build"
	@echo "  make archive                  Release build + archive for current platform"
	@echo "  make publish VERSION=x.y.z    Tag + push (triggers GitHub Actions release)"
	@echo "  make clean                    Remove build artifacts"
	@echo ""
	@echo "The 'publish' command creates a git tag and pushes it."
	@echo "GitHub Actions then builds for all platforms automatically:"
	@echo "  - x86_64-unknown-linux-gnu"
	@echo "  - aarch64-unknown-linux-gnu"
	@echo "  - x86_64-apple-darwin"
	@echo "  - aarch64-apple-darwin"
	@echo "  - x86_64-pc-windows-msvc"
	@echo ""
	@echo "Examples:"
	@echo "  make release                  Build release locally"
	@echo "  make archive                  Build + create .zip/.tar.gz"
	@echo "  make publish VERSION=0.1.0    Release to GitHub"
