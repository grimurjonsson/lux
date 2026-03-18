test:
  cargo test

build-install:
  cargo build --release
  cp target/release/lux ~/.local/bin

install-completions:
  mkdir -p ~/.zsh/completions
  cargo run -- completions zsh > ~/.zsh/completions/_lux
  @echo "Done. Make sure ~/.zsh/completions is in your fpath."
  @echo "Add to ~/.zshrc if not already:  fpath=(~/.zsh/completions \$fpath)"
