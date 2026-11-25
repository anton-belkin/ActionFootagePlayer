#!/usr/bin/env python3
"""The website's player screenshots, taken from the installed app over mpv's IPC (website/site/media/).

    python3 -I website/shots/player_shots.py <out-dir> [clip]

Needs the Mac's display awake and unlocked (mpv's gpu-next window hangs otherwise) and the built-in Retina display
online: the window opens there at 1280x720 points (2560x1440 px) and mpv's own `screenshot-to-file ... window` is taken
(the video with the side panel and the navigation bar as drawn; no macOS title bar, no screen-recording permission
needed). The look is switched to NBCU's SDR for this session only (`script-message-to dlogm look nbcu`): a window
screenshot of the HDR output looks washed out.

It only opens a clip, changes panel tabs and moves the pointer: no Save, no k/M/X, no moments opened, no measuring, so
nothing is written beside the clip or into the project. (Opening a clip in a project does add lines to the project's
shared catalog journal, .afp/catalog/journal/<this computer>/: what the player read.)

Made 2026-11-11 from the 0.11 player on Anton's Thistlegorm clip (DJI_20261024094541_0093_D, 3 marks, 2 moments):
player-marks.jpg, library-2.jpg, library-6.jpg, photos-5.jpg, stabilization-7.jpg.

Still to take, by hand (they need a write, a dialog, or the Library window, which only screencapture can shoot):
- library-1.jpg, the Project tab's "No project yet" (its text changed): a clip outside any project, e.g. a copy on the
  desktop; the panel on Project, the folder list open. Dropped from the page until then.
- library-3.jpg, the moment-name prompt: the rail gained Playlist. l, l, k on a clip of a test project, then Esc.
- library-4.jpg, Transform while a moment plays, changed for viewing: open a moment of a test project, Ctrl+k, shot,
  Revert to saved.
- library-5.jpg, the Ctrl+x question: only the overlay; still right, retake with the new bar for consistency, Esc.
- stabilization-2.jpg (Transform tab, whole) and stabilization-4.jpg (with a Gyroflow project): the tab now also has
  Temporal denoise and Sideways jerks below Water; take them taller (window 1280x960 pt) and stitch two scrolls.
- photos-1.jpg (Save frame over the reef): the rail gained Playlist, the bar changed. photos-2.jpg (denoise off/on
  at 100 %): from 0.10's denoise; 0.11 takes out about twice as much: retake with Ctrl+p off/on on the same paused
  frame, Full.
- log-3.jpg (Output › Export with a part marked): Export is now one button with a list of kinds (as it plays, SDR,
  360°, flat); mark 1:05–1:30 with l, l, shot, Clear the part.
- Looked unchanged, not re-checked against the app: log-2/4/5 (Dolby Vision and Output's Display), photos-4 (Save
  frame panel), stabilization-5 (Transform's top without motion data), stabilization-6 (the "can't keep up" list).
- The Library window (organizing.html has HTML comments where they go): library-window.jpg (Footage view, a trip's
  folder in the grid, Filters open, Commands), library-info.jpg (Info on one clip: map, place, Lens profile),
  library-optical.jpg (Commands > Stabilize optically on Action 6 clips shot with EIS on). In Anton's library, avoid
  the folders and moments named after people ("Mountains with Liza", "Maks diving close up", "sasha dives").
"""
import json, os, socket, subprocess, sys, time

APP = os.path.expanduser("~/Applications/ActionFootagePlayer.app/Contents/MacOS/ActionFootagePlayer")
SOCK = "/tmp/afp-site.sock"
CLIP = "/Volumes/photo/DATA/DJI_Inbox/DJI Device/OsmoAction6-4E8C/20261024/DJI_20261024094541_0093_D.MP4"
PANEL_X = 1880          # the panel's left edge in the 2560 px window (rail + panel = 340 pt)


class Mpv:
    def __init__(self, path, *args):
        if os.path.exists(SOCK):
            os.unlink(SOCK)
        self.p = subprocess.Popen([APP, "--input-ipc-server=" + SOCK, "--screen-name=Built-in Retina Display",
                                   "--geometry=2560x1440", "--pause", *args, path],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        for _ in range(200):
            if os.path.exists(SOCK):
                break
            time.sleep(0.1)
        self.s = socket.socket(socket.AF_UNIX)
        self.s.connect(SOCK)
        self.f = self.s.makefile("rb")
        self.n = 0

    def cmd(self, *c):
        self.n += 1
        self.s.sendall((json.dumps({"command": list(c), "request_id": self.n}) + "\n").encode())
        while True:
            m = json.loads(self.f.readline())
            if m.get("request_id") == self.n:
                return m

    def get(self, prop):
        return self.cmd("get_property", prop).get("data")

    def wait_loaded(self, timeout=60):
        t = time.time()
        while time.time() - t < timeout:
            if self.get("video-params/w") and self.get("time-pos") is not None:
                return True
            time.sleep(0.5)
        return False

    def shot(self, path, settle=3.0):
        time.sleep(settle)
        print(path, self.cmd("screenshot-to-file", path, "window").get("error"))

    def quit(self):
        try:
            self.cmd("quit")
        except Exception:
            pass
        self.p.wait(timeout=20)


def tab(m, name):
    m.cmd("script-message-to", "dlogm", "look", "nbcu")
    m.cmd("script-message-to", "side_panel", "tab", name)


def scroll(m, n):
    m.cmd("mouse", 2200, 700)
    time.sleep(0.5)
    for _ in range(n):
        m.cmd("keypress", "WHEEL_DOWN")
        time.sleep(0.05)
    m.cmd("mouse", 1000, 500)


def main(out, clip):
    os.makedirs(out, exist_ok=True)
    raw = lambda n: os.path.join(out, n + "-raw.png")
    m = Mpv(clip, "--start=21")
    m.wait_loaded()
    time.sleep(8)                       # "Preparing the first frame…" and the look's message gone
    tab(m, "project")
    m.cmd("mouse", 1127, 1349)          # over the second camera highlight on the track: its bubble
    m.shot(raw("project-hover"), 3)
    m.cmd("mouse", 1000, 500)
    m.shot(raw("project"), 2.5)
    tab(m, "transform")
    scroll(m, 40)
    m.shot(raw("transform-end"), 4)
    tab(m, "quality")
    m.shot(raw("quality"), 4)
    m.quit()

    from PIL import Image               # pip install pillow
    def save(im, name):
        im.convert("RGB").save(os.path.join(out, name), "JPEG", quality=85, optimize=True, progressive=True)
        print(name, im.size)
    h, p = Image.open(raw("project-hover")), Image.open(raw("project"))
    save(h.resize((1600, 900), Image.LANCZOS), "player-marks.jpg")
    save(h.crop((0, 1140, 1880, 1440)).resize((1600, 255), Image.LANCZOS), "library-2.jpg")
    save(p.crop((PANEL_X, 40, PANEL_X + 584, 1380)), "library-6.jpg")
    save(Image.open(raw("quality")).crop((PANEL_X, 725, PANEL_X + 584, 1210)), "photos-5.jpg")
    save(Image.open(raw("transform-end")).crop((PANEL_X, 545, PANEL_X + 584, 1420)), "stabilization-7.jpg")
    print("Check the crops: the panel's scroll and the pointer's bubble move with the clip and the settings.")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else CLIP)
