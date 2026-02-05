BINDIR ?= $(HOME)/.local/bin
LIBDIR ?= $(HOME)/.local/lib
UNITDIR ?= $(HOME)/.config/systemd/user
SERVICE ?= whisp.service
CARGO ?= cargo
RPATH_FLAG ?= -C link-arg=-Wl,-rpath,\$$ORIGIN/../lib

.PHONY: build install install-bin install-lib install-service daemon-reload

build:
	RUSTFLAGS="$(RUSTFLAGS) $(RPATH_FLAG)" $(CARGO) build --release

install: build install-bin install-lib install-service daemon-reload

install-bin:
	install -Dm755 target/release/whisp $(BINDIR)/whisp

install-lib:
	install -Dm644 target/release/libsherpa-onnx-c-api.so $(LIBDIR)/libsherpa-onnx-c-api.so
	install -Dm644 target/release/libonnxruntime.so $(LIBDIR)/libonnxruntime.so

install-service:
	install -Dm644 systemd/user/$(SERVICE) $(UNITDIR)/$(SERVICE)

daemon-reload:
	@command -v systemctl >/dev/null 2>&1 && systemctl --user daemon-reload || true
