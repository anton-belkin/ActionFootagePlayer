// SPDX-License-Identifier: GPL-3.0-or-later
//
// pipebench: how long libplacebo spends building shader pipelines for the player's shader lists, with no window
// (docs/gpu-render.md, "Pipeline builds"). One renderer, as one player process has one, renders a few frames with each
// phase's list of mpv user shaders in turn (a phase: the list after a change, e.g. underwater colour turned on), and
// prints per phase what libplacebo logged: how many SPIR-V compiles (glslang), pipelines created (MoltenVK: SPIR-V to
// MSL, Metal's compile), re-specializations, and their milliseconds; and the first render's wall time.
//
//   pipebench [name=value ...] -- a.glsl b.glsl -- a.glsl b.glsl c.glsl ...   (src=709|hlg in a phase: the video's
//   color space from then on, as the underwater colour switches it)
//
// A shader listed again in a later phase is kept as parsed, as mpv keeps it (by its path); one dropped is destroyed
// (mpv-patches 0015). name=value: a shader parameter, in every phase.
//
// Environment: PB_CACHE=<dir>/<prefix> libplacebo's cache on disk, one file per object (SPIR-V, Vulkan pipeline cache
// data), as mpv's ~/Library/Caches/io.mpv/shader_* holds it; PB_FRAMES (5) renders a phase; PB_SIZE=WxH the video (1920x1080);
// PB_OUT=WxH the target (the video's size); PB_DYNAMIC=1 render params' dynamic_constants; PB_LOG=1 libplacebo's
// messages (info and up) to stderr.
//
// Build: tools/pipebench/build.sh (against libplacebo 7.387, as tools/mpv.lock pins it; MoltenVK via the loader:
// VK_ICD_FILENAMES=<app>/Contents/Resources/vulkan/icd.d/MoltenVK_icd.json for the app's own).

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <libplacebo/log.h>
#include <libplacebo/cache.h>
#include <libplacebo/vulkan.h>
#include <libplacebo/renderer.h>
#include <libplacebo/shaders/custom.h>
#include <libplacebo/utils/upload.h>

static double now_ms(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec * 1e3 + t.tv_nsec / 1e6; }

// What libplacebo logged in the current phase: "Spent N ms <operation>"
enum { OP_SPIRV, OP_PIPE, OP_RESPEC, OP_COMPILE, OP_COUNT };
static const char *op_names[OP_COUNT] = { "translating SPIR-V", "creating pipeline", "re-specializing shader", "compiling shader" };
static int op_n[OP_COUNT];
static double op_ms[OP_COUNT];
static int verbose;

static void log_cb(void *priv, enum pl_log_level level, const char *msg) {
    (void) priv;
    const char *s = strstr(msg, "Spent ");
    if (s) {
        double ms = atof(s + 6);
        for (int i = 0; i < OP_COUNT; i++)
            if (strstr(s, op_names[i])) { op_n[i]++; op_ms[i] += ms; }
    }
    if (verbose && level <= PL_LOG_INFO) fprintf(stderr, "  [pl] %s\n", msg);
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

#define MAXH 32
struct hook { const char *path; const struct pl_hook *hook; };
static struct hook loaded[MAXH];
static int nloaded;

static const struct pl_hook *load(pl_gpu gpu, const char *path) {
    for (int i = 0; i < nloaded; i++)
        if (!strcmp(loaded[i].path, path)) return loaded[i].hook;
    size_t len; char *text = slurp(path, &len);
    const struct pl_hook *h = pl_mpv_user_shader_parse(gpu, text, len);
    free(text);
    if (!h) { fprintf(stderr, "%s: the shader didn't parse\n", path); exit(1); }
    loaded[nloaded++] = (struct hook) { path, h };
    return h;
}

static void set_par(const struct pl_hook *const *hooks, int nh, const char *name, double v) {
    for (int h = 0; h < nh; h++)
        for (int i = 0; i < hooks[h]->num_parameters; i++) {
            const struct pl_hook_par *p = &hooks[h]->parameters[i];
            if (strcmp(p->name, name)) continue;
            if (p->type == PL_VAR_FLOAT) p->data->f = (float) v;
            else if (p->type == PL_VAR_SINT) p->data->i = (int) v;
            else p->data->u = (unsigned) v;
        }
}

static void size_env(const char *name, int *w, int *h) {
    const char *v = getenv(name);
    if (v && strchr(v, 'x')) { *w = atoi(v); *h = atoi(strchr(v, 'x') + 1); }
}

int main(int argc, char **argv) {
    int a = 1;
    const char *pars[64]; int npars = 0;
    for (; a < argc && strcmp(argv[a], "--"); a++) pars[npars++] = argv[a];
    if (a >= argc) { fprintf(stderr, "usage: pipebench [name=value ...] -- a.glsl ... [-- a.glsl b.glsl ...]\n"); return 1; }
    verbose = getenv("PB_LOG") != NULL;
    int frames = getenv("PB_FRAMES") ? atoi(getenv("PB_FRAMES")) : 5;
    int w = 1920, h = 1080; size_env("PB_SIZE", &w, &h);
    int W = w, H = h; size_env("PB_OUT", &W, &H);

    double t_start = now_ms();
    pl_log log = pl_log_create(PL_API_VER, pl_log_params(.log_cb = log_cb, .log_level = PL_LOG_DEBUG));
    pl_vulkan vk = pl_vulkan_create(log, pl_vulkan_params());
    if (!vk) { fprintf(stderr, "no Vulkan device\n"); return 1; }
    pl_gpu gpu = vk->gpu;

    const char *cache_path = getenv("PB_CACHE");
    pl_cache cache = NULL;
    if (cache_path) {
        // One file per object, looked up as needed, as mpv's ~/Library/Caches/io.mpv/shader_<key>
        cache = pl_cache_create(pl_cache_params(.log = log, .get = pl_cache_get_dir, .set = pl_cache_set_dir,
                                                .priv = (void *) cache_path));
        pl_gpu_set_cache(gpu, cache);
    }
    printf("device ready in %.0f ms\n", now_ms() - t_start);

    struct pl_frame image = {
        .num_planes = 3,
        .repr = { .sys = PL_COLOR_SYSTEM_BT_709, .levels = PL_COLOR_LEVELS_LIMITED, .bits = { .sample_depth = 16, .color_depth = 10 } },
        .color = { .primaries = PL_COLOR_PRIM_BT_2020, .transfer = PL_COLOR_TRC_HLG },
    };
    pl_tex tex[3] = {0};
    pl_fmt fmt = pl_find_fmt(gpu, PL_FMT_UNORM, 4, 16, 16, PL_FMT_CAP_RENDERABLE);
    pl_tex target_tex = pl_tex_create(gpu, pl_tex_params(.w = W, .h = H, .format = fmt, .renderable = true));
    struct pl_frame target = {
        .num_planes = 1,
        .planes = {{ .texture = target_tex, .components = 4, .component_mapping = {0, 1, 2, 3} }},
        .repr = { .sys = PL_COLOR_SYSTEM_RGB, .levels = PL_COLOR_LEVELS_FULL, .bits = { .sample_depth = 16, .color_depth = 16 } },
        .color = image.color,
    };
    // PB_TARGET=pq:<nits> or 709: the target as mpv on an HDR screen sets it (PQ, its peak the screen's EDR headroom) or
    // SDR
    const char *tgt = getenv("PB_TARGET");
    if (tgt && !strncmp(tgt, "pq:", 3)) {
        target.color = (struct pl_color_space) { .primaries = PL_COLOR_PRIM_BT_2020, .transfer = PL_COLOR_TRC_PQ,
                                                 .hdr = { .max_luma = atof(tgt + 3), .min_luma = 0.0005f } };
    } else if (tgt && !strcmp(tgt, "709")) target.color = pl_color_space_bt709;

    // A gray picture with a ramp, 10-bit in 16, limited range
    size_t ysz = (size_t) w * h * 2, csz = (size_t) (w / 2) * (h / 2) * 2;
    uint16_t *buf = malloc(ysz + 2 * csz);
    for (int y = 0; y < h; y++) for (int x = 0; x < w; x++) buf[y * w + x] = (uint16_t) ((64 + (x * 876) / w) << 6);
    for (size_t i = ysz / 2; i < (ysz + 2 * csz) / 2; i++) buf[i] = 512 << 6;

    struct pl_render_params params = pl_render_default_params;
    params.upscaler = &pl_filter_lanczos;
    params.downscaler = &pl_filter_hermite;
    params.plane_upscaler = &pl_filter_bilinear;
    params.dynamic_constants = getenv("PB_DYNAMIC") != NULL;
    pl_renderer rr = pl_renderer_create(log, gpu);

    int phase = 0, n = 0;
    while (a < argc) {
        a++; // the "--"
        const struct pl_hook *hooks[MAXH]; int nh = 0;
        int first = a;
        for (; a < argc && strcmp(argv[a], "--"); a++) {
            // src=709 / src=hlg: the video's color space from this phase on (the underwater colour turned off / on)
            if (!strncmp(argv[a], "src=", 4)) {
                image.color = !strcmp(argv[a] + 4, "709") ? pl_color_space_bt709
                    : (struct pl_color_space) { .primaries = PL_COLOR_PRIM_BT_2020, .transfer = PL_COLOR_TRC_HLG };
                continue;
            }
            hooks[nh++] = load(gpu, argv[a]);
        }
        // Destroy what this phase doesn't list (as mpv-patches 0015 does)
        for (int i = nloaded - 1; i >= 0; i--) {
            int listed = 0;
            for (int j = first; j < a; j++) if (!strcmp(argv[j], loaded[i].path)) listed = 1;
            if (listed) continue;
            pl_mpv_user_shader_destroy(&loaded[i].hook);
            loaded[i] = loaded[--nloaded];
        }
        for (int i = 0; i < npars; i++) {
            char name[128]; const char *eq = strchr(pars[i], '=');
            if (!eq) continue;
            snprintf(name, sizeof name, "%.*s", (int) (eq - pars[i]), pars[i]);
            set_par(hooks, nh, name, atof(eq + 1));
        }
        params.hooks = hooks; params.num_hooks = nh;
        memset(op_n, 0, sizeof op_n); memset(op_ms, 0, sizeof op_ms);
        double first_ms = 0, rest_max = 0;
        for (int f = 0; f < frames; f++, n++) {
            for (int i = 0; i < 3; i++) {
                struct pl_plane_data data = {
                    .type = PL_FMT_UNORM, .width = i ? w / 2 : w, .height = i ? h / 2 : h,
                    .component_size = {16}, .component_map = {i}, .pixel_stride = 2, .row_stride = (size_t) (i ? w / 2 : w) * 2,
                    .pixels = (uint8_t *) buf + (i == 0 ? 0 : ysz + (i - 1) * csz),
                };
                if (!pl_upload_plane(gpu, &image.planes[i], &tex[i], &data)) { fprintf(stderr, "upload failed\n"); return 1; }
            }
            pl_frame_set_chroma_location(&image, PL_CHROMA_LEFT);
            set_par(hooks, nh, "PTS", n / 30.0);
            pl_gpu_finish(gpu);
            double t = now_ms();
            if (!pl_render_image(rr, &image, &target, &params)) { fprintf(stderr, "render failed\n"); return 1; }
            pl_gpu_finish(gpu);
            double ms = now_ms() - t;
            if (f == 0) first_ms = ms; else if (ms > rest_max) rest_max = ms;
        }
        printf("phase %d (%d shaders): first render %6.0f ms, later max %4.0f ms | SPIR-V compiled %2d (%5.0f ms) | "
               "pipelines %2d (%5.0f ms) | respec %d (%.0f ms)\n", ++phase, nh, first_ms, rest_max,
               op_n[OP_SPIRV], op_ms[OP_SPIRV], op_n[OP_PIPE], op_ms[OP_PIPE], op_n[OP_RESPEC], op_ms[OP_RESPEC]);
        fflush(stdout);
    }

    if (pl_renderer_get_errors(rr).errors) fprintf(stderr, "renderer reported errors: 0x%x\n", (unsigned) pl_renderer_get_errors(rr).errors);
    pl_renderer_destroy(&rr);
    for (int i = 0; i < nloaded; i++) pl_mpv_user_shader_destroy(&loaded[i].hook);
    for (int i = 0; i < 3; i++) pl_tex_destroy(gpu, &tex[i]);
    pl_tex_destroy(gpu, &target_tex);
    if (cache) {
        pl_gpu_set_cache(gpu, NULL);
        pl_cache_destroy(&cache);
    }
    pl_vulkan_destroy(&vk);
    pl_log_destroy(&log);
    free(buf);
    return 0;
}
