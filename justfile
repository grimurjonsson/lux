test:
  cargo test

build-install:
  cargo build --release
  cp target/release/ctail ~/.local/bin

install-completions:
  mkdir -p ~/.zsh/completions
  cargo run -- completions zsh > ~/.zsh/completions/_ctail
  @echo "Done. Make sure ~/.zsh/completions is in your fpath."
  @echo "Add to ~/.zshrc if not already:  fpath=(~/.zsh/completions \$fpath)"
