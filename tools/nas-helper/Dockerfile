# Action Footage NAS Helper (tools/nas-helper/README.md): the guardian, the stream and the links for Jellyfin in one
# image, everything built here from the repository. Built from the repository's top:
#
#   docker build -f tools/nas-helper/Dockerfile --build-arg VERSION=0.13.0 -t actionfootage-nas-helper:0.13.0 .
#
# Fedora 43 with RPM Fusion's ffmpeg (HEVC, VA-API) and Intel's media driver; libplacebo 7.387 and mpv 0.42 (user
# shaders get the frame's PTS) built from their sources, as tools/gpu-spike's test image had them.

ARG FEDORA=43

# --- What the parts run on: ffmpeg, the GPU's drivers, Python, and what mpv and libplacebo link
FROM fedora:${FEDORA} AS base
ARG FEDORA
RUN dnf -y install https://mirrors.rpmfusion.org/free/fedora/rpmfusion-free-release-${FEDORA}.noarch.rpm \
        https://mirrors.rpmfusion.org/nonfree/fedora/rpmfusion-nonfree-release-${FEDORA}.noarch.rpm \
 && dnf -y swap ffmpeg-free ffmpeg --allowerasing \
 && dnf -y install --setopt=install_weak_deps=False intel-media-driver libva-utils mesa-vulkan-drivers mesa-va-drivers \
        vulkan-loader python3 vapoursynth-libs python3-vapoursynth libass luajit libdovi lcms2 xxhash-libs libshaderc glslang \
        libdisplay-info libwayland-client libwayland-cursor libwayland-egl libxkbcommon mesa-libEGL mesa-libgbm libdrm \
        uchardet pipewire-libs alsa-lib zlib libatomic procps-ng \
 && dnf clean all

# --- libplacebo and mpv
FROM base AS mpv
RUN dnf -y install --setopt=install_weak_deps=False meson ninja-build gcc gcc-c++ git pkgconf python3-jinja2 \
        python3-markupsafe glslang-devel libshaderc-devel vulkan-headers vulkan-loader-devel libdovi-devel lcms2-devel \
        xxhash-devel ffmpeg-devel libass-devel libva-devel wayland-devel wayland-protocols-devel libxkbcommon-devel \
        mesa-libEGL-devel mesa-libgbm-devel libdrm-devel luajit-devel zlib-devel libdisplay-info-devel vapoursynth-devel \
        uchardet-devel pipewire-devel alsa-lib-devel \
 && dnf clean all
RUN git clone --depth 1 --branch v7.387.0 --recurse-submodules https://code.videolan.org/videolan/libplacebo.git /src/libplacebo \
 && cd /src/libplacebo && meson setup build --prefix=/opt/pl --libdir=lib -Dbuildtype=release -Dtests=false -Ddemos=false \
 && ninja -C build install
RUN git clone --depth 1 --branch v0.42.0 https://github.com/mpv-player/mpv.git /src/mpv \
 && cd /src/mpv && PKG_CONFIG_PATH=/opt/pl/lib/pkgconfig meson setup build --prefix=/opt/mpv -Dbuildtype=release \
        -Dvapoursynth=enabled -Dwayland=enabled -Dvaapi=enabled -Dvulkan=enabled -Dlibmpv=false -Dx11=disabled \
 && ninja -C build install
# The renderer: VA-API decode, libplacebo, VA-API encode
COPY tools/stream/afp-render.c /src/
RUN cc -O2 /src/afp-render.c -o /opt/afp-render $(PKG_CONFIG_PATH=/opt/pl/lib/pkgconfig pkg-config --cflags --libs libplacebo \
        libavformat libavcodec libavutil libva libva-drm) -lm

# --- The Rust programs: the gyroflow plugin's binaries (the player asks them), gyroflow-gpu-table, afp-catalog
FROM base AS rust
RUN dnf -y install --setopt=install_weak_deps=False gcc gcc-c++ clang make cmake pkgconf perl git \
 && dnf clean all \
 && curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH=/root/.cargo/bin:$PATH
WORKDIR /src
# Dependencies first (a layer of their own: the sources change more often)
COPY vs-gyroflow/Cargo.toml vs-gyroflow/Cargo.lock vs-gyroflow/
COPY catalog/Cargo.toml catalog/Cargo.lock catalog/
RUN mkdir -p vs-gyroflow/src catalog/src && touch vs-gyroflow/src/lib.rs catalog/src/lib.rs && cd vs-gyroflow && cargo fetch --locked
COPY catalog catalog
COPY shaders shaders
COPY mpv mpv
COPY vs-gyroflow vs-gyroflow
RUN cd vs-gyroflow && cargo build --release --locked \
 && mkdir -p /out && for f in libvs_gyroflow.so gyroflow-probe gyroflow-project gyroflow-gpu-table gyroflow-export afp-cut afp-meta afp-catalog gyroflow-optical; do \
        install -m 755 target/release/$f /out/$f && strip --strip-unneeded /out/$f; done

# --- The player's plugins (the repository's Lua with the binaries above) and the shaders the stream renders with
FROM base AS files
COPY tools/cube_to_glsl.py /src/tools/
COPY shaders /src/shaders
COPY luts /src/luts
COPY mpv/scripts/workspace /opt/afp/player/workspace
COPY mpv/scripts/gyroflow /opt/afp/player/gyroflow
COPY mpv/scripts/dlogm /opt/afp/player/dlogm
COPY --from=rust /out/ /opt/afp/player/gyroflow/
RUN cd /src && mkdir -p /opt/afp/bin /opt/afp/player/dlogm/shaders \
 && mv /opt/afp/player/gyroflow/underwater.gyroflow /opt/afp/player/gyroflow/default.gyroflow \
 && python3 -I tools/cube_to_glsl.py shaders/dlogm_hlg.glsl.in luts/dlogm_hlg.cube /opt/afp/bin/dlogm_hlg.glsl \
 && cp /opt/afp/bin/dlogm_hlg.glsl /opt/afp/player/dlogm/shaders/ \
 && python3 -I tools/cube_to_glsl.py shaders/nbcu_sdr.glsl.in luts/3-NBCU_HLG2SDR_DL_v1.2.cube /opt/afp/player/dlogm/shaders/nbcu_hlg2sdr.glsl \
 && python3 -I tools/cube_to_glsl.py shaders/nbcu_sdr.glsl.in luts/5-NBCU_PQ2SDR_DL_v1.2.cube /opt/afp/player/dlogm/shaders/nbcu_pq2sdr.glsl \
 && ln -s ../player/gyroflow/gyroflow-gpu-table /opt/afp/bin/gyroflow-gpu-table \
 && ln -s ../player/gyroflow/afp-catalog /opt/afp/bin/afp-catalog

# --- The helper
FROM base
ARG VERSION=dev
COPY --from=mpv /opt/pl /opt/pl
COPY --from=mpv /opt/mpv /opt/mpv
COPY --from=files /opt/afp /opt/afp
COPY --from=mpv /opt/afp-render /opt/afp/bin/afp-render
COPY tools/stream/hlg_gain.glsl tools/stream/hlg_highlights.glsl /opt/afp/bin/
COPY tools/stream/afp_stream.py tools/stream/jellyfin.py tools/nas-helper/helper.py tools/nas-helper/nas_settings.py /opt/afp/helper/
ENV PATH=/opt/mpv/bin:$PATH LD_LIBRARY_PATH=/opt/pl/lib AFP_BIN=/opt/afp/bin AFP_PLAYER=/opt/afp/player PYTHONDONTWRITEBYTECODE=1
# Everything links (a library missing at run time would show only when a part starts)
# No machine id of its own: the guardian's name (kept in /data/catalog) stays the same from one image to the next
RUN echo "$VERSION" > /opt/afp/helper/VERSION && : > /etc/machine-id \
 && ! (cd /opt/afp/player/gyroflow && ldd /opt/mpv/bin/mpv /opt/afp/bin/afp-render libvs_gyroflow.so gyroflow-probe gyroflow-project \
        gyroflow-gpu-table gyroflow-export afp-cut afp-meta afp-catalog gyroflow-optical | grep "not found") \
 && mpv --version | head -1 && /opt/afp/bin/afp-catalog 2>&1 | head -1 && python3 /opt/afp/helper/helper.py --help > /dev/null
LABEL org.opencontainers.image.title="Action Footage NAS Helper" \
      org.opencontainers.image.source="https://github.com/anton-belkin/ActionFootagePlayer" \
      org.opencontainers.image.licenses="GPL-3.0-or-later" \
      org.opencontainers.image.version="$VERSION"
VOLUME ["/data", "/config"]
EXPOSE 8099
ENTRYPOINT ["python3", "-u", "/opt/afp/helper/helper.py"]
