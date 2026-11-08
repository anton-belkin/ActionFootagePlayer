// SPDX-License-Identifier: GPL-3.0-or-later
//
// afp-render: a clip rendered on the GPU, start to end, as an HLS stream (docs/gpu-render.md, "The streaming
// server"). Linux with VAAPI and Vulkan (the NAS's Intel N100): the frame never leaves the GPU.
//
//   VAAPI decode -> each plane imported into Vulkan as a dmabuf (as mpv does) -> libplacebo renders the frame with
//   mpv user shaders (the Gyroflow warp from gyroflow-gpu-table, its PTS set per frame as mpv sets it; the look) into
//   the encoder's own VAAPI P010 surface (also imported) -> hevc_vaapi (VDEnc, Main10) -> HLS, fMP4 segments, the audio
//   copied.
//
//   afp-render [options] <in.mp4> <out dir> [shader.glsl ...]
//     -w <width> -h <height>   output size (default 1920x1080)
//     -ss <seconds>            start there
//     -t <seconds>             only this long
//     -offset <seconds>        the stream's time starts there in the clip (a moment: its part, from 0): the shaders and
//                              the segments get the clip's time minus this, as mpv plays a moment's EDL
//     -seg <seconds>           segment length (default 1): a keyframe every round(fps * seg) frames, a segment each
//     -start_number <n>        the first segment's number (a render from -ss into a VOD playlist's segments: the
//                              timestamps are the clip's own, so the segments of several renders fit together)
//     -b <bitrate>             video bitrate, e.g. 12M (default 12M)
//     --hlg                    the source is log made HLG by a look shader (the dlogm plugin's way): tagged
//                              BT.2020/HLG in and out, no conversion between; else the file's own tags
//     --sdr                    with --hlg: the shaders end in SDR (the player's nbcu look: nbcu_hlg2sdr.glsl last), tagged
//                              BT.709 in and out, no conversion between
//     --pq                     with --hlg: the HLG made HDR10 by libplacebo (BT.2020, PQ; HLG's nominal peak, 1000 nits)
//     --realtime               no faster than the clip's own rate
//
// Build: cc -O2 afp-render.c -o afp-render $(pkg-config --cflags --libs libplacebo libavformat libavcodec libavutil libva libva-drm)

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#include <unistd.h>
#include <sys/stat.h>

#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libavutil/hwcontext.h>
#include <libavutil/hwcontext_vaapi.h>
#include <libavutil/opt.h>
#include <libavutil/pixdesc.h>
#include <va/va.h>
#include <va/va_drmcommon.h>

#include <libplacebo/log.h>
#include <libplacebo/renderer.h>
#include <libplacebo/shaders/custom.h>
#include <libplacebo/utils/libav.h>
#include <libplacebo/vulkan.h>

#define DIE(...) do { fprintf(stderr, "afp-render: " __VA_ARGS__); fprintf(stderr, "\n"); exit(1); } while (0)

static double now(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec + t.tv_nsec / 1e9; }

// A VAAPI surface's planes as libplacebo textures, kept per surface: the decoder's and the encoder's pools reuse theirs
struct mapped { VASurfaceID id; pl_tex tex[2]; };
#define MAX_MAPPED 64
static struct mapped mapped[2][MAX_MAPPED];
static int num_mapped[2];

static pl_gpu gpu;
static VADisplay va;

static pl_tex *map_surface(int pool, VASurfaceID id, int w, int h, bool write)
{
    for (int i = 0; i < num_mapped[pool]; i++)
        if (mapped[pool][i].id == id) return mapped[pool][i].tex;
    if (num_mapped[pool] == MAX_MAPPED) DIE("too many surfaces");
    VADRMPRIMESurfaceDescriptor desc;
    VAStatus st = vaExportSurfaceHandle(va, id, VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
        (write ? VA_EXPORT_SURFACE_WRITE_ONLY : VA_EXPORT_SURFACE_READ_ONLY) | VA_EXPORT_SURFACE_SEPARATE_LAYERS, &desc);
    if (st != VA_STATUS_SUCCESS) DIE("vaExportSurfaceHandle: %s", vaErrorStr(st));
    if (desc.num_layers != 2) DIE("expected 2 layers (P010), got %d", desc.num_layers);
    struct mapped *m = &mapped[pool][num_mapped[pool]++];
    m->id = id;
    const char *fmts[2] = { "r16", "rg16" };
    for (int n = 0; n < 2; n++) {
        int obj = desc.layers[n].object_index[0];
        uint32_t size = desc.objects[obj].size;
        if (!size) { size = lseek(desc.objects[obj].fd, 0, SEEK_END); lseek(desc.objects[obj].fd, 0, SEEK_SET); }
        m->tex[n] = pl_tex_create(gpu, &(struct pl_tex_params) {
            .w = n ? (w + 1) / 2 : w, .h = n ? (h + 1) / 2 : h,
            .format = pl_find_named_fmt(gpu, fmts[n]),
            .sampleable = !write, .renderable = write, .storable = false,
            .import_handle = PL_HANDLE_DMA_BUF,
            .shared_mem = {
                .handle.fd = desc.objects[obj].fd,
                .size = size,
                .offset = desc.layers[n].offset[0],
                .drm_format_mod = desc.objects[obj].drm_format_modifier,
                .stride_w = desc.layers[n].pitch[0],
            },
        });
        if (!m->tex[n]) DIE("importing plane %d of surface %u (%s) failed", n, id, write ? "render target" : "source");
    }
    for (uint32_t i = 0; i < desc.num_objects; i++) close(desc.objects[i].fd);
    return m->tex;
}

static enum AVPixelFormat get_vaapi(AVCodecContext *c, const enum AVPixelFormat *f)
{
    for (; *f != AV_PIX_FMT_NONE; f++) if (*f == AV_PIX_FMT_VAAPI) return *f;
    return AV_PIX_FMT_NONE;
}

static char *read_file(const char *path, size_t *len)
{
    FILE *f = fopen(path, "rb");
    if (!f) DIE("can't read %s", path);
    fseek(f, 0, SEEK_END); *len = ftell(f); fseek(f, 0, SEEK_SET);
    char *buf = malloc(*len + 1);
    if (fread(buf, 1, *len, f) != *len) DIE("can't read %s", path);
    buf[*len] = 0;
    fclose(f);
    return buf;
}

int main(int argc, char **argv)
{
    int W = 1920, H = 1080, start_number = 0; double start = 0, duration = 0, seg = 1.0, offset = 0; const char *bitrate = "12M"; bool hlg = false, realtime = false, sdr = false, pq = false;
    const char *pos[64]; int npos = 0;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "-w") && i + 1 < argc) W = atoi(argv[++i]);
        else if (!strcmp(argv[i], "-h") && i + 1 < argc) H = atoi(argv[++i]);
        else if (!strcmp(argv[i], "-ss") && i + 1 < argc) start = atof(argv[++i]);
        else if (!strcmp(argv[i], "-t") && i + 1 < argc) duration = atof(argv[++i]);
        else if (!strcmp(argv[i], "-offset") && i + 1 < argc) offset = atof(argv[++i]);
        else if (!strcmp(argv[i], "-start_number") && i + 1 < argc) start_number = atoi(argv[++i]);
        else if (!strcmp(argv[i], "-seg") && i + 1 < argc) seg = atof(argv[++i]);
        else if (!strcmp(argv[i], "-b") && i + 1 < argc) bitrate = argv[++i];
        else if (!strcmp(argv[i], "--hlg")) hlg = true;
        else if (!strcmp(argv[i], "--sdr")) sdr = true;
        else if (!strcmp(argv[i], "--pq")) pq = true;
        else if (!strcmp(argv[i], "--realtime")) realtime = true;
        else if (npos < 64) pos[npos++] = argv[i];
    }
    if (npos < 2) DIE("usage: afp-render [-w W -h H -ss S -t T -offset O -b RATE --hlg --realtime] <in> <out dir> [shader.glsl ...]");
    W &= ~1; H &= ~1;
    const char *in = pos[0], *outdir = pos[1];
    mkdir(outdir, 0755);

    // ---- Input, VAAPI decoding ----
    AVFormatContext *ic = NULL;
    if (avformat_open_input(&ic, in, NULL, NULL) < 0) DIE("can't open %s", in);
    avformat_find_stream_info(ic, NULL);
    int vs = av_find_best_stream(ic, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    int as = av_find_best_stream(ic, AVMEDIA_TYPE_AUDIO, -1, vs, NULL, 0);
    if (vs < 0) DIE("no video in %s", in);
    AVStream *vst = ic->streams[vs];
    AVBufferRef *hw = NULL;
    if (av_hwdevice_ctx_create(&hw, AV_HWDEVICE_TYPE_VAAPI, "/dev/dri/renderD128", NULL, 0) < 0) DIE("no VAAPI device");
    va = ((AVVAAPIDeviceContext *) ((AVHWDeviceContext *) hw->data)->hwctx)->display;
    const AVCodec *dec = avcodec_find_decoder(vst->codecpar->codec_id);
    AVCodecContext *dc = avcodec_alloc_context3(dec);
    avcodec_parameters_to_context(dc, vst->codecpar);
    dc->hw_device_ctx = av_buffer_ref(hw);
    dc->get_format = get_vaapi;
    dc->extra_hw_frames = 8;
    if (avcodec_open2(dc, dec, NULL) < 0) DIE("can't open the decoder");
    AVRational fr = av_guess_frame_rate(ic, vst, NULL);
    double fps = fr.num > 0 ? av_q2d(fr) : 30.0;
    double t0 = vst->start_time != AV_NOPTS_VALUE ? vst->start_time * av_q2d(vst->time_base) : 0.0;
    if (start > 0) av_seek_frame(ic, -1, (int64_t) ((start + t0) * AV_TIME_BASE), AVSEEK_FLAG_BACKWARD);

    // ---- libplacebo on Vulkan ----
    pl_log log = pl_log_create(PL_API_VER, pl_log_params(.log_cb = pl_log_color, .log_level = PL_LOG_WARN));
    pl_vulkan vk = pl_vulkan_create(log, pl_vulkan_params(.allow_software = false));
    if (!vk) DIE("no Vulkan device");
    gpu = vk->gpu;
    if (!(gpu->import_caps.tex & PL_HANDLE_DMA_BUF)) DIE("Vulkan can't import dmabufs");
    pl_renderer rr = pl_renderer_create(log, gpu);
    const struct pl_hook *hooks[16]; int nhooks = 0;
    union pl_var_data *pts_vars[16]; int npts = 0;
    for (int i = 2; i < npos && nhooks < 16; i++) {
        size_t len; char *text = read_file(pos[i], &len);
        const struct pl_hook *h = pl_mpv_user_shader_parse(gpu, text, len);
        if (!h) DIE("can't parse %s", pos[i]);
        for (int p = 0; p < h->num_parameters; p++)
            if (!strcmp(h->parameters[p].name, "PTS") && npts < 16) pts_vars[npts++] = h->parameters[p].data;
        hooks[nhooks++] = h;
        free(text);
    }

    // ---- Encoder: hevc_vaapi, low power (VDEnc), Main10, into its own P010 surfaces ----
    const AVCodec *enc = avcodec_find_encoder_by_name("hevc_vaapi");
    if (!enc) DIE("no hevc_vaapi");
    AVCodecContext *ec = avcodec_alloc_context3(enc);
    ec->width = W; ec->height = H;
    ec->time_base = av_inv_q(fr.num > 0 ? fr : (AVRational) { 30, 1 });
    ec->framerate = fr;
    ec->pix_fmt = AV_PIX_FMT_VAAPI;
    ec->profile = AV_PROFILE_HEVC_MAIN_10;
    ec->gop_size = (int) (fps * seg + 0.5);
    ec->max_b_frames = 0;
    ec->bit_rate = 0;
    av_opt_set(ec, "b", bitrate, AV_OPT_SEARCH_CHILDREN);
    ec->rc_max_rate = 0;
    ec->color_range = AVCOL_RANGE_MPEG;
    if (hlg && sdr) {
        ec->color_primaries = AVCOL_PRI_BT709; ec->color_trc = AVCOL_TRC_BT709; ec->colorspace = AVCOL_SPC_BT709;
    } else if (hlg && pq) {
        ec->color_primaries = AVCOL_PRI_BT2020; ec->color_trc = AVCOL_TRC_SMPTE2084; ec->colorspace = AVCOL_SPC_BT2020_NCL;
    } else if (hlg) {
        ec->color_primaries = AVCOL_PRI_BT2020; ec->color_trc = AVCOL_TRC_ARIB_STD_B67; ec->colorspace = AVCOL_SPC_BT2020_NCL;
    } else {
        ec->color_primaries = vst->codecpar->color_primaries; ec->color_trc = vst->codecpar->color_trc; ec->colorspace = vst->codecpar->color_space;
    }
    ec->chroma_sample_location = AVCHROMA_LOC_LEFT;
    ec->flags |= AV_CODEC_FLAG_GLOBAL_HEADER;
    av_opt_set_int(ec->priv_data, "low_power", 1, 0);
    AVBufferRef *ef = av_hwframe_ctx_alloc(hw);
    AVHWFramesContext *efc = (AVHWFramesContext *) ef->data;
    efc->format = AV_PIX_FMT_VAAPI; efc->sw_format = AV_PIX_FMT_P010; efc->width = W; efc->height = H; efc->initial_pool_size = 24;
    if (av_hwframe_ctx_init(ef) < 0) DIE("can't make the encoder's surfaces");
    ec->hw_frames_ctx = av_buffer_ref(ef);
    if (avcodec_open2(ec, enc, NULL) < 0) DIE("can't open hevc_vaapi");

    // ---- HLS out: fMP4 segments of a second, the playlist growing as they come ----
    char playlist[1024], segs[1024];
    snprintf(playlist, sizeof playlist, "%s/render.m3u8", outdir);
    snprintf(segs, sizeof segs, "%s/seg%%05d.m4s", outdir);
    AVFormatContext *oc = NULL;
    avformat_alloc_output_context2(&oc, NULL, "hls", playlist);
    AVStream *ovs = avformat_new_stream(oc, NULL);
    avcodec_parameters_from_context(ovs->codecpar, ec);
    ovs->codecpar->codec_tag = MKTAG('h', 'v', 'c', '1');
    ovs->time_base = ec->time_base;
    AVStream *oas = NULL;
    if (as >= 0) {
        oas = avformat_new_stream(oc, NULL);
        avcodec_parameters_copy(oas->codecpar, ic->streams[as]->codecpar);
        oas->codecpar->codec_tag = 0;
        oas->time_base = ic->streams[as]->time_base;
    }
    AVDictionary *mo = NULL;
    char hls_time[32];
    snprintf(hls_time, sizeof hls_time, "%g", seg);
    av_dict_set(&mo, "hls_time", hls_time, 0);
    av_dict_set(&mo, "hls_segment_type", "fmp4", 0);
    av_dict_set(&mo, "hls_playlist_type", "event", 0);
    av_dict_set(&mo, "hls_flags", "independent_segments+temp_file", 0);
    av_dict_set(&mo, "hls_segment_filename", segs, 0);
    av_dict_set(&mo, "hls_fmp4_init_filename", "init.mp4", 0);
    char sn[32];
    snprintf(sn, sizeof sn, "%d", start_number);
    av_dict_set(&mo, "start_number", sn, 0);
    if (avformat_write_header(oc, &mo) < 0) DIE("can't start the HLS output");

    // ---- The frames ----
    struct pl_color_space color = hlg ? (struct pl_color_space) { .primaries = PL_COLOR_PRIM_BT_2020, .transfer = PL_COLOR_TRC_HLG } : (struct pl_color_space) {
        .primaries = pl_primaries_from_av(vst->codecpar->color_primaries), .transfer = pl_transfer_from_av(vst->codecpar->color_trc) };
    // What the frames are in and go out as: the same (no conversion), but SDR's (the shaders made it SDR) or HDR10's
    // (libplacebo converts the HLG)
    if (hlg && sdr) color = (struct pl_color_space) { .primaries = PL_COLOR_PRIM_BT_709, .transfer = PL_COLOR_TRC_BT_1886 };
    struct pl_color_space color_out = color;
    if (hlg && pq) color_out = (struct pl_color_space) { .primaries = PL_COLOR_PRIM_BT_2020, .transfer = PL_COLOR_TRC_PQ };
    struct pl_color_repr repr_out = {
        .sys = hlg && !sdr ? PL_COLOR_SYSTEM_BT_2020_NC : hlg ? PL_COLOR_SYSTEM_BT_709 : pl_system_from_av(vst->codecpar->color_space), .levels = PL_COLOR_LEVELS_LIMITED,
        .bits = { .sample_depth = 16, .color_depth = 10, .bit_shift = 6 },
    };
    if (repr_out.sys == PL_COLOR_SYSTEM_UNKNOWN) repr_out.sys = PL_COLOR_SYSTEM_BT_709;
    struct pl_render_params params = pl_render_fast_params;
    params.hooks = hooks; params.num_hooks = nhooks;
    params.upscaler = &pl_filter_bilinear; params.downscaler = &pl_filter_bilinear;
    AVPacket *pkt = av_packet_alloc(), *opkt = av_packet_alloc();
    // Frames in flight: rendered, waiting for the GPU before the encoder takes them; their decoded sources held until then
#define DEPTH 3
    struct { AVFrame *src, *out; pl_tex tex; } queue[DEPTH];
    int qn = 0;
#define DRAIN_ONE() do { \
        while (pl_tex_poll(gpu, queue[0].tex, UINT64_MAX)) ; \
        if (avcodec_send_frame(ec, queue[0].out) < 0) DIE("encoding failed"); \
        av_frame_free(&queue[0].out); av_frame_free(&queue[0].src); \
        memmove(&queue[0], &queue[1], sizeof(queue[0]) * (DEPTH - 1)); qn--; \
        while (avcodec_receive_packet(ec, opkt) == 0) { \
            if (!opkt->duration) opkt->duration = 1; \
            av_packet_rescale_ts(opkt, ec->time_base, ovs->time_base); \
            opkt->stream_index = ovs->index; \
            av_interleaved_write_frame(oc, opkt); \
        } \
    } while (0)
    AVFrame *f = av_frame_alloc();
    int64_t frames = 0; double tstart = now(), first_pts = -1;
    bool eof = false;
    while (!eof) {
        int r = av_read_frame(ic, pkt);
        if (r < 0) { eof = true; avcodec_send_packet(dc, NULL); }
        else if (pkt->stream_index == as && oas) {
            double ts = pkt->pts * av_q2d(ic->streams[as]->time_base) - t0;
            if (ts >= start - 0.05) {
                av_packet_rescale_ts(pkt, ic->streams[as]->time_base, oas->time_base);
                int64_t off = av_rescale_q((int64_t) ((t0 + offset) * 1e6), (AVRational) { 1, 1000000 }, oas->time_base);
                pkt->pts -= off; pkt->dts -= off;
                pkt->stream_index = oas->index;
                av_interleaved_write_frame(oc, pkt);
            }
            av_packet_unref(pkt);
            continue;
        } else if (pkt->stream_index != vs) { av_packet_unref(pkt); continue; }
        else { avcodec_send_packet(dc, pkt); av_packet_unref(pkt); }
        while (avcodec_receive_frame(dc, f) == 0) {
            double pts = f->best_effort_timestamp * av_q2d(vst->time_base) - t0;
            if (pts < start - 0.5 / fps) { av_frame_unref(f); continue; }
            if (duration > 0 && pts >= start + duration) { av_frame_unref(f); eof = true; break; }
            if (first_pts < 0) first_pts = pts;
            // The shaders find the frame by mpv's PTS: the file's time from its start
            for (int i = 0; i < npts; i++) pts_vars[i]->f = (float) (pts - offset);
            VASurfaceID sid = (VASurfaceID) (uintptr_t) f->data[3];
            vaSyncSurface(va, sid);
            pl_tex *src_tex = map_surface(0, sid, f->width, f->height, false);
            AVFrame *of = av_frame_alloc();
            if (av_hwframe_get_buffer(ef, of, 0) < 0) DIE("no encoder surface");
            VASurfaceID oid = (VASurfaceID) (uintptr_t) of->data[3];
            pl_tex *dst_tex = map_surface(1, oid, W, H, true);

            struct pl_frame src = {
                .num_planes = 2,
                .planes = {
                    { .texture = src_tex[0], .components = 1, .component_mapping = { PL_CHANNEL_Y } },
                    { .texture = src_tex[1], .components = 2, .component_mapping = { PL_CHANNEL_CB, PL_CHANNEL_CR } },
                },
                .repr = { .sys = hlg ? PL_COLOR_SYSTEM_BT_2020_NC : pl_system_from_av(f->colorspace), .levels = pl_levels_from_av(f->color_range),
                          .bits = { .sample_depth = 16, .color_depth = 10, .bit_shift = 6 } },
                .color = color,
            };
            if (src.repr.sys == PL_COLOR_SYSTEM_UNKNOWN) src.repr.sys = PL_COLOR_SYSTEM_BT_709;
            if (src.repr.levels == PL_COLOR_LEVELS_UNKNOWN) src.repr.levels = PL_COLOR_LEVELS_LIMITED;
            pl_frame_set_chroma_location(&src, PL_CHROMA_LEFT);
            struct pl_frame dst = {
                .num_planes = 2,
                .planes = {
                    { .texture = dst_tex[0], .components = 1, .component_mapping = { PL_CHANNEL_Y } },
                    { .texture = dst_tex[1], .components = 2, .component_mapping = { PL_CHANNEL_CB, PL_CHANNEL_CR } },
                },
                .repr = repr_out,
                .color = color_out,
            };
            pl_frame_set_chroma_location(&dst, PL_CHROMA_LEFT);
            if (!pl_render_image(rr, &src, &dst, &params)) DIE("rendering failed");
            pl_gpu_flush(gpu);

            // The clip's own time: segments of renders from different points fit one playlist
            of->pts = av_rescale_q((int64_t) llround((pts - offset) * 1e6), (AVRational) { 1, 1000000 }, ec->time_base);
            of->color_primaries = ec->color_primaries; of->color_trc = ec->color_trc; of->colorspace = ec->colorspace; of->color_range = AVCOL_RANGE_MPEG;
            if (qn == DEPTH) DRAIN_ONE();
            queue[qn].src = av_frame_clone(f);
            queue[qn].out = of;
            queue[qn].tex = dst_tex[1];
            qn++;
            av_frame_unref(f);
            frames++;
            if (realtime) {
                double ahead = (pts - first_pts) - (now() - tstart);
                if (ahead > 0) usleep((useconds_t) (ahead * 1e6));
            }
            if (frames % 120 == 0)
                fprintf(stderr, "afp-render: %lld frames, %.1f fps\n", (long long) frames, frames / (now() - tstart));
        }
    }
    while (qn > 0) DRAIN_ONE();
    avcodec_send_frame(ec, NULL);
    while (avcodec_receive_packet(ec, opkt) == 0) {
        av_packet_rescale_ts(opkt, ec->time_base, ovs->time_base);
        opkt->stream_index = ovs->index;
        av_interleaved_write_frame(oc, opkt);
    }
    av_write_trailer(oc);
    fprintf(stderr, "afp-render: done, %lld frames in %.1f s (%.1f fps)\n", (long long) frames, now() - tstart, frames / (now() - tstart));
    return 0;
}
