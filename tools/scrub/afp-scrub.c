// afp-scrub: the pictures over the player's seek bar (docs/design/seek-and-scrub.md, "Preview on hover"). A long-lived helper
// on the libav libraries mpv ships: it keeps the files it was asked about and their decoders open, so a picture takes
// ~2-50 ms where an ffmpeg process per picture takes 160-180 ms.
//
//   afp-scrub                          requests on stdin, replies on stdout (tests, tools/scrub/scrubbench.py)
//   afp-scrub --ipc <socket> [--pid N] [--to <script>]
//                                      the player's: requests from mpv's property user-data/afp-scrub/request (set by
//                                      side-panel/scrub.lua), replies as `script-message-to <script> scrub <reply>`
//                                      (default side_panel); --pid: mpv's pid, to be sure the socket is its; it exits
//                                      when mpv closes the socket
//   --cache-mb N                       pictures kept in memory (default 160 MB: ~330 pictures of 460x260)
//
// A request, one line, tab-separated:
//   <id> <file> <time> <exact|key> <out.bgra> <w> <h> [<clip>]
//   exact  the frame nearest <time> (seconds from the file's start), decoded on the CPU with frame threads: for the
//          camera's low-resolution sidecar (DJI .LRF, GoPro .LRV). When <time> is just ahead of the frame the decoder
//          is at (< 2 s), it decodes on without a seek (~2 ms as the mouse moves along the bar)
//   key    the keyframe at or before <time>, decoded by the GPU (VideoToolbox, D3D11VA, VA-API; else the CPU): the
//          keyframe's packet alone, then a flush. For a clip without a sidecar
//   open-exact, open-key  the file opened (and its decoder set up) for such requests, nothing decoded: the mouse is
//          over the bar, the track's next. Answered "<id> opened <ms>"
//   <clip> with exact: the clip the sidecar belongs to. The sidecar is used only when its duration is the clip's
//          (within 0.1 s); otherwise the clip's keyframe is the answer (kind "key")
//   The picture is scaled to <w>x<h> BGRA (premultiplied, opaque) and written to <out> (through <out>.tmp, renamed):
//   what mpv's overlay-add reads.
// The newest request wins: one waiting when another comes is answered "dropped", and one being decoded stops at the
// next frame.
//
// A reply, one line: <id> ok <ms> <kind> <frame time> | <id> opened <ms> | <id> dropped | <id> error <message>
//   kind: exact, walk (decoded on, no seek), key, cache (made before, kept in memory)
#include <libavformat/avformat.h>
#include <libavcodec/avcodec.h>
#include <libavutil/hwcontext.h>
#include <libavutil/imgutils.h>
#include <libswscale/swscale.h>
#include <pthread.h>
#include <errno.h>
#include <stdarg.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#ifndef _WIN32
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <signal.h>
#endif

#define MAX_SRC 4
#define LINE 8192
// The file read in bigger pieces than libav's 32 KB: fewer round trips to a NAS (each picture reads 0.1-1 MB)
#define IO_BUF (256 * 1024)
// Decoded on without a seek when the time is this far ahead of the decoder at most (s)
#define WALK 2.0

static double now_ms(void)
{
    struct timespec t;
    clock_gettime(CLOCK_MONOTONIC, &t);
    return t.tv_sec * 1e3 + t.tv_nsec / 1e6;
}

// ---------- requests and replies ----------

typedef struct {
    char id[64], file[2048], out[2048], clip[2048];
    double t;
    int exact, open, w, h;
} Request;

static pthread_mutex_t lock = PTHREAD_MUTEX_INITIALIZER;
static pthread_cond_t cond = PTHREAD_COND_INITIALIZER;
static Request waiting;
static int have_waiting = 0, closed = 0;
// Bumped by each request: a decode under way stops when it changes
static volatile int generation = 0;

static pthread_mutex_t out_lock = PTHREAD_MUTEX_INITIALIZER;
static int ipc_fd = -1;
static const char *reply_to = "side_panel";

static void json_string(char *dst, size_t n, const char *s)
{
    size_t j = 0;
    dst[j++] = '"';
    for (; *s && j + 8 < n; s++) {
        unsigned char c = (unsigned char)*s;
        if (c == '"' || c == '\\') { dst[j++] = '\\'; dst[j++] = c; }
        else if (c < 0x20) j += snprintf(dst + j, n - j, "\\u%04x", c);
        else dst[j++] = c;
    }
    dst[j++] = '"';
    dst[j] = 0;
}

static void send_all(int fd, const char *s, size_t n)
{
#ifndef _WIN32
    while (n > 0) {
        ssize_t k = write(fd, s, n);
        if (k < 0 && errno == EINTR) continue;
        if (k <= 0) return;
        s += k; n -= (size_t)k;
    }
#endif
}

static void reply(const char *fmt, ...)
{
    char line[LINE];
    va_list ap;
    va_start(ap, fmt);
    vsnprintf(line, sizeof line, fmt, ap);
    va_end(ap);
    pthread_mutex_lock(&out_lock);
    if (ipc_fd >= 0) {
        char a[LINE + 64], b[256], msg[2 * LINE + 512];
        json_string(a, sizeof a, line);
        json_string(b, sizeof b, reply_to);
        int n = snprintf(msg, sizeof msg, "{\"command\":[\"script-message-to\",%s,\"scrub\",%s]}\n", b, a);
        send_all(ipc_fd, msg, (size_t)n);
    } else {
        fputs(line, stdout);
        fputc('\n', stdout);
        fflush(stdout);
    }
    pthread_mutex_unlock(&out_lock);
}

static int parse_request(char *line, Request *r)
{
    char *f[8] = { 0 };
    int n = 0;
    char *p = line;
    while (n < 8) {
        f[n++] = p;
        char *tab = strchr(p, '\t');
        if (!tab) break;
        *tab = 0;
        p = tab + 1;
    }
    char *nl = strpbrk(f[n - 1], "\r\n");
    if (nl) *nl = 0;
    if (n < 7) return 0;
    memset(r, 0, sizeof *r);
    snprintf(r->id, sizeof r->id, "%s", f[0]);
    snprintf(r->file, sizeof r->file, "%s", f[1]);
    r->t = atof(f[2]);
    r->exact = !strcmp(f[3], "exact") || !strcmp(f[3], "open-exact");
    r->open = !strncmp(f[3], "open-", 5);
    snprintf(r->out, sizeof r->out, "%s", f[4]);
    r->w = atoi(f[5]);
    r->h = atoi(f[6]);
    if (n >= 8) snprintf(r->clip, sizeof r->clip, "%s", f[7]);
    return r->w > 0 && r->h > 0 && r->w <= 4096 && r->h <= 4096;
}

static void submit(char *line)
{
    Request r;
    if (!parse_request(line, &r)) {
        if (*line) reply("%.60s\terror\tbad request", line);
        return;
    }
    pthread_mutex_lock(&lock);
    if (have_waiting) reply("%s\tdropped", waiting.id);
    waiting = r;
    have_waiting = 1;
    generation++;
    pthread_cond_signal(&cond);
    pthread_mutex_unlock(&lock);
}

static void finish(void)
{
    pthread_mutex_lock(&lock);
    closed = 1;
    generation++;
    pthread_cond_signal(&cond);
    pthread_mutex_unlock(&lock);
}

// mpv's JSON string at s (after its opening quote), unescaped into dst
static void json_unescape(const char *s, char *dst, size_t n)
{
    size_t j = 0;
    while (*s && *s != '"' && j + 4 < n) {
        if (*s != '\\') { dst[j++] = *s++; continue; }
        s++;
        char c = *s++;
        switch (c) {
        case 'n': dst[j++] = '\n'; break;
        case 't': dst[j++] = '\t'; break;
        case 'r': dst[j++] = '\r'; break;
        case 'b': dst[j++] = '\b'; break;
        case 'f': dst[j++] = '\f'; break;
        case 'u': {
            unsigned u = 0;
            sscanf(s, "%4x", &u);
            s += 4;
            if (u >= 0xd800 && u < 0xdc00 && s[0] == '\\' && s[1] == 'u') {
                unsigned lo = 0;
                sscanf(s + 2, "%4x", &lo);
                s += 6;
                u = 0x10000 + ((u - 0xd800) << 10) + (lo - 0xdc00);
            }
            if (u < 0x80) dst[j++] = (char)u;
            else if (u < 0x800) { dst[j++] = (char)(0xc0 | u >> 6); dst[j++] = (char)(0x80 | (u & 63)); }
            else if (u < 0x10000) { dst[j++] = (char)(0xe0 | u >> 12); dst[j++] = (char)(0x80 | (u >> 6 & 63)); dst[j++] = (char)(0x80 | (u & 63)); }
            else { dst[j++] = (char)(0xf0 | u >> 18); dst[j++] = (char)(0x80 | (u >> 12 & 63)); dst[j++] = (char)(0x80 | (u >> 6 & 63)); dst[j++] = (char)(0x80 | (u & 63)); }
            break;
        }
        default: dst[j++] = c;
        }
    }
    dst[j] = 0;
}

static void *read_stdin(void *arg)
{
    (void)arg;
    char line[LINE];
    while (fgets(line, sizeof line, stdin)) submit(line);
    finish();
    return NULL;
}

#ifndef _WIN32
static long want_pid = 0;

// mpv's events, a line each: the request property's changes are requests
static void *read_ipc(void *arg)
{
    (void)arg;
    static char buf[4 * LINE];
    size_t have = 0;
    for (;;) {
        ssize_t k = read(ipc_fd, buf + have, sizeof buf - 1 - have);
        if (k < 0 && errno == EINTR) continue;
        if (k <= 0) break;
        have += (size_t)k;
        buf[have] = 0;
        char *start = buf, *nl;
        while ((nl = memchr(start, '\n', have - (size_t)(start - buf)))) {
            *nl = 0;
            if (strstr(start, "\"request_id\":2") && want_pid) {
                const char *d = strstr(start, "\"data\":");
                if (d && atol(d + 7) != want_pid) {
                    fprintf(stderr, "afp-scrub: the socket is another mpv's (pid %ld, not %ld)\n", atol(d + 7), want_pid);
                    exit(3);
                }
            } else if (strstr(start, "\"property-change\"") && strstr(start, "user-data/afp-scrub/request")) {
                const char *d = strstr(start, "\"data\":\"");
                if (d) {
                    char line[LINE];
                    json_unescape(d + 8, line, sizeof line);
                    submit(line);
                }
            }
            start = nl + 1;
        }
        have -= (size_t)(start - buf);
        memmove(buf, start, have);
        if (have >= sizeof buf - 1) have = 0;
    }
    finish();
    return NULL;
}

static int ipc_connect(const char *path)
{
    int fd = socket(AF_UNIX, SOCK_STREAM, 0);
    struct sockaddr_un a = { .sun_family = AF_UNIX };
    snprintf(a.sun_path, sizeof a.sun_path, "%s", path);
    if (fd < 0 || connect(fd, (struct sockaddr *)&a, sizeof a) < 0) return -1;
    return fd;
}
#endif

// ---------- the files ----------

typedef struct {
    char path[2048];
    FILE *f;
    AVIOContext *io;
    AVFormatContext *fc;
    AVCodecContext *cc;
    int vs, exact, hw;
    double duration, fdur, start;
    // The last frame decoded (exact: the decoder goes on from it), and the one before it, kept
    AVFrame *cur, *prev;
    int64_t cur_idx;
    double used;
} Src;

static Src *srcs[MAX_SRC];

static int io_read(void *o, uint8_t *buf, int n)
{
    size_t k = fread(buf, 1, (size_t)n, ((Src *)o)->f);
    return k > 0 ? (int)k : AVERROR_EOF;
}

static int64_t io_seek(void *o, int64_t off, int whence)
{
    FILE *f = ((Src *)o)->f;
    if (whence == AVSEEK_SIZE) {
        int64_t at = ftello(f);
        fseeko(f, 0, SEEK_END);
        int64_t size = ftello(f);
        fseeko(f, at, SEEK_SET);
        return size;
    }
    if (fseeko(f, off, whence & ~AVSEEK_FORCE) < 0) return AVERROR(errno);
    return ftello(f);
}

static enum AVPixelFormat pick_hw(AVCodecContext *c, const enum AVPixelFormat *f)
{
    for (const enum AVPixelFormat *p = f; *p != AV_PIX_FMT_NONE; p++)
        if (*p == AV_PIX_FMT_VIDEOTOOLBOX || *p == AV_PIX_FMT_D3D11 || *p == AV_PIX_FMT_VAAPI) return *p;
    return c->sw_pix_fmt != AV_PIX_FMT_NONE ? c->sw_pix_fmt : f[0];
}

static void src_close(Src *s)
{
    if (!s) return;
    av_frame_free(&s->cur);
    av_frame_free(&s->prev);
    avcodec_free_context(&s->cc);
    avformat_close_input(&s->fc);
    if (s->io) { av_freep(&s->io->buffer); avio_context_free(&s->io); }
    if (s->f) fclose(s->f);
    free(s);
}

// Opened for a kind of decode (exact: CPU, frame threads; key: the GPU's decoder), kept for the next request
static Src *src_get(const char *path, int exact, char *err)
{
    int lru = 0;
    for (int i = 0; i < MAX_SRC; i++) {
        if (srcs[i] && srcs[i]->exact == exact && !strcmp(srcs[i]->path, path)) { srcs[i]->used = now_ms(); return srcs[i]; }
        if (!srcs[i] || (srcs[lru] && srcs[i]->used < srcs[lru]->used)) lru = i;
    }
    src_close(srcs[lru]);
    srcs[lru] = NULL;
    Src *s = calloc(1, sizeof *s);
    snprintf(s->path, sizeof s->path, "%s", path);
    s->exact = exact;
    s->f = fopen(path, "rb");
    if (!s->f) { snprintf(err, 256, "can't open: %s", strerror(errno)); free(s); return NULL; }
    setvbuf(s->f, NULL, _IONBF, 0);
    s->io = avio_alloc_context(av_malloc(IO_BUF), IO_BUF, 0, s, io_read, NULL, io_seek);
    s->fc = avformat_alloc_context();
    s->fc->pb = s->io;
    if (avformat_open_input(&s->fc, path, NULL, NULL) < 0) { snprintf(err, 256, "not a video"); src_close(s); return NULL; }
    avformat_find_stream_info(s->fc, NULL);
    s->vs = av_find_best_stream(s->fc, AVMEDIA_TYPE_VIDEO, -1, -1, NULL, 0);
    if (s->vs < 0) { snprintf(err, 256, "no video"); src_close(s); return NULL; }
    AVStream *st = s->fc->streams[s->vs];
    for (unsigned i = 0; i < s->fc->nb_streams; i++)
        if ((int)i != s->vs) s->fc->streams[i]->discard = AVDISCARD_ALL;
    s->duration = s->fc->duration > 0 ? s->fc->duration / (double)AV_TIME_BASE : 0;
    s->start = s->fc->start_time != AV_NOPTS_VALUE ? s->fc->start_time / (double)AV_TIME_BASE : 0;
    AVRational fr = st->avg_frame_rate.num > 0 ? st->avg_frame_rate : st->r_frame_rate;
    s->fdur = fr.num > 0 ? av_q2d(av_inv_q(fr)) : 1 / 30.0;
    const AVCodec *dec = avcodec_find_decoder(st->codecpar->codec_id);
    if (!dec) { snprintf(err, 256, "no decoder"); src_close(s); return NULL; }
    s->cc = avcodec_alloc_context3(dec);
    avcodec_parameters_to_context(s->cc, st->codecpar);
    s->cc->pkt_timebase = st->time_base;
    if (exact) {
        // Frame threads: through a GOP of the small H.264 3x faster than slice threads
        s->cc->thread_count = 0;
        s->cc->thread_type = FF_THREAD_FRAME;
    } else {
        s->cc->thread_count = 0;
        s->cc->thread_type = FF_THREAD_SLICE;
        static const enum AVHWDeviceType types[] = { AV_HWDEVICE_TYPE_VIDEOTOOLBOX, AV_HWDEVICE_TYPE_D3D11VA, AV_HWDEVICE_TYPE_VAAPI };
        for (size_t i = 0; i < sizeof types / sizeof *types && !s->hw; i++) {
            AVBufferRef *dev = NULL;
            if (av_hwdevice_ctx_create(&dev, types[i], NULL, NULL, 0) == 0) {
                s->cc->hw_device_ctx = dev;
                s->cc->get_format = pick_hw;
                s->hw = 1;
            }
        }
    }
    if (avcodec_open2(s->cc, dec, NULL) < 0) { snprintf(err, 256, "decoder failed"); src_close(s); return NULL; }
    s->cur = av_frame_alloc();
    s->prev = av_frame_alloc();
    s->cur_idx = INT64_MIN;
    s->used = now_ms();
    srcs[lru] = s;
    return s;
}

// A frame's number from the file's start (its time, in frames, rounded)
static int64_t frame_idx(Src *s, AVFrame *f)
{
    int64_t pts = f->best_effort_timestamp != AV_NOPTS_VALUE ? f->best_effort_timestamp : f->pts;
    if (pts == AV_NOPTS_VALUE) return INT64_MIN;
    double t = pts * av_q2d(s->fc->streams[s->vs]->time_base) - s->start;
    return llround(t / s->fdur);
}

static void seek_to(Src *s, double t)
{
    AVStream *st = s->fc->streams[s->vs];
    int64_t ts = av_rescale_q((int64_t)((t + s->start) * AV_TIME_BASE), AV_TIME_BASE_Q, st->time_base);
    av_seek_frame(s->fc, s->vs, ts, AVSEEK_FLAG_BACKWARD);
    avcodec_flush_buffers(s->cc);
    av_frame_unref(s->cur);
    av_frame_unref(s->prev);
    s->cur_idx = INT64_MIN;
}

// The frame nearest t into s->cur (s->prev may be nearer: *out says which), decoding on from where the decoder is when
// t is just ahead. 0 ok, 1 dropped (a newer request came), -1 failed
static int decode_exact(Src *s, double t, int gen, AVFrame **out, int *walked)
{
    int64_t want = llround(t / s->fdur);
    // The one already there
    if (s->cur_idx != INT64_MIN && want == s->cur_idx) { *out = s->cur; *walked = 1; return 0; }
    if (s->cur_idx != INT64_MIN && s->prev->buf[0] && want == frame_idx(s, s->prev)) { *out = s->prev; *walked = 1; return 0; }
    *walked = s->cur_idx != INT64_MIN && want > s->cur_idx && (want - s->cur_idx) * s->fdur < WALK;
    if (!*walked) seek_to(s, t);
    AVPacket *pkt = av_packet_alloc();
    AVFrame *fr = av_frame_alloc();
    int eof = 0, result = -1;
    for (;;) {
        if (gen != generation) { result = 1; break; }
        int r = avcodec_receive_frame(s->cc, fr);
        if (r == 0) {
            int64_t idx = frame_idx(s, fr);
            av_frame_unref(s->prev);
            av_frame_move_ref(s->prev, s->cur);
            av_frame_move_ref(s->cur, fr);
            s->cur_idx = idx;
            if (idx >= want) {
                // The nearer of this one and the one before (the sidecar may have half the clip's frame rate)
                *out = s->cur;
                if (idx > want && s->prev->buf[0] && want - frame_idx(s, s->prev) < idx - want) *out = s->prev;
                result = 0;
                break;
            }
            continue;
        }
        if (r == AVERROR_EOF) {
            // Past the end: its last frame
            if (s->cur->buf[0]) { *out = s->cur; result = 0; }
            break;
        }
        if (eof) break;
        if (av_read_frame(s->fc, pkt) < 0) {
            avcodec_send_packet(s->cc, NULL);
            eof = 1;
            continue;
        }
        if (pkt->stream_index == s->vs) avcodec_send_packet(s->cc, pkt);
        av_packet_unref(pkt);
    }
    if (eof) {
        // Drained: it takes a flush (and a seek) to decode again
        avcodec_flush_buffers(s->cc);
        s->cur_idx = INT64_MIN;
    }
    av_packet_free(&pkt);
    av_frame_free(&fr);
    return result;
}

// The keyframe at or before t, alone: its packet, then a flush
static int decode_key(Src *s, double t, AVFrame *fr, int64_t *key)
{
    seek_to(s, t);
    AVPacket *pkt = av_packet_alloc();
    int got = 0;
    while (!got && av_read_frame(s->fc, pkt) >= 0) {
        if (pkt->stream_index == s->vs) {
            *key = pkt->pos;
            avcodec_send_packet(s->cc, pkt);
            avcodec_send_packet(s->cc, NULL);
            got = avcodec_receive_frame(s->cc, fr) == 0;
            avcodec_flush_buffers(s->cc);
        }
        av_packet_unref(pkt);
    }
    av_packet_free(&pkt);
    return got ? 0 : -1;
}

// The keyframe's file position at or before t, from the file's index (no read): its picture kept is the answer
static int64_t key_pos(Src *s, double t)
{
    AVStream *st = s->fc->streams[s->vs];
    int64_t ts = av_rescale_q((int64_t)((t + s->start) * AV_TIME_BASE), AV_TIME_BASE_Q, st->time_base);
    const AVIndexEntry *e = avformat_index_get_entry_from_timestamp(st, ts, AVSEEK_FLAG_BACKWARD);
    return e ? e->pos : -1;
}

// ---------- the pictures kept ----------

typedef struct Pic {
    char path[2048];
    int exact, w, h;
    int64_t key;
    double t;
    uint8_t *bgra;
    double used;
    struct Pic *next;
} Pic;

static Pic *pics = NULL;
static size_t pics_bytes = 0, pics_cap = 160u << 20;

static Pic *pic_find(const char *path, int exact, int64_t key, int w, int h)
{
    for (Pic *p = pics; p; p = p->next)
        if (p->key == key && p->exact == exact && p->w == w && p->h == h && !strcmp(p->path, path)) { p->used = now_ms(); return p; }
    return NULL;
}

static void pic_add(const char *path, int exact, int64_t key, double t, int w, int h, const uint8_t *bgra)
{
    size_t n = (size_t)w * h * 4;
    if (n > pics_cap) return;
    while (pics && pics_bytes + n > pics_cap) {
        Pic **oldest = &pics;
        for (Pic **p = &pics; *p; p = &(*p)->next) if ((*p)->used < (*oldest)->used) oldest = p;
        Pic *o = *oldest;
        *oldest = o->next;
        pics_bytes -= (size_t)o->w * o->h * 4;
        free(o->bgra);
        free(o);
    }
    Pic *p = calloc(1, sizeof *p);
    snprintf(p->path, sizeof p->path, "%s", path);
    p->exact = exact; p->key = key; p->t = t; p->w = w; p->h = h; p->used = now_ms();
    p->bgra = malloc(n);
    memcpy(p->bgra, bgra, n);
    p->next = pics;
    pics = p;
    pics_bytes += n;
}

// ---------- a picture ----------

static struct SwsContext *sws = NULL;
static uint8_t *bgra = NULL;
static size_t bgra_n = 0;

static int scale(AVFrame *src_frame, int w, int h)
{
    AVFrame *sw = NULL, *src = src_frame;
    if (src_frame->hw_frames_ctx) {
        sw = av_frame_alloc();
        if (av_hwframe_transfer_data(sw, src_frame, 0) < 0) { av_frame_free(&sw); return -1; }
        sw->colorspace = src_frame->colorspace;
        sw->color_range = src_frame->color_range;
        src = sw;
    }
    size_t n = (size_t)w * h * 4;
    if (n > bgra_n) { free(bgra); bgra = malloc(n); bgra_n = n; }
    sws = sws_getCachedContext(sws, src->width, src->height, src->format, w, h, AV_PIX_FMT_BGRA, SWS_BILINEAR, NULL, NULL, NULL);
    if (!sws) { av_frame_free(&sw); return -1; }
    // The file's matrix and range (BT.709 for the cameras' HD and 4K; else what it says)
    int cs = src->colorspace == AVCOL_SPC_BT2020_NCL || src->colorspace == AVCOL_SPC_BT2020_CL ? SWS_CS_BT2020
           : src->colorspace == AVCOL_SPC_BT470BG || src->colorspace == AVCOL_SPC_SMPTE170M ? SWS_CS_ITU601 : SWS_CS_ITU709;
    int full = src->color_range == AVCOL_RANGE_JPEG || src->format == AV_PIX_FMT_YUVJ420P;
    sws_setColorspaceDetails(sws, sws_getCoefficients(cs), full, sws_getCoefficients(SWS_CS_DEFAULT), 1, 0, 1 << 16, 1 << 16);
    uint8_t *dst[4] = { bgra };
    int ls[4] = { w * 4 };
    sws_scale(sws, (const uint8_t *const *)src->data, src->linesize, 0, src->height, dst, ls);
    av_frame_free(&sw);
    return 0;
}

static int write_out(const char *path, const uint8_t *data, size_t n)
{
    char tmp[2100];
    snprintf(tmp, sizeof tmp, "%s.tmp", path);
    FILE *f = fopen(tmp, "wb");
    if (!f) return -1;
    size_t k = fwrite(data, 1, n, f);
    if (fclose(f) != 0 || k != n) { remove(tmp); return -1; }
#ifdef _WIN32
    remove(path);
#endif
    return rename(tmp, path);
}

// A file's duration: from one of the files open, else from its header alone (no decoder: a clip's hardware decoder
// costs ~0.1 s to set up, and the sidecar's answer doesn't need it). Kept for the clips asked about
static struct { char path[2048]; double d; } durs[8];
static int durs_next = 0;

static double dur_of(const char *path)
{
    for (int i = 0; i < MAX_SRC; i++)
        if (srcs[i] && !strcmp(srcs[i]->path, path)) return srcs[i]->duration;
    for (int i = 0; i < 8; i++)
        if (durs[i].path[0] && !strcmp(durs[i].path, path)) return durs[i].d;
    Src tmp = { 0 };
    double d = -1;
    tmp.f = fopen(path, "rb");
    if (tmp.f) {
        setvbuf(tmp.f, NULL, _IONBF, 0);
        AVIOContext *io = avio_alloc_context(av_malloc(IO_BUF), IO_BUF, 0, &tmp, io_read, NULL, io_seek);
        AVFormatContext *fc = avformat_alloc_context();
        fc->pb = io;
        if (avformat_open_input(&fc, path, NULL, NULL) == 0) {
            // The header's: the streams' own lengths (the container's is only worked out by find_stream_info)
            if (fc->duration > 0) d = fc->duration / (double)AV_TIME_BASE;
            for (unsigned i = 0; i < fc->nb_streams; i++) {
                AVStream *st = fc->streams[i];
                if (st->duration > 0 && st->duration != AV_NOPTS_VALUE && st->duration * av_q2d(st->time_base) > d)
                    d = st->duration * av_q2d(st->time_base);
            }
            avformat_close_input(&fc);
        } else {
            avformat_free_context(fc);
        }
        av_freep(&io->buffer);
        avio_context_free(&io);
        fclose(tmp.f);
    }
    snprintf(durs[durs_next].path, sizeof durs[0].path, "%s", path);
    durs[durs_next].d = d;
    durs_next = (durs_next + 1) % 8;
    return d;
}

static void serve(Request *r, int gen)
{
    double a = now_ms();
    char err[256] = "";
    const char *file = r->file;
    int exact = r->exact;
    // A sidecar whose length isn't the clip's isn't one: the clip's keyframe instead
    if (exact && r->clip[0]) {
        Src *s = src_get(r->file, 1, err);
        double d = s ? s->duration : -1, dc = dur_of(r->clip);
        if (!s || dc <= 0 || d <= 0 || d - dc > 0.1 || dc - d > 0.1) { file = r->clip; exact = 0; }
    }
    Src *s = src_get(file, exact, err);
    if (!s) { reply("%s\terror\t%s", r->id, err); return; }
    if (r->open) { reply("%s\topened\t%.1f", r->id, now_ms() - a); return; }
    size_t n = (size_t)r->w * r->h * 4;
    if (exact) {
        int64_t want = llround(r->t / s->fdur);
        Pic *p = pic_find(file, 1, want, r->w, r->h);
        if (p) {
            if (write_out(r->out, p->bgra, n) < 0) { reply("%s\terror\tcan't write", r->id); return; }
            reply("%s\tok\t%.1f\tcache\t%.3f", r->id, now_ms() - a, p->t);
            return;
        }
        AVFrame *f = NULL;
        int walked = 0;
        int got = decode_exact(s, r->t, gen, &f, &walked);
        if (got == 1) { reply("%s\tdropped", r->id); return; }
        if (got < 0 || scale(f, r->w, r->h) < 0) { reply("%s\terror\tno frame", r->id); return; }
        int64_t idx = frame_idx(s, f);
        double ft = idx * s->fdur;
        pic_add(file, 1, want, ft, r->w, r->h, bgra);
        // Its own number too: the times either side of it give the same picture
        if (idx != want && !pic_find(file, 1, idx, r->w, r->h)) pic_add(file, 1, idx, ft, r->w, r->h, bgra);
        if (write_out(r->out, bgra, n) < 0) { reply("%s\terror\tcan't write", r->id); return; }
        reply("%s\tok\t%.1f\t%s\t%.3f", r->id, now_ms() - a, walked ? "walk" : "exact", ft);
        return;
    }
    int64_t pos = key_pos(s, r->t);
    Pic *p = pos >= 0 ? pic_find(file, 0, pos, r->w, r->h) : NULL;
    if (p) {
        if (write_out(r->out, p->bgra, n) < 0) { reply("%s\terror\tcan't write", r->id); return; }
        reply("%s\tok\t%.1f\tcache\t%.3f", r->id, now_ms() - a, p->t);
        return;
    }
    AVFrame *fr = av_frame_alloc();
    int64_t key = -1;
    if (decode_key(s, r->t, fr, &key) < 0 || scale(fr, r->w, r->h) < 0) {
        av_frame_free(&fr);
        reply("%s\terror\tno frame", r->id);
        return;
    }
    int64_t pts = fr->best_effort_timestamp != AV_NOPTS_VALUE ? fr->best_effort_timestamp : fr->pts;
    double ft = pts != AV_NOPTS_VALUE ? pts * av_q2d(s->fc->streams[s->vs]->time_base) - s->start : r->t;
    av_frame_free(&fr);
    pic_add(file, 0, pos >= 0 ? pos : key, ft, r->w, r->h, bgra);
    if (write_out(r->out, bgra, n) < 0) { reply("%s\terror\tcan't write", r->id); return; }
    reply("%s\tok\t%.1f\tkey\t%.3f", r->id, now_ms() - a, ft);
}

int main(int argc, char **argv)
{
    const char *ipc = NULL;
    for (int i = 1; i < argc; i++) {
        if (!strcmp(argv[i], "--ipc") && i + 1 < argc) ipc = argv[++i];
#ifndef _WIN32
        else if (!strcmp(argv[i], "--pid") && i + 1 < argc) want_pid = atol(argv[++i]);
#endif
        else if (!strcmp(argv[i], "--to") && i + 1 < argc) reply_to = argv[++i];
        else if (!strcmp(argv[i], "--cache-mb") && i + 1 < argc) pics_cap = (size_t)atol(argv[++i]) << 20;
        else if (!strcmp(argv[i], "--version")) { printf("afp-scrub 1 (%s)\n", LIBAVCODEC_IDENT); return 0; }
        else { fprintf(stderr, "usage: afp-scrub [--ipc <mpv socket> [--pid N] [--to <script>]] [--cache-mb N]\n"); return 2; }
    }
    av_log_set_level(AV_LOG_ERROR);
    pthread_t reader;
#ifndef _WIN32
    signal(SIGPIPE, SIG_IGN);
    if (ipc) {
        ipc_fd = ipc_connect(ipc);
        if (ipc_fd < 0) { fprintf(stderr, "afp-scrub: can't connect to %s\n", ipc); return 1; }
        const char *hello = "{\"command\":[\"observe_property\",1,\"user-data/afp-scrub/request\"]}\n"
                            "{\"command\":[\"get_property\",\"pid\"],\"request_id\":2}\n";
        send_all(ipc_fd, hello, strlen(hello));
        pthread_create(&reader, NULL, read_ipc, NULL);
    } else
#endif
    {
        if (ipc) { fprintf(stderr, "afp-scrub: --ipc isn't built for this system yet\n"); return 2; }
        pthread_create(&reader, NULL, read_stdin, NULL);
    }
    for (;;) {
        pthread_mutex_lock(&lock);
        while (!have_waiting && !closed) pthread_cond_wait(&cond, &lock);
        if (!have_waiting && closed) { pthread_mutex_unlock(&lock); break; }
        Request r = waiting;
        have_waiting = 0;
        int gen = generation;
        pthread_mutex_unlock(&lock);
        serve(&r, gen);
    }
    for (int i = 0; i < MAX_SRC; i++) src_close(srcs[i]);
    return 0;
}
