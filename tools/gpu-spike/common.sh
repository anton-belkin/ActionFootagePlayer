# Inside the container (afp-test:f43-mpv042): headless weston 1920x1080, the clip copied to RAM, mpv 0.42 without config
export XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p $XDG_RUNTIME_DIR; chmod 700 $XDG_RUNTIME_DIR; export TMPDIR=/tmp
[ -f /ram/c.mp4 ] || cp /clips/DJI_20261020130432_0126_D.MP4 /ram/c.mp4
[ -f /ram/c.gyroflow ] || sed 's#"videofile": *"[^"]*"#"videofile": "/ram/c.mp4"#' /clips/DJI_20261020130432_0126_D.gyroflow > /ram/c.gyroflow
C=/ram/c.mp4
if ! [ -S $XDG_RUNTIME_DIR/wl0 ]; then
  weston --backend=headless --renderer=gl --width=1920 --height=1080 --socket=wl0 --idle-time=0 >/dev/null 2>&1 &
  sleep 2
fi
export WAYLAND_DISPLAY=wl0
BASE="--no-config --vo=gpu-next --gpu-api=vulkan --gpu-context=waylandvk --ao=null --fs --hr-seek=yes --input-ipc-server=/tmp/t.sock"
UD='{"workers":3,"lens":"stabilized","interpolation":"bicubic","project":"/ram/c.gyroflow","level":"medium","plugin":"/cfg/scripts/gyroflow/libvs_gyroflow.so","lookahead":4,"width":1920,"height":1080,"edges":"off","fast":false}'
VS="vapoursynth=file=/cfg/scripts/gyroflow/gyroflow.vpy:concurrent-frames=4:user-data=%${#UD}%${UD}"
q() { echo "{\"command\":[\"get_property\",\"$1\"]}" | socat - /tmp/t.sock 2>/dev/null | jq -c 'select(has("error")) | .data' | head -1; }
cmd() { echo "$1" | socat - /tmp/t.sock >/dev/null 2>&1; }
