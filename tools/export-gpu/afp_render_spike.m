// SPDX-License-Identifier: GPL-3.0-or-later
//
// Zero-copy export spike (docs/design/export-gpu-path.md, "The zero-copy spike"): the player's GPU shaders through libplacebo
// with the frame never leaving the GPU. VideoToolbox decodes (AVAssetReader) into IOSurface-backed P010 pixel buffers;
// each plane is an MTLTexture on that IOSurface, imported into Vulkan (MoltenVK, VK_EXT_metal_objects) and wrapped for
// libplacebo; libplacebo renders the shader file (PTS set per frame, as mpv's vo_gpu_next does) into the planes of a
// pixel buffer from the encoder's own pool, which VideoToolbox encodes (AVAssetWriter, HEVC Main10 HLG). Imports are
// kept per IOSurface: both pools recycle their buffers. A spike: video only, HLG only, one frame on the GPU at a time.
//
//   afp_render_spike <in.mp4> <shader.glsl> <out.mov> [--frames n] [--bitrate bps] [--fps-hint f]

#import <AVFoundation/AVFoundation.h>
#import <CoreVideo/CoreVideo.h>
#import <Metal/Metal.h>
#import <IOSurface/IOSurface.h>
#import <VideoToolbox/VideoToolbox.h>
#define VK_USE_PLATFORM_METAL_EXT
#include <vulkan/vulkan.h>
#include <libplacebo/vulkan.h>
#include <libplacebo/renderer.h>
#include <libplacebo/shaders/custom.h>
#include <libplacebo/log.h>
#include <mach/mach_time.h>

static double now(void) {
    static mach_timebase_info_data_t tb;
    if (!tb.denom) mach_timebase_info(&tb);
    return (double) mach_absolute_time() * tb.numer / tb.denom / 1e9;
}

static void die(const char *msg) { fprintf(stderr, "afp_render_spike: %s\n", msg); exit(1); }

// One IOSurface's two planes, imported once
typedef struct {
    IOSurfaceID id;
    VkImage image[2];
    pl_tex tex[2];
    id<MTLTexture> mtl[2];
} Import;

#define MAX_IMPORTS 64
static Import imports[MAX_IMPORTS];
static int num_imports;

static pl_vulkan vk;
static id<MTLDevice> mtl_device;

static Import *import_surface(CVPixelBufferRef pb, bool output) {
    IOSurfaceRef surf = CVPixelBufferGetIOSurface(pb);
    if (!surf) die("a pixel buffer without an IOSurface");
    IOSurfaceID sid = IOSurfaceGetID(surf);
    for (int i = 0; i < num_imports; i++) if (imports[i].id == sid) return &imports[i];
    if (num_imports == MAX_IMPORTS) die("too many surfaces");
    Import *im = &imports[num_imports++];
    im->id = sid;
    for (int p = 0; p < 2; p++) {
        int w = (int) CVPixelBufferGetWidthOfPlane(pb, p), h = (int) CVPixelBufferGetHeightOfPlane(pb, p);
        MTLTextureDescriptor *d = [MTLTextureDescriptor texture2DDescriptorWithPixelFormat:(p ? MTLPixelFormatRG16Unorm : MTLPixelFormatR16Unorm)
                                                                                    width:w height:h mipmapped:NO];
        d.usage = MTLTextureUsageShaderRead | (output ? (MTLTextureUsageShaderWrite | MTLTextureUsageRenderTarget) : 0);
        d.storageMode = MTLStorageModeShared;
        im->mtl[p] = [mtl_device newTextureWithDescriptor:d iosurface:surf plane:p];
        if (!im->mtl[p]) die("no MTLTexture on the IOSurface");
        VkFormat fmt = p ? VK_FORMAT_R16G16_UNORM : VK_FORMAT_R16_UNORM;
        VkImageUsageFlags usage = VK_IMAGE_USAGE_SAMPLED_BIT | VK_IMAGE_USAGE_TRANSFER_SRC_BIT;
        if (output) usage |= VK_IMAGE_USAGE_COLOR_ATTACHMENT_BIT | VK_IMAGE_USAGE_STORAGE_BIT | VK_IMAGE_USAGE_TRANSFER_DST_BIT;
        VkImportMetalTextureInfoEXT imp = { .sType = VK_STRUCTURE_TYPE_IMPORT_METAL_TEXTURE_INFO_EXT,
                                            .plane = VK_IMAGE_ASPECT_PLANE_0_BIT, .mtlTexture = im->mtl[p] };
        VkImageCreateInfo ci = { .sType = VK_STRUCTURE_TYPE_IMAGE_CREATE_INFO, .pNext = &imp, .imageType = VK_IMAGE_TYPE_2D,
                                 .format = fmt, .extent = { w, h, 1 }, .mipLevels = 1, .arrayLayers = 1,
                                 .samples = VK_SAMPLE_COUNT_1_BIT, .tiling = VK_IMAGE_TILING_OPTIMAL, .usage = usage,
                                 .sharingMode = VK_SHARING_MODE_EXCLUSIVE, .initialLayout = VK_IMAGE_LAYOUT_UNDEFINED };
        if (vkCreateImage(vk->device, &ci, NULL, &im->image[p]) != VK_SUCCESS) die("vkCreateImage with the Metal texture");
        im->tex[p] = pl_vulkan_wrap(vk->gpu, pl_vulkan_wrap_params(.image = im->image[p], .width = w, .height = h,
                                                                   .format = fmt, .usage = usage));
        if (!im->tex[p]) die("pl_vulkan_wrap");
        // libplacebo's from now on; MoltenVK doesn't move pixels on a layout change
        pl_vulkan_release_ex(vk->gpu, pl_vulkan_release_params(.tex = im->tex[p], .layout = VK_IMAGE_LAYOUT_GENERAL,
                                                               .qf = VK_QUEUE_FAMILY_IGNORED));
    }
    return im;
}

static void frame_from(struct pl_frame *f, Import *im, int w, int h, enum pl_color_system sys) {
    *f = (struct pl_frame) {
        .num_planes = 2,
        .planes = {
            { .texture = im->tex[0], .components = 1, .component_mapping = { 0 } },
            { .texture = im->tex[1], .components = 2, .component_mapping = { 1, 2 } },
        },
        // P010: 10 bits at the top of 16
        .repr = { .sys = sys, .levels = PL_COLOR_LEVELS_LIMITED, .bits = { .sample_depth = 16, .color_depth = 10, .bit_shift = 6 } },
        // Tagged HLG / BT.2020 as the dlogm plugin tags the video: the LUT's output passes through
        .color = { .primaries = PL_COLOR_PRIM_BT_2020, .transfer = PL_COLOR_TRC_HLG },
        .crop = { 0, 0, w, h },
    };
    pl_frame_set_chroma_location(f, PL_CHROMA_LEFT);
}

int main(int argc, const char **argv) {
    @autoreleasepool {
        if (argc < 4) die("usage: afp_render_spike <in.mp4> <shader.glsl> <out.mov> [--frames n] [--bitrate bps]");
        long max_frames = LONG_MAX;
        bool realtime = false; // --realtime 1: VideoToolbox's real-time mode (speed over its slower search)
        double bitrate = 89874482;
        for (int i = 4; i + 1 < argc; i += 2) {
            if (!strcmp(argv[i], "--frames")) max_frames = atol(argv[i + 1]);
            else if (!strcmp(argv[i], "--bitrate")) bitrate = atof(argv[i + 1]);
            else if (!strcmp(argv[i], "--realtime")) realtime = atoi(argv[i + 1]) != 0;
        }
        double t0 = now();

        // libplacebo on MoltenVK, with the Metal objects extension
        pl_log log = pl_log_create(PL_API_VER, pl_log_params(.log_cb = pl_log_color, .log_level = PL_LOG_WARN));
        const char *exts[] = { VK_EXT_METAL_OBJECTS_EXTENSION_NAME };
        vk = pl_vulkan_create(log, pl_vulkan_params(.extensions = exts, .num_extensions = 1));
        if (!vk) die("pl_vulkan_create");
        // The MTLDevice MoltenVK renders with
        PFN_vkExportMetalObjectsEXT export = (PFN_vkExportMetalObjectsEXT) vkGetDeviceProcAddr(vk->device, "vkExportMetalObjectsEXT");
        VkExportMetalDeviceInfoEXT dev_info = { .sType = VK_STRUCTURE_TYPE_EXPORT_METAL_DEVICE_INFO_EXT };
        VkExportMetalObjectsInfoEXT objs = { .sType = VK_STRUCTURE_TYPE_EXPORT_METAL_OBJECTS_INFO_EXT, .pNext = &dev_info };
        if (!export) die("no vkExportMetalObjectsEXT");
        export(vk->device, &objs);
        mtl_device = dev_info.mtlDevice;
        if (!mtl_device) die("no MTLDevice");
        fprintf(stderr, "GPU: %s\n", mtl_device.name.UTF8String);

        // The player's shader, as mpv loads it
        NSData *text = [NSData dataWithContentsOfFile:@(argv[2])];
        if (!text) die("shader file");
        const struct pl_hook *hook = pl_mpv_user_shader_parse(vk->gpu, text.bytes, text.length);
        if (!hook) die("the shader doesn't parse");
        pl_var_data *pts_par = NULL;
        for (int i = 0; i < hook->num_parameters; i++)
            if (!strcmp(hook->parameters[i].name, "PTS")) pts_par = hook->parameters[i].data;
        if (!pts_par) fprintf(stderr, "no PTS parameter in the shader\n");
        pl_renderer rr = pl_renderer_create(log, vk->gpu);
        struct pl_render_params params = pl_render_default_params;
        params.hooks = &hook;
        params.num_hooks = 1;
        params.dither_params = NULL; // as the filter path's dithering=none

        // Decoder: VideoToolbox into IOSurface-backed P010
        AVURLAsset *asset = [AVURLAsset URLAssetWithURL:[NSURL fileURLWithPath:@(argv[1])] options:nil];
        AVAssetTrack *track = [asset tracksWithMediaType:AVMediaTypeVideo].firstObject;
        if (!track) die("no video track");
        NSError *err = nil;
        AVAssetReader *reader = [AVAssetReader assetReaderWithAsset:asset error:&err];
        NSDictionary *pbattrs = @{ (id) kCVPixelBufferPixelFormatTypeKey: @(kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange),
                                   (id) kCVPixelBufferIOSurfacePropertiesKey: @{}, (id) kCVPixelBufferMetalCompatibilityKey: @YES };
        AVAssetReaderTrackOutput *rout = [AVAssetReaderTrackOutput assetReaderTrackOutputWithTrack:track outputSettings:pbattrs];
        rout.alwaysCopiesSampleData = NO;
        [reader addOutput:rout];
        if (![reader startReading]) die("reader");
        int w = (int) track.naturalSize.width, h = (int) track.naturalSize.height;

        // Encoder: VideoToolbox HEVC Main10, HLG, from its own pool of IOSurfaces
        [[NSFileManager defaultManager] removeItemAtPath:@(argv[3]) error:nil];
        AVAssetWriter *writer = [AVAssetWriter assetWriterWithURL:[NSURL fileURLWithPath:@(argv[3])] fileType:AVFileTypeQuickTimeMovie error:&err];
        if (!writer) die("writer");
        NSDictionary *vs = @{ AVVideoCodecKey: AVVideoCodecTypeHEVC, AVVideoWidthKey: @(w), AVVideoHeightKey: @(h),
                              AVVideoCompressionPropertiesKey: @{ AVVideoAverageBitRateKey: @(bitrate),
                                                                  AVVideoProfileLevelKey: (__bridge NSString *) kVTProfileLevel_HEVC_Main10_AutoLevel,
                                                                  AVVideoExpectedSourceFrameRateKey: @(track.nominalFrameRate),
                                                                  (__bridge NSString *) kVTCompressionPropertyKey_RealTime: @(realtime),
                                                                  (__bridge NSString *) kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality: @(realtime) },
                              AVVideoColorPropertiesKey: @{ AVVideoColorPrimariesKey: AVVideoColorPrimaries_ITU_R_2020,
                                                            AVVideoTransferFunctionKey: AVVideoTransferFunction_ITU_R_2100_HLG,
                                                            AVVideoYCbCrMatrixKey: AVVideoYCbCrMatrix_ITU_R_2020 } };
        AVAssetWriterInput *win = [AVAssetWriterInput assetWriterInputWithMediaType:AVMediaTypeVideo outputSettings:vs];
        win.expectsMediaDataInRealTime = NO;
        NSMutableDictionary *outattrs = [pbattrs mutableCopy];
        outattrs[(id) kCVPixelBufferWidthKey] = @(w);
        outattrs[(id) kCVPixelBufferHeightKey] = @(h);
        AVAssetWriterInputPixelBufferAdaptor *adaptor = [AVAssetWriterInputPixelBufferAdaptor assetWriterInputPixelBufferAdaptorWithAssetWriterInput:win
                                                                                                                       sourcePixelBufferAttributes:outattrs];
        [writer addInput:win];
        if (![writer startWriting]) die("startWriting");

        double t_setup = now() - t0, t_read = 0, t_render = 0, t_wait = 0;
        long n = 0;
        bool started = false;
        // One frame in flight: frame n renders while n - 1 finishes on the GPU and goes to the encoder
        __block struct { CMSampleBufferRef sb; CVPixelBufferRef dst; CMTime pts; Import *out; } prev = { 0 };
        __block double t_drain = 0;
        void (^drain)(void) = ^{
            if (!prev.dst) return;
            double d = now();
            for (int p = 0; p < 2; p++) while (pl_tex_poll(vk->gpu, prev.out->tex[p], UINT64_MAX)) ;
            t_drain += now() - d;
            while (!win.readyForMoreMediaData) usleep(200);
            if (![adaptor appendPixelBuffer:prev.dst withPresentationTime:prev.pts]) die("append");
            CVPixelBufferRelease(prev.dst);
            CFRelease(prev.sb);
            prev.dst = NULL;
        };
        double t_loop = now();
        while (n < max_frames) {
            @autoreleasepool {
                double a = now();
                CMSampleBufferRef sb = [rout copyNextSampleBuffer];
                if (!sb) break;
                CVPixelBufferRef src = CMSampleBufferGetImageBuffer(sb);
                CMTime pts = CMSampleBufferGetPresentationTimeStamp(sb);
                if (!src) { CFRelease(sb); continue; }
                if (!started) { [writer startSessionAtSourceTime:pts]; started = true; }
                double b = now();
                t_read += b - a;
                while (!win.readyForMoreMediaData) usleep(200);
                CVPixelBufferRef dst = NULL;
                if (CVPixelBufferPoolCreatePixelBuffer(NULL, adaptor.pixelBufferPool, &dst) != kCVReturnSuccess) die("encoder's pool");
                double c = now();
                t_wait += c - b;

                struct pl_frame in, out;
                Import *oi = import_surface(dst, true);
                frame_from(&in, import_surface(src, false), w, h, PL_COLOR_SYSTEM_BT_709);
                frame_from(&out, oi, w, h, PL_COLOR_SYSTEM_BT_2020_NC);
                if (pts_par) pts_par->f = (float) CMTimeGetSeconds(pts);
                if (!pl_render_image(rr, &in, &out, &params)) die("pl_render_image");
                pl_gpu_flush(vk->gpu);
                t_render += now() - c;
                // The frame before: in the encoder's IOSurface once the GPU is done with it; its decoder buffer may
                // be reused after
                drain();
                prev.sb = sb; prev.dst = dst; prev.pts = pts; prev.out = oi;
                n++;
            }
        }
        drain();
        [win markAsFinished];
        dispatch_semaphore_t done = dispatch_semaphore_create(0);
        double t_end = now();
        [writer finishWritingWithCompletionHandler:^{ dispatch_semaphore_signal(done); }];
        dispatch_semaphore_wait(done, DISPATCH_TIME_FOREVER);
        double loop = now() - t_loop;
        if (writer.status != AVAssetWriterStatusCompleted) die(writer.error.description.UTF8String);
        fprintf(stderr, "%ld frames in %.2f s (%.1f fps), setup %.2f s, finishing %.2f s; per frame: decode wait %.2f ms, encoder wait %.2f ms, "
                "render submit %.2f ms, GPU wait %.2f ms; %d surfaces imported\n", n, loop, n / loop, t_setup, now() - t_end, t_read * 1e3 / n,
                t_wait * 1e3 / n, t_render * 1e3 / n, t_drain * 1e3 / n, num_imports);
        pl_renderer_destroy(&rr);
    }
    return 0;
}
