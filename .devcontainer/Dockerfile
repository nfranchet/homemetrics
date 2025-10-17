FROM mcr.microsoft.com/devcontainers/rust:1-1-bookworm

RUN sudo apt update && \
    sudo apt install -y tig zsh-antigen && \
    cargo install grcov  && \
    rustup component add llvm-tools-preview

# Fix cargo cache permissions
RUN sudo chown -R vscode:vscode /usr/local/cargo