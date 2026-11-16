// AMD FidelityFX Super Resolution 1 (FSR 1.0.2): EASU, the edge-adaptive upscale, then RCAS, the sharpening, as an
// mpv/libplacebo user shader on the picture's RGB. The gyroflow plugin puts it last in glsl-shaders while the GPU
// path renders smaller than the video shows (docs/gpu-render.md, "The upscale to the screen: FSR 1"): after the warp,
// the edge fill, the denoise and the look, it takes the picture from the size it was rendered at to the size of the
// video's rectangle on screen (at most twice as wide and high; mpv scales what's left), in place of mpv's own scaler.
// It does nothing when the picture is already that size or bigger, or with afp_fsr 0 (gyroflow.conf upscaler=mpv).
//
// Ported from AMD's ffx_fsr1.h (FsrEasuF, FsrRcasF: the RGB versions, luma for the analysis taken from RGB as AMD
// does, the taps and the sharpening applied to each channel), with the GLSL layout of agyild's mpv port
// (FSR.glsl, MIT; a luma-only port). Changes from AMD's: the inputs are libplacebo's (HOOKED_pos, gather, texOff), no
// PQ conversion (below), no alpha, RCAS's divisions kept off zero.
//
// The values are coded as the look leaves them: HLG (the hdr and Dolby Vision looks), PQ (an HDR file shown as it
// is), or SDR gamma, and FSR works on them as they are. It wants a perceptual signal, not linear light: HLG's lower
// half is a square root (gamma 2, what AMD tunes for) and its upper half a logarithm; PQ is perceptual too. AMD's
// optional PQ-to-gamma-2 step (x^4 in, the fourth root out) was tried: it acts as a slightly weaker sharpening in the
// darks, which afp_fsr_sharpness already sets, for two pow per channel (docs/gpu-render.md).
//
// For a still: run this file through libplacebo (pl_mpv_user_shader_parse, as a hook) with the target at the size
// wanted; the passes (EASU into the texture AFP_FSR_EASU, then RCAS back into MAIN) run when the target is bigger than
// the image, at most twice its width and height (libplacebo's own scaler does the rest). Parameters: afp_fsr (0/1),
// afp_fsr_sharpness (RCAS in stops: 0 the strongest, 2 the weakest; AMD's range; 0.2 by default, as in the probe).
//
// Copyright (c) 2021 Advanced Micro Devices, Inc. All rights reserved.
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
// documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
// rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or substantial portions of
// the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO
// THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
//
// (AMD's notice, as in ffx_fsr1.h and in agyild's port.) The changes and this file as a whole: part of
// ActionFootagePlayer, GPL-3.0-or-later.

//!PARAM afp_fsr
//!TYPE DYNAMIC int
//!MINIMUM 0
//!MAXIMUM 1
1

//!PARAM afp_fsr_sharpness
//!TYPE DYNAMIC float
//!MINIMUM 0.0
//!MAXIMUM 2.0
0.2

//!HOOK MAIN
//!BIND HOOKED
//!SAVE AFP_FSR_EASU
//!DESC FSR 1 EASU (upscale)
//!WHEN afp_fsr OUTPUT.w MAIN.w > OUTPUT.h MAIN.h > + *
//!WIDTH OUTPUT.w MAIN.w 2 * < OUTPUT.w * 1 OUTPUT.w MAIN.w 2 * < - MAIN.w 2 * * +
//!HEIGHT OUTPUT.h MAIN.h 2 * < OUTPUT.h * 1 OUTPUT.h MAIN.h 2 * < - MAIN.h 2 * * +
//!COMPONENTS 4

// The size is the smaller of the target and twice the picture, per axis: AMD's EASU is made for up to 2x a side.

// Approximations from AMD's ffx_a.h
float APrxLoRcpF1(float a) { return uintBitsToFloat(uint(0x7ef07ebb) - floatBitsToUint(a)); }
float APrxLoRsqF1(float a) { return uintBitsToFloat(uint(0x5f347d74) - (floatBitsToUint(a) >> uint(1))); }

// One tap: its color with the kernel's weight (a Lanczos-2 approximation, stretched along the edge)
void FsrEasuTap(inout vec3 aC, inout float aW, vec2 off, vec2 dir, vec2 len, float lob, float clp, vec3 c) {
    vec2 v;
    v.x = (off.x * ( dir.x)) + (off.y * dir.y);
    v.y = (off.x * (-dir.y)) + (off.y * dir.x);
    v *= len;
    float d2 = v.x * v.x + v.y * v.y;
    d2 = min(d2, clp);
    float wB = float(2.0 / 5.0) * d2 + -1.0;
    float wA = lob * d2 + -1.0;
    wB *= wB;
    wA *= wA;
    wB = float(25.0 / 16.0) * wB + float(-(25.0 / 16.0 - 1.0));
    float w = wB * wA;
    aC += c * w;
    aW += w;
}

// The edge's direction and length around one of the four nearest texels, weighted bilinearly
void FsrEasuSet(inout vec2 dir, inout float len, vec2 pp, bool biS, bool biT, bool biU, bool biV,
                float lA, float lB, float lC, float lD, float lE) {
    float w = 0.0;
    if (biS) w = (1.0 - pp.x) * (1.0 - pp.y);
    if (biT) w =        pp.x  * (1.0 - pp.y);
    if (biU) w = (1.0 - pp.x) *        pp.y;
    if (biV) w =        pp.x  *        pp.y;
    float dc = lD - lC;
    float cb = lC - lB;
    float lenX = max(abs(dc), abs(cb));
    lenX = APrxLoRcpF1(lenX);
    float dirX = lD - lB;
    lenX = clamp(abs(dirX) * lenX, 0.0, 1.0);
    lenX *= lenX;
    float ec = lE - lC;
    float ca = lC - lA;
    float lenY = max(abs(ec), abs(ca));
    lenY = APrxLoRcpF1(lenY);
    float dirY = lE - lA;
    lenY = clamp(abs(dirY) * lenY, 0.0, 1.0);
    lenY *= lenY;
    dir += vec2(dirX, dirY) * w;
    len += dot(vec2(w), vec2(lenX, lenY));
}

vec4 hook() {
    // The 12 texels around the output pixel's position, F the top left of the nearest four:
    //    b c
    //  e F g h
    //  i j k l
    //    n o
    vec2 pp = HOOKED_pos * HOOKED_size - vec2(0.5);
    vec2 fp = floor(pp);
    pp -= fp;
#if defined(HOOKED_gather) && (__VERSION__ >= 400 || (GL_ES && __VERSION__ >= 310))
    vec2 p0 = (fp + vec2(1.0, -1.0)) * HOOKED_pt;
    vec2 p1 = (fp + vec2(0.0,  1.0)) * HOOKED_pt;
    vec2 p2 = (fp + vec2(2.0,  1.0)) * HOOKED_pt;
    vec2 p3 = (fp + vec2(1.0,  3.0)) * HOOKED_pt;
    vec4 bczzR = HOOKED_gather(p0, 0), bczzG = HOOKED_gather(p0, 1), bczzB = HOOKED_gather(p0, 2);
    vec4 ijfeR = HOOKED_gather(p1, 0), ijfeG = HOOKED_gather(p1, 1), ijfeB = HOOKED_gather(p1, 2);
    vec4 klhgR = HOOKED_gather(p2, 0), klhgG = HOOKED_gather(p2, 1), klhgB = HOOKED_gather(p2, 2);
    vec4 zzonR = HOOKED_gather(p3, 0), zzonG = HOOKED_gather(p3, 1), zzonB = HOOKED_gather(p3, 2);
    vec3 b = vec3(bczzR.x, bczzG.x, bczzB.x), c = vec3(bczzR.y, bczzG.y, bczzB.y);
    vec3 i = vec3(ijfeR.x, ijfeG.x, ijfeB.x), j = vec3(ijfeR.y, ijfeG.y, ijfeB.y);
    vec3 f = vec3(ijfeR.z, ijfeG.z, ijfeB.z), e = vec3(ijfeR.w, ijfeG.w, ijfeB.w);
    vec3 k = vec3(klhgR.x, klhgG.x, klhgB.x), l = vec3(klhgR.y, klhgG.y, klhgB.y);
    vec3 h = vec3(klhgR.z, klhgG.z, klhgB.z), g = vec3(klhgR.w, klhgG.w, klhgB.w);
    vec3 o = vec3(zzonR.z, zzonG.z, zzonB.z), n = vec3(zzonR.w, zzonG.w, zzonB.w);
#else
    vec3 b = HOOKED_tex((fp + vec2( 0.5, -0.5)) * HOOKED_pt).rgb;
    vec3 c = HOOKED_tex((fp + vec2( 1.5, -0.5)) * HOOKED_pt).rgb;
    vec3 e = HOOKED_tex((fp + vec2(-0.5,  0.5)) * HOOKED_pt).rgb;
    vec3 f = HOOKED_tex((fp + vec2( 0.5,  0.5)) * HOOKED_pt).rgb;
    vec3 g = HOOKED_tex((fp + vec2( 1.5,  0.5)) * HOOKED_pt).rgb;
    vec3 h = HOOKED_tex((fp + vec2( 2.5,  0.5)) * HOOKED_pt).rgb;
    vec3 i = HOOKED_tex((fp + vec2(-0.5,  1.5)) * HOOKED_pt).rgb;
    vec3 j = HOOKED_tex((fp + vec2( 0.5,  1.5)) * HOOKED_pt).rgb;
    vec3 k = HOOKED_tex((fp + vec2( 1.5,  1.5)) * HOOKED_pt).rgb;
    vec3 l = HOOKED_tex((fp + vec2( 2.5,  1.5)) * HOOKED_pt).rgb;
    vec3 n = HOOKED_tex((fp + vec2( 0.5,  2.5)) * HOOKED_pt).rgb;
    vec3 o = HOOKED_tex((fp + vec2( 1.5,  2.5)) * HOOKED_pt).rgb;
#endif
    // Luma times 2, as AMD's: G + (R + B) / 2
    const vec3 Y2 = vec3(0.5, 1.0, 0.5);
    float bL = dot(b, Y2), cL = dot(c, Y2), eL = dot(e, Y2), fL = dot(f, Y2), gL = dot(g, Y2), hL = dot(h, Y2);
    float iL = dot(i, Y2), jL = dot(j, Y2), kL = dot(k, Y2), lL = dot(l, Y2), nL = dot(n, Y2), oL = dot(o, Y2);

    vec2 dir = vec2(0.0);
    float len = 0.0;
    FsrEasuSet(dir, len, pp, true, false, false, false, bL, eL, fL, gL, jL);
    FsrEasuSet(dir, len, pp, false, true, false, false, cL, fL, gL, hL, kL);
    FsrEasuSet(dir, len, pp, false, false, true, false, fL, iL, jL, kL, nL);
    FsrEasuSet(dir, len, pp, false, false, false, true, gL, jL, kL, lL, oL);

    // The direction normalized (none where it's close to zero), the kernel stretched along an edge
    vec2 dir2 = dir * dir;
    float dirR = dir2.x + dir2.y;
    bool zro = dirR < float(1.0 / 32768.0);
    dirR = APrxLoRsqF1(dirR);
    dirR = zro ? 1.0 : dirR;
    dir.x = zro ? 1.0 : dir.x;
    dir *= vec2(dirR);
    len = len * 0.5;
    len *= len;
    float stretch = (dir.x * dir.x + dir.y * dir.y) * APrxLoRcpF1(max(abs(dir.x), abs(dir.y)));
    vec2 len2 = vec2(1.0 + (stretch - 1.0) * len, 1.0 + -0.5 * len);
    float lob = 0.5 + float((1.0 / 4.0 - 0.04) - 0.5) * len;
    float clp = APrxLoRcpF1(lob);

    vec3 aC = vec3(0.0);
    float aW = 0.0;
    FsrEasuTap(aC, aW, vec2( 0.0, -1.0) - pp, dir, len2, lob, clp, b);
    FsrEasuTap(aC, aW, vec2( 1.0, -1.0) - pp, dir, len2, lob, clp, c);
    FsrEasuTap(aC, aW, vec2(-1.0,  1.0) - pp, dir, len2, lob, clp, i);
    FsrEasuTap(aC, aW, vec2( 0.0,  1.0) - pp, dir, len2, lob, clp, j);
    FsrEasuTap(aC, aW, vec2( 0.0,  0.0) - pp, dir, len2, lob, clp, f);
    FsrEasuTap(aC, aW, vec2(-1.0,  0.0) - pp, dir, len2, lob, clp, e);
    FsrEasuTap(aC, aW, vec2( 1.0,  1.0) - pp, dir, len2, lob, clp, k);
    FsrEasuTap(aC, aW, vec2( 2.0,  1.0) - pp, dir, len2, lob, clp, l);
    FsrEasuTap(aC, aW, vec2( 2.0,  0.0) - pp, dir, len2, lob, clp, h);
    FsrEasuTap(aC, aW, vec2( 1.0,  0.0) - pp, dir, len2, lob, clp, g);
    FsrEasuTap(aC, aW, vec2( 1.0,  2.0) - pp, dir, len2, lob, clp, o);
    FsrEasuTap(aC, aW, vec2( 0.0,  2.0) - pp, dir, len2, lob, clp, n);

    // No ringing: within the nearest four's range, per channel
    vec3 mn = min(min(f, g), min(j, k));
    vec3 mx = max(max(f, g), max(j, k));
    return vec4(min(mx, max(mn, aC / aW)), 1.0);
}

//!HOOK MAIN
//!BIND AFP_FSR_EASU
//!DESC FSR 1 RCAS (sharpen)
//!WHEN afp_fsr OUTPUT.w MAIN.w > OUTPUT.h MAIN.h > + *
//!WIDTH AFP_FSR_EASU.w
//!HEIGHT AFP_FSR_EASU.h
//!COMPONENTS 4

// AMD's limit, where the sharpening starts to look unnatural
#define FSR_RCAS_LIMIT (0.25 - (1.0 / 16.0))

float APrxMedRcpF1(float a) {
    float b = uintBitsToFloat(uint(0x7ef19fff) - floatBitsToUint(a));
    return b * (-b * a + 2.0);
}
float AMax3F1(float x, float y, float z) { return max(x, max(y, z)); }
float AMin3F1(float x, float y, float z) { return min(x, min(y, z)); }

vec4 hook() {
    // The pixel and its four neighbours:
    //    b
    //  d e f
    //    h
    vec3 b = AFP_FSR_EASU_texOff(vec2( 0.0, -1.0)).rgb;
    vec3 d = AFP_FSR_EASU_texOff(vec2(-1.0,  0.0)).rgb;
    vec3 e = AFP_FSR_EASU_texOff(vec2( 0.0,  0.0)).rgb;
    vec3 f = AFP_FSR_EASU_texOff(vec2( 1.0,  0.0)).rgb;
    vec3 h = AFP_FSR_EASU_texOff(vec2( 0.0,  1.0)).rgb;

    // Luma times 2, for the noise detection: less sharpening where it's noise rather than an edge
    const vec3 Y2 = vec3(0.5, 1.0, 0.5);
    float bL = dot(b, Y2), dL = dot(d, Y2), eL = dot(e, Y2), fL = dot(f, Y2), hL = dot(h, Y2);
    float nz = 0.25 * bL + 0.25 * dL + 0.25 * fL + 0.25 * hL - eL;
    nz = clamp(abs(nz) * APrxMedRcpF1(AMax3F1(AMax3F1(bL, dL, eL), fL, hL) - AMin3F1(AMin3F1(bL, dL, eL), fL, hL)), 0.0, 1.0);
    nz = -0.5 * nz + 1.0;

    // The ring's min and max per channel, and the strongest lobe that clips none of them
    vec3 mn4 = min(min(b, d), min(f, h));
    vec3 mx4 = max(max(b, d), max(f, h));
    vec2 peakC = vec2(1.0, -1.0 * 4.0);
    // (AMD's divisions, kept off zero: black and full white rings gave NaN)
    vec3 hitMin = min(mn4, e) / max(4.0 * mx4, 1e-6);
    vec3 hitMax = (peakC.x - max(mx4, e)) / min(4.0 * mn4 + peakC.y, -1e-6);
    vec3 lobeRGB = max(-hitMin, hitMax);
    float lobe = max(float(-FSR_RCAS_LIMIT), min(AMax3F1(lobeRGB.r, lobeRGB.g, lobeRGB.b), 0.0))
               * exp2(-clamp(afp_fsr_sharpness, 0.0, 2.0));
    lobe *= nz;

    float rcpL = APrxMedRcpF1(4.0 * lobe + 1.0);
    vec3 pix = (lobe * b + lobe * d + lobe * h + lobe * f + e) * rcpL;
    return vec4(clamp(pix, 0.0, 1.0), 1.0);
}
