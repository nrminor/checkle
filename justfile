export CARGO_TERM_COLOR := "always"

all: rustup-show check fmt clippy test sort audit

rustup-show:
  rustup show

check:
  cargo check --all-targets

fmt:
  cargo fmt --all -- --check

clippy:
  cargo clippy --all-targets

test:
  cargo insta test --workspace

sort:
  cargo sort --check --workspace

audit:
  cargo audit

install:
  cargo install --locked cargo-sort cargo-audit
