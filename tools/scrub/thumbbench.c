// thumbbench: how long a seek-bar picture takes from a file.
// thumbbench [--cold] [--hw] [--exact] [--width W] [--n N] [--dump dir] file
// Opens the file once, then for N times spread over the clip (in a shuffled order, as a mouse would): seek to the
// keyframe at/before t, decode the first frame (or, --exact, up to t), scale to W wide RGBA. Prints open ms, per-picture
// ms (median, p90, max), bytes read per picture. --cold: drop the file's pages from the cache first (msync MS_INVALIDATE).
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext.h>
#include <libavutil/imgutils.h>
#include <libswscale/swscale.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <fcntl.h>
#include <unistd.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

static double now_ms(void) { struct timespec t; clock_gettime(CLOCK_MONOTONIC, &t); return t.tv_sec * 1e3 + t.tv_nsec / 1e6; }
static int cmp(const void *a, const void *b) { double x = *(double *)a, y = *(double *)b; return x < y ? -1 : x > y; }

static void evict(const char *path)
{
    int fd = open(path, O_RDONLY);
    if (fd < 0) return;
    struct stat st; fstat(fd, &st);
    void *p = mmap(NULL, st.st_size, PROT_READ, MAP_SHARED, fd, 0);
    if (p != MAP_FAILED) { msync(p, st.st_size, MS_INVALIDATE); munmap(p, st.st_size); }
    close(fd);
}

static enum AVPixelFormat hw_fmt(AVCodecContext *c, const enum AVPixelFormat *f)
{
    for (; *f != AV_PIX_FMT_NONE; f++) if (*f == AV_PIX_FMT_VIDEOTOOLBOX) return *f;
    return c->sw_pix_fmt;
}

int main(int argc, char **argv)
{
    int cold = 0, hw = 0, exact = 0, W = 320, N = 40; const char *dump = NULL, *path = NULL;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--cold")) cold = 1;
        else if (!strcmp(argv[i], "--hw")) hw = 1;
        else if (!strcmp(argv[i], "--exact")) exact = 1;
        else if (!strcmp(argv[i], "--width")) W = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--n")) N = atoi(argv[++i]);
        else if (!strcmp(argv[i], "--dump")) dump = argv[++i];
        else path = argv[i];
    }
    av_log_set_level(AV_LOG_ERROR);
    if (cold) evict(path);
    double t0 = now_ms();
    AVFormatContext *fc = NULL;
    if (avformat_open_input(&fc, path, NULL, NULL) < 0) { fprintf(stderr, "open failed\n"); return 1; }
    avformat_find_stream_info(fc, NULL);
    int vs = av_find_best_stream(fc, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    AVStream *st = fc->streams[vs];
    const AVCodec *dec = avcodec_find_decoder(st->codecpar->codec_id);
    AVCodecContext *cc = avcodec_alloc_context3(dec);
    avcodec_parameters_to_context(cc, st->codecpar);
    cc->thread_count = 0; cc->thread_type = FF_THREAD_SLICE;
    if (hw) {
        AVBufferRef *dev = NULL;
        if (av_hwdevice_ctx_create(&dev, AV_HWDEVICE_TYPE_VIDEOTOOLBOX, NULL, NULL, 0) == 0) { cc->hw_device_ctx = dev; cc->get_format = hw_fmt; }
    }
    avcodec_open2(cc, dec, NULL);
    double open_ms = now_ms() - t0;
    int64_t open_bytes = fc->pb ? avio_tell(fc->pb) : 0;
    double dur = fc->duration / (double)AV_TIME_BASE;
    double *ms = calloc(N, sizeof(double)); double bytes_total = 0;
    AVPacket *pkt = av_packet_alloc(); AVFrame *fr = av_frame_alloc(), *sw = av_frame_alloc();
    struct SwsContext *sc = NULL; uint8_t *rgba = NULL;
    // Times: N spread over the clip, visited in a fixed shuffled order
    double *ts = calloc(N, sizeof(double));
    for (int i = 0; i < N; i++) ts[i] = dur * (i + 0.5) / N;
    srand(7); for (int i = N - 1; i > 0; i--) { int j = rand() % (i + 1); double x = ts[i]; ts[i] = ts[j]; ts[j] = x; }
    int frames_decoded = 0;
    for (int i = 0; i < N; i++) {
        double t = ts[i];
        int64_t before = fc->pb->bytes_read;
        double a = now_ms();
        int64_t target = (int64_t)(t / av_q2d(st->time_base)) + (st->start_time != AV_NOPTS_VALUE ? st->start_time : 0);
        av_seek_frame(fc, vs, target, AVSEEK_FLAG_BACKWARD);
        avcodec_flush_buffers(cc);
        int got = 0;
        while (!got && av_read_frame(fc, pkt) >= 0) {
            if (pkt->stream_index == vs) {
                avcodec_send_packet(cc, pkt);
                if (!exact) avcodec_send_packet(cc, NULL);
                while (avcodec_receive_frame(cc, fr) == 0) {
                    frames_decoded++;
                    if (exact && fr->pts != AV_NOPTS_VALUE && fr->pts < target) { av_frame_unref(fr); continue; }
                    got = 1; break;
                }
            }
            av_packet_unref(pkt);
        }
        if (!got) { avcodec_send_packet(cc, NULL); if (avcodec_receive_frame(cc, fr) == 0) got = 1; }
        if (got) {
            AVFrame *src = fr;
            if (fr->format == AV_PIX_FMT_VIDEOTOOLBOX) { av_hwframe_transfer_data(sw, fr, 0); src = sw; }
            int H = (int)((double)W * src->height / src->width) & ~1;
            sc = sws_getCachedContext(sc, src->width, src->height, src->format, W, H, AV_PIX_FMT_RGBA, SWS_BILINEAR, NULL, NULL, NULL);
            if (!rgba) rgba = malloc(W * H * 4 + 64);
            uint8_t *dst[4] = { rgba }; int ls[4] = { W * 4 };
            sws_scale(sc, (const uint8_t *const *)src->data, src->linesize, 0, src->height, dst, ls);
            if (dump && i < 4) {
                char fn[1024]; snprintf(fn, sizeof fn, "%s/t%02d_%.2f.rgba_%dx%d", dump, i, t, W, H);
                FILE *f = fopen(fn, "wb"); fwrite(rgba, 1, W * H * 4, f); fclose(f);
            }
            av_frame_unref(sw);
        }
        av_frame_unref(fr);
        ms[i] = now_ms() - a;
        bytes_total += fc->pb->bytes_read - before;
    }
    double total = 0; for (int i = 0; i < N; i++) total += ms[i];
    qsort(ms, N, sizeof(double), cmp);
    printf("%s%s%s open %.0f ms (%.0f KB) | per picture median %.1f p90 %.1f max %.1f ms, all %d: %.0f ms | %.0f KB read each, %.1f frames decoded each\n",
           cold ? "cold " : "warm ", hw ? "hw " : "sw ", exact ? "exact" : "key", open_ms, open_bytes / 1024.0,
           ms[N / 2], ms[(int)(N * 0.9)], ms[N - 1], N, total, bytes_total / N / 1024.0, (double)frames_decoded / N);
    return 0;
}
