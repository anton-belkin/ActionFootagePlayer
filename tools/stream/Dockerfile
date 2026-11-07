# The streaming service: the mpv 0.42 test image, plus Rust (gyroflow-gpu-table) and the renderer's build dependencies
FROM afp-test:f43-mpv042
RUN dnf -y install --skip-unavailable rust cargo clang libva-devel python3 jq && dnf clean all
