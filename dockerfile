FROM --platform=linux/amd64 ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Deps
RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    ca-certificates \
    python3 \
    pkg-config \
    libssl-dev \
    libwebkit2gtk-4.1-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    xdg-utils \
    file \
    && rm -rf /var/lib/apt/lists/*


# Node.js
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y nodejs \
    && rm -rf /var/lib/apt/lists/*


# Rust
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y


ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /app

CMD ["/bin/bash"]