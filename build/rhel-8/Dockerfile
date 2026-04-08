FROM rockylinux:8

RUN dnf install -y --allowerasing \
    curl \
    gcc \
    make \
    openssl-devel \
    pkg-config \
    rpm-build \
    && dnf clean all

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN curl --proto '=https' --tlsv1.2 -sSf https://just.systems/install.sh | bash -s -- --to /usr/local/bin

WORKDIR /build
