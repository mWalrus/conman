clean:
	cargo clean

install:
	cargo build --release
	mkdir -p /usr/local/bin
	install -Dm755 ./target/release/conman /usr/local/bin/conman

uninstall:
	rm -f /usr/local/bin/conman
