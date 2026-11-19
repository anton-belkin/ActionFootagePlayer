// SPDX-License-Identifier: GPL-3.0-or-later
//
// afpchain: the player's GPU chain run offscreen through libplacebo, with no window (docs/gpu-render.md, "The upscale
// to the screen: FSR 1"): a clip's frames from stdin, each rendered with mpv user shaders (the Gyroflow warp from
// gyroflow-gpu-table --player, its PTS set per frame as mpv sets it; the look; fsr.glsl) into a target the size of
// the video's rectangle on screen. It prints each pass's GPU time per render (mean, p95, max), and the time from the
// render's start to the GPU done with it (WALL: with many passes MoltenVK reports some passes' time as 0, so their sum
// falls short), and can save frames.
//
//   ffmpeg -ss <t> -i clip.mp4 -frames:v <n> -pix_fmt yuv420p10le -f rawvideo - |
//     afpchain <w> <h> <fps> <t> <W> <H> <out prefix|-> <frames to save, e.g. 30,60|-> [name=value ...] -- shader.glsl ...
//
// AFP_RAW_OUT=<file or fifo>: every frame written there too (rgba64le), for clips (clip.sh)
//
// in: yuv420p10le, limited range, BT.709 matrix, taken as HLG/BT.2020 as the dlogm plugin tags log video for its hdr
// look; target: rgba64le, the same (the coded values pass as they are). name=value: a shader parameter (PTS is set per
// frame: t + i / fps). Renders before the 20th are left out of the timing (pipelines built, the denoise's ring filled).
//
// Build (macOS, Homebrew libplacebo + MoltenVK): cc -O2 -o afpchain afpchain.c $(pkg-config --cflags --libs libplacebo)

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <libplacebo/log.h>
#include <libplacebo/vulkan.h>
#include <libplacebo/renderer.h>
#include <libplacebo/shaders/custom.h>
#include <libplacebo/utils/upload.h>

#define MAXP 128
#define MAXR 4000
#define WARM 20
static int npass, pidx, ridx;
static char pdesc[MAXP][96];
static double ptime[MAXP][MAXR], wall[MAXR];
static double now_ms(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec * 1e3 + t.tv_nsec / 1e6; }

static void info_cb(void *priv, const struct pl_render_info *info) {
    (void) priv;
    if (info->stage != PL_RENDER_STAGE_FRAME || ridx < 0) return;
    // Grouped by the pass's description (the edge fill has several of each), summed per render
    const char *d = info->pass->shader->description ? info->pass->shader->description : "?";
    int i = 0;
    while (i < npass && strcmp(pdesc[i], d)) i++;
    if (i == MAXP) return;
    if (i == npass) snprintf(pdesc[npass++], sizeof pdesc[i], "%s", d);
    pidx++;
    if (ridx < MAXR) ptime[i][ridx] += info->pass->last / 1e6;
}

static int cmpd(const void *a, const void *b) { double x = *(const double *) a, y = *(const double *) b; return (x > y) - (x < y); }
static void stats(const char *name, const double *v, int n) {
    double s = 0, *c = malloc(n * sizeof *c);
    for (int i = 0; i < n; i++) s += v[i], c[i] = v[i];
    qsort(c, n, sizeof *c, cmpd);
    printf("  %-44s mean %6.2f  p95 %6.2f  max %6.2f ms\n", name, s / n, c[(int) (0.95 * (n - 1))], c[n - 1]);
    free(c);
}

static void *slurp(const char *path, size_t *len) {
    FILE *f = fopen(path, "rb");
    if (!f) { perror(path); exit(1); }
    fseek(f, 0, SEEK_END); long n = ftell(f); fseek(f, 0, SEEK_SET);
    char *p = malloc(n + 1);
    if (fread(p, 1, n, f) != (size_t) n) { perror(path); exit(1); }
    p[n] = 0; fclose(f); *len = n;
    return p;
}

static void set_par(const struct pl_hook **hooks, int nh, const char *name, double v) {
    for (int h = 0; h < nh; h++)
        for (int i = 0; i < hooks[h]->num_parameters; i++) {
            const struct pl_hook_par *p = &hooks[h]->parameters[i];
            if (strcmp(p->name, name)) continue;
            if (p->type == PL_VAR_FLOAT) p->data->f = (float) v;
            else if (p->type == PL_VAR_SINT) p->data->i = (int) v;
            else p->data->u = (unsigned) v;
        }
}

int main(int argc, char **argv) {
    if (argc < 10) { fprintf(stderr, "usage: afpchain w h fps t W H out|- frames|- [name=value ...] -- shader ...\n"); return 1; }
    int w = atoi(argv[1]), h = atoi(argv[2]), W = atoi(argv[5]), H = atoi(argv[6]);
    double fps = atof(argv[3]), t0 = atof(argv[4]);
    const char *out = argv[7], *save = argv[8];
    int a = 9;
    const char *pars[64]; int npars = 0;
    for (; a < argc && strcmp(argv[a], "--"); a++) pars[npars++] = argv[a];
    if (a >= argc) { fprintf(stderr, "no -- before the shaders\n"); return 1; }
    a++;

    pl_log log = pl_log_create(PL_API_VER, pl_log_params(.log_cb = pl_log_simple, .log_level = PL_LOG_WARN));
    pl_vulkan vk = pl_vulkan_create(log, pl_vulkan_params());
    if (!vk) { fprintf(stderr, "no Vulkan device\n"); return 1; }
    pl_gpu gpu = vk->gpu;

    const struct pl_hook *hooks[16]; int nh = 0;
    for (; a < argc; a++) {
        size_t len; char *text = slurp(argv[a], &len);
        hooks[nh] = pl_mpv_user_shader_parse(gpu, text, len);
        if (!hooks[nh]) { fprintf(stderr, "%s: the shader didn't parse\n", argv[a]); return 1; }
        nh++; free(text);
    }
    for (int i = 0; i < npars; i++) {
        char name[128]; const char *eq = strchr(pars[i], '=');
        if (!eq) continue;
        snprintf(name, sizeof name, "%.*s", (int) (eq - pars[i]), pars[i]);
        set_par(hooks, nh, name, atof(eq + 1));
    }

    struct pl_frame image = {
        .num_planes = 3,
        .repr = { .sys = PL_COLOR_SYSTEM_BT_709, .levels = PL_COLOR_LEVELS_LIMITED, .bits = { .sample_depth = 16, .color_depth = 10 } },
        .color = { .primaries = PL_COLOR_PRIM_BT_2020, .transfer = PL_COLOR_TRC_HLG },
    };
    pl_tex tex[3] = {0};

    pl_fmt fmt = pl_find_fmt(gpu, PL_FMT_UNORM, 4, 16, 16, PL_FMT_CAP_RENDERABLE | PL_FMT_CAP_HOST_READABLE);
    pl_tex target_tex = pl_tex_create(gpu, pl_tex_params(.w = W, .h = H, .format = fmt, .renderable = true, .host_readable = true));
    struct pl_frame target = {
        .num_planes = 1,
        .planes = {{ .texture = target_tex, .components = 4, .component_mapping = {0, 1, 2, 3} }},
        .repr = { .sys = PL_COLOR_SYSTEM_RGB, .levels = PL_COLOR_LEVELS_FULL, .bits = { .sample_depth = 16, .color_depth = 16 } },
        .color = image.color,
    };

    struct pl_render_params params = pl_render_default_params;
    params.upscaler = &pl_filter_lanczos;     // mpv's scale=lanczos
    params.downscaler = &pl_filter_hermite;   // mpv's dscale=hermite
    params.plane_upscaler = &pl_filter_bilinear;  // the GPU path's cscale (main.lua: bilinear in grid mode)
    params.dither_params = NULL;
    params.peak_detect_params = NULL;
    params.hooks = hooks; params.num_hooks = nh;
    params.info_callback = info_cb;

    size_t ysz = (size_t) w * h * 2, csz = (size_t) (w / 2) * (h / 2) * 2;
    uint8_t *buf = malloc(ysz + 2 * csz), *res = malloc((size_t) W * H * 8);
    pl_renderer rr = pl_renderer_create(log, gpu);
    int n = 0;
    for (;; n++) {
        if (fread(buf, 1, ysz + 2 * csz, stdin) != ysz + 2 * csz) break;
        for (int i = 0; i < 3; i++) {
            struct pl_plane_data data = {
                .type = PL_FMT_UNORM, .width = i ? w / 2 : w, .height = i ? h / 2 : h,
                .component_size = {16}, .component_map = {i}, .pixel_stride = 2, .row_stride = (size_t) (i ? w / 2 : w) * 2,
                .pixels = buf + (i == 0 ? 0 : ysz + (i - 1) * csz),
            };
            if (!pl_upload_plane(gpu, &image.planes[i], &tex[i], &data)) { fprintf(stderr, "upload failed\n"); return 1; }
        }
        pl_frame_set_chroma_location(&image, PL_CHROMA_LEFT);
        set_par(hooks, nh, "PTS", t0 + n / fps);
        pidx = 0; ridx = n - WARM;
        pl_gpu_finish(gpu);
        double t = now_ms();
        if (!pl_render_image(rr, &image, &target, &params)) { fprintf(stderr, "render failed\n"); return 1; }
        pl_gpu_finish(gpu);
        if (ridx >= 0 && ridx < MAXR) wall[ridx] = now_ms() - t;
        static FILE *raw; static int raw_init;
        if (!raw_init) { raw_init = 1; const char *rp = getenv("AFP_RAW_OUT"); if (rp) raw = fopen(rp, "wb"); }
        if (raw) {
            if (!pl_tex_download(gpu, pl_tex_transfer_params(.tex = target_tex, .ptr = res, .row_pitch = (size_t) W * 8))) { fprintf(stderr, "download failed\n"); return 1; }
            fwrite(res, 1, (size_t) W * H * 8, raw); fflush(raw);
        }
        char tag[16]; snprintf(tag, sizeof tag, ",%d,", n);
        char list[256]; snprintf(list, sizeof list, ",%s,", save);
        if (strcmp(out, "-") && strstr(list, tag)) {
            if (!pl_tex_download(gpu, pl_tex_transfer_params(.tex = target_tex, .ptr = res, .row_pitch = (size_t) W * 8))) { fprintf(stderr, "download failed\n"); return 1; }
            char path[1024]; snprintf(path, sizeof path, "%s%d.rgba", out, n);
            FILE *f = fopen(path, "wb");
            if (!f || fwrite(res, 1, (size_t) W * H * 8, f) != (size_t) W * H * 8) { perror(path); return 1; }
            fclose(f);
        }
    }
    int m = n - WARM;
    if (m > MAXR) m = MAXR;
    if (m > 0) {
        static double tot[MAXR];
        printf("%d renders timed (%dx%d -> %dx%d)\n", m, w, h, W, H);
        for (int i = 0; i < npass; i++) { stats(pdesc[i], ptime[i], m); for (int r = 0; r < m; r++) tot[r] += ptime[i][r]; }
        stats("TOTAL (the passes' sum per render)", tot, m);
        stats("WALL (render to GPU done, per render)", wall, m);
    }
    if (pl_renderer_get_errors(rr).errors) fprintf(stderr, "renderer reported errors: 0x%x\n", (unsigned) pl_renderer_get_errors(rr).errors);
    pl_renderer_destroy(&rr);
    for (int i = 0; i < nh; i++) pl_mpv_user_shader_destroy(&hooks[i]);
    for (int i = 0; i < 3; i++) pl_tex_destroy(gpu, &tex[i]);
    pl_tex_destroy(gpu, &target_tex);
    pl_vulkan_destroy(&vk);
    pl_log_destroy(&log);
    return 0;
}
