PREFIX ?= /usr/local
BINDIR = $(PREFIX)/bin
KAKDIR = $(PREFIX)/share/kak/autoload/plugins/kakoune-scrollback

.PHONY: build install uninstall test test-kak test-all

build:
	cargo build --release

test:
	cargo test

test-kak:
	./test/kak_tests.sh

test-all: test test-kak

install: build
	install -Dm755 target/release/kakoune-scrollback $(DESTDIR)$(BINDIR)/kakoune-scrollback
	install -Dm644 rc/kakoune-scrollback.kak $(DESTDIR)$(KAKDIR)/kakoune-scrollback.kak

uninstall:
	rm -f $(DESTDIR)$(BINDIR)/kakoune-scrollback
	rm -rf $(DESTDIR)$(KAKDIR)
