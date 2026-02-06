PREFIX ?= $(HOME)/.local
DESTDIR ?=
BINDIR ?= $(PREFIX)/bin
LIBDIR ?= $(PREFIX)/lib
XDG_CONFIG_HOME ?= $(HOME)/.config
XDG_CACHE_HOME ?= $(HOME)/.cache
SYSTEMD_USER_UNITDIR ?= $(XDG_CONFIG_HOME)/systemd/user
SYSTEMD_SYSTEM_UNITDIR ?= $(PREFIX)/lib/systemd/system
SERVICE ?= whisp.service
SERVICE_USER_FILE ?= systemd/user/$(SERVICE)
CARGO ?= cargo
RPATH_FLAG ?= -C link-arg=-Wl,-rpath,\$$ORIGIN/../lib
PURGE_CONFIG ?= 0
PURGE_CACHE ?= 0
MODEL_CACHE_DIR ?= $(XDG_CACHE_HOME)/huggingface/hub/models--csukuangfj--sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8

.PHONY: \
	build \
	install \
	install-user \
	install-bin \
	install-lib \
	install-user-service \
	install-system-service \
	uninstall \
	uninstall-user \
	uninstall-bin \
	uninstall-lib \
	uninstall-user-service \
	daemon-reload-user \
	purge

build:
	RUSTFLAGS="$(RUSTFLAGS) $(RPATH_FLAG)" $(CARGO) build --release

install: install-user

install-user: build install-bin install-lib install-user-service daemon-reload-user

install-bin:
	install -Dm755 target/release/whisp $(DESTDIR)$(BINDIR)/whisp

install-lib:
	install -Dm644 target/release/libsherpa-onnx-c-api.so $(DESTDIR)$(LIBDIR)/libsherpa-onnx-c-api.so
	install -Dm644 target/release/libonnxruntime.so $(DESTDIR)$(LIBDIR)/libonnxruntime.so

install-user-service:
	install -Dm644 $(SERVICE_USER_FILE) $(DESTDIR)$(SYSTEMD_USER_UNITDIR)/$(SERVICE)

install-system-service:
	@echo "System service install is not supported for desktop hotkey capture."
	@echo "Use the user service instead: make install-user"
	@exit 1

daemon-reload-user:
	@if [ -z "$(DESTDIR)" ] && command -v systemctl >/dev/null 2>&1; then \
		systemctl --user daemon-reload || true; \
	fi

uninstall: uninstall-user

uninstall-user: uninstall-user-service uninstall-bin uninstall-lib daemon-reload-user

uninstall-bin:
	rm -f $(DESTDIR)$(BINDIR)/whisp

uninstall-lib:
	rm -f $(DESTDIR)$(LIBDIR)/libsherpa-onnx-c-api.so
	rm -f $(DESTDIR)$(LIBDIR)/libonnxruntime.so

uninstall-user-service:
	@if [ -z "$(DESTDIR)" ] && command -v systemctl >/dev/null 2>&1; then \
		systemctl --user disable --now $(SERVICE) >/dev/null 2>&1 || true; \
	fi
	rm -f $(DESTDIR)$(SYSTEMD_USER_UNITDIR)/$(SERVICE)

purge:
	@if [ "$(PURGE_CONFIG)" = "1" ]; then \
		echo "Removing config directory: $(XDG_CONFIG_HOME)/whisp"; \
		rm -rf $(XDG_CONFIG_HOME)/whisp; \
	else \
		echo "Skipping config purge. Set PURGE_CONFIG=1 to remove $(XDG_CONFIG_HOME)/whisp"; \
	fi
	@if [ "$(PURGE_CACHE)" = "1" ]; then \
		echo "Removing model cache directory: $(MODEL_CACHE_DIR)"; \
		rm -rf $(MODEL_CACHE_DIR); \
	else \
		echo "Skipping model cache purge. Set PURGE_CACHE=1 to remove $(MODEL_CACHE_DIR)"; \
	fi
