// SPDX-License-Identifier: GPL-3.0-or-later
//
// The GPU path's stand-in while a clip's warp is written (main.lua): the clip opens meanwhile, paused, and shows black
// rather than its unstabilized frame; the warp takes this one's place in mpv's list

//!HOOK MAIN
//!BIND HOOKED
//!DESC Gyroflow: the warp on its way

vec4 hook() {
    return vec4(0.0, 0.0, 0.0, 1.0);
}
