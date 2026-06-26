cargo:
	@cargo fmt
	@cargo clippy
	@cargo check

game-run: ## platformer: run the Bevy 2D Metroidvania (detached crate; needs a display)
	@cargo run

release: ## build an optimised, self-contained native binary -> target/release/wisp
	@cargo build --release
	@echo "built target/release/wisp"

# Cross-compile a Windows .exe from Linux via the MinGW-w64 toolchain.
WIN_TARGET := x86_64-pc-windows-gnu

release-windows: ## cross-compile a self-contained Windows .exe (needs mingw-w64 installed)
	@rustup target add $(WIN_TARGET)
	@cargo build --release --target $(WIN_TARGET)
	@echo "built target/$(WIN_TARGET)/release/wisp.exe"
