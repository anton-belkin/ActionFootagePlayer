# mpv 0.42.0 (PTS for user shaders) on top of the test image, with libplacebo v7.387.0 (mpv 0.42 needs it; Fedora 43 has 7.351)
FROM afp-test:f43
RUN dnf -y install --skip-unavailable meson ninja-build gcc gcc-c++ git pkgconf python3-jinja2 python3-markupsafe glslang-devel libshaderc-devel \
    vulkan-headers vulkan-loader-devel libdovi-devel lcms2-devel xxhash-devel ffmpeg-devel libass-devel libva-devel \
    wayland-devel wayland-protocols-devel libxkbcommon-devel mesa-libEGL-devel mesa-libgbm-devel libdrm-devel luajit-devel zlib-devel \
    libdisplay-info-devel vapoursynth-devel uchardet-devel pipewire-devel alsa-lib-devel libplacebo-devel \
 && dnf clean all
RUN git clone --depth 1 --branch v7.387.0 --recurse-submodules https://code.videolan.org/videolan/libplacebo.git /src/libplacebo \
 && cd /src/libplacebo && meson setup build --prefix=/opt/pl --libdir=lib -Dbuildtype=release -Dtests=false -Ddemos=false \
 && ninja -C build install
RUN dnf -y install libatomic && dnf clean all
COPY mpv-0.42.0.tar.gz /src/
RUN cd /src && tar xzf mpv-0.42.0.tar.gz && cd mpv-0.42.0 \
 && PKG_CONFIG_PATH=/opt/pl/lib/pkgconfig meson setup build --prefix=/opt/mpv -Dbuildtype=release -Dvapoursynth=enabled -Dwayland=enabled -Dvaapi=enabled -Dvulkan=enabled -Dlibmpv=false -Dx11=disabled \
 && ninja -C build install
ENV LD_LIBRARY_PATH=/opt/pl/lib PATH=/opt/mpv/bin:$PATH
