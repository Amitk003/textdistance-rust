FROM rust:1.97-slim AS builder
RUN apt-get update \
    && apt-get install -y --no-install-recommends python3 python3-venv python3-pip \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN python3 -m venv /opt/venv \
    && /opt/venv/bin/pip install --quiet --upgrade pip \
    && /opt/venv/bin/pip install --quiet "maturin>=1.4,<2.0" pytest hypothesis \
    && /opt/venv/bin/maturin develop --release

FROM rust:1.97-slim
COPY --from=builder /opt/venv /opt/venv
COPY --from=builder /src /src
WORKDIR /src
ENV PATH="/opt/venv/bin:$PATH"
CMD ["pytest", "-m", "not external", "tests/original"]
