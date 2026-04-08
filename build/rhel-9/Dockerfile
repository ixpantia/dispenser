FROM rockylinux:9

RUN dnf install -y \
    curl \
    gcc \
    make \
    openssl-devel \
    pkg-config \
    rpm-build \
    && dnf clean all

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

WORKDIR /build
