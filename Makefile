bin:
	cargo build --release
bin_bitmap_u64:
	cargo build --release --features=bitmap_u64
clean:
	cargo clean --release -p exfat-fuse
clean_all:
	cargo clean
fmt:
	cargo fmt
	git status
lint:
	cargo clippy --release --fix --all
	git status
plint:
	cargo clippy --release --fix --all -- -W clippy::pedantic
	git status
test:
	cargo test --release
test_debug:
	cargo test --release -- --nocapture
install:
	cargo install --path .
uninstall:
	cargo uninstall

xxx:	fmt lint test
