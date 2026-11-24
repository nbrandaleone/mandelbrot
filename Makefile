# =============================================================================
# Makefile for the Rust / WebAssembly Mandelbrot Set Viewer
# =============================================================================
# Run `make help` to see all available commands.
# Common workflow:
#   make test   - Run Rust unit tests
#   make run    - Build WASM (if needed) and start the local web server
#   make stop   - Stop the background web server
# =============================================================================

# Configurable settings (you can override these, e.g. `make run PORT=9000`)
PORT          ?= 8080
PID_FILE      := .server.pid
WASM_OUT      := www/pkg
WASM_ARTIFACT := $(WASM_OUT)/mandelbrot_bg.wasm

# Declare targets that do not correspond to real files on disk
.PHONY: help build run stop test clean

# Default target when running `make` with no arguments
.DEFAULT_GOAL := help

## help: Show this list of available targets
help:
	@echo "Rust/WASM Mandelbrot Set Viewer — Available Make Targets:"
	@echo ""
	@echo "  make build   Compile the Rust crate to WebAssembly into $(WASM_OUT)/"
	@echo "  make run     Build WASM if needed and start the local server on port $(PORT)"
	@echo "  make stop    Stop the background local server and clean up $(PID_FILE)"
	@echo "  make test    Run the Rust unit test suite (cargo test)"
	@echo "  make clean   Stop any running server and remove build artifacts"

# File rule: automatically rebuild the WASM package when Rust sources change
$(WASM_ARTIFACT): Cargo.toml $(wildcard src/*.rs)
	@echo "Building Rust/WASM module into $(WASM_OUT)/..."
	wasm-pack build --target web --out-dir $(WASM_OUT)

## build: Force-build the Rust/WASM module into www/pkg/
build:
	@echo "Building Rust/WASM module into $(WASM_OUT)/..."
	wasm-pack build --target web --out-dir $(WASM_OUT)

## run: Build WASM if needed and launch a local HTTP server in the background
run: $(WASM_ARTIFACT)
	@if [ -f "$(PID_FILE)" ]; then \
		pid=$$(cat "$(PID_FILE)"); \
		if kill -0 "$$pid" 2>/dev/null; then \
			echo "Server is already running (PID $$pid) at http://localhost:$(PORT)"; \
			exit 0; \
		else \
			rm -f "$(PID_FILE)"; \
		fi; \
	fi; \
	echo "Starting local web server for www/ on port $(PORT)..."; \
	python3 -m http.server $(PORT) --directory www </dev/null >/dev/null 2>&1 & \
	pid=$$!; \
	echo "$$pid" > "$(PID_FILE)"; \
	sleep 0.3; \
	if kill -0 "$$pid" 2>/dev/null; then \
		echo "Mandelbrot Viewer running at http://localhost:$(PORT) (PID $$pid)"; \
		echo "Run 'make stop' to shut down the server."; \
	else \
		rm -f "$(PID_FILE)"; \
		echo "Error: failed to start server on port $(PORT) (is the port already in use?)"; \
		exit 1; \
	fi

## stop: Cleanly stop the background HTTP server using the saved PID file
stop:
	@if [ ! -f "$(PID_FILE)" ]; then \
		echo "Server is not running (no $(PID_FILE) found)."; \
		exit 0; \
	fi; \
	pid=$$(cat "$(PID_FILE)"); \
	if kill -0 "$$pid" 2>/dev/null; then \
		echo "Stopping server (PID $$pid)..."; \
		kill "$$pid" 2>/dev/null || true; \
		for i in 1 2 3 4 5; do \
			kill -0 "$$pid" 2>/dev/null || break; \
			sleep 0.2; \
		done; \
		if kill -0 "$$pid" 2>/dev/null; then \
			kill -9 "$$pid" 2>/dev/null || true; \
		fi; \
		echo "Server stopped."; \
	else \
		echo "Server (PID $$pid) was not running; cleaning up stale $(PID_FILE)."; \
	fi; \
	rm -f "$(PID_FILE)"

## test: Run Rust unit tests
test:
	@echo "Running Rust unit tests..."
	cargo test

## clean: Stop server if running and remove generated WASM & Cargo artifacts
clean: stop
	@echo "Cleaning build artifacts..."
	rm -rf "$(WASM_OUT)" "$(PID_FILE)"
	cargo clean
