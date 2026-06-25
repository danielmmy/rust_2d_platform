cargo:
	@cargo fmt
	@cargo clippy
	@cargo check

game-run: ## platformer: run the Bevy 2D Metroidvania (detached crate; needs a display)
	@cargo run
