// The 360 stamp's probe (vo=gpu-next): reads the stamp vs-gyroflow writes into the bottom of the luma plane
// (vs-gyroflow/src/stamp.rs, docs/360-cameras.md "The stamp") the way the view shader is to read it, with texel fetches
// of the raw plane, scaled by its black and white references (gpu-next normalizes some formats a little off),
// decodes it, and paints what it decoded over the picture as a grid of bits, for
// `tools/stamp360/stamp360.py check` to read from a screenshot and compare with what the plugin wrote. Not part of the player.
//
// The grid: 8x8 pixels a bit, white 1, black 0, a 32-bit word a row, the most significant bit first, from (32, 32):
//   0      0xF0F0F0F0: the probe ran
//   1      bit 31 the magic, 30 the check adds up, 29 every block's four pixels alike; bits 16-23 the version,
//          8-15 the bits per fine code, 0-7 the depth (all as read)
//   2, 3   the time in µs, the upper and lower 32 bits
//   4-7    q_world_from_cam w, x, y, z, as fixed point
//   8-11   the heading modes' yaws
//   12-15  their confidences
//   16     the check, as read
//   17-33  the codes, two a row (the first in the upper half): round((v - black) / (white - black) * (2^depth - 1))
//   34-67  each code's normalized value as read (v, the float's bits)

//!HOOK LUMA
//!BIND HOOKED
//!DESC 360 stamp probe

#define CODES 34
#define COARSE 7
#define BLACK 7
#define WHITE 8
#define HEADER 9
#define CELL 8
#define ORIGIN ivec2(32, 32)
#define ROWS 68

float code_value(int i, out bool alike) {
    ivec2 at = ivec2(2 * i, int(HOOKED_size.y) - 2);
    float v = texelFetch(HOOKED_raw, at, 0).x * HOOKED_mul;
    alike = v == texelFetch(HOOKED_raw, at + ivec2(1, 0), 0).x * HOOKED_mul
         && v == texelFetch(HOOKED_raw, at + ivec2(0, 1), 0).x * HOOKED_mul
         && v == texelFetch(HOOKED_raw, at + ivec2(1, 1), 0).x * HOOKED_mul;
    return v;
}

uint coarse(float v) { return uint(clamp(floor(v * 16.0), 0.0, 15.0)); }

uint word(int row) {
    float v[CODES];
    bool all_alike = true;
    for (int i = 0; i < CODES; i++) {
        bool alike;
        v[i] = code_value(i, alike);
        all_alike = all_alike && alike;
    }
    if (row >= 34)
        return floatBitsToUint(v[row - 34]);

    bool magic = coarse(v[0]) == 12u && coarse(v[1]) == 3u && coarse(v[2]) == 9u && coarse(v[3]) == 6u;
    uint depth = coarse(v[4]) + 1u, bits = coarse(v[5]) + 1u, version = coarse(v[6]);
    uint d = magic ? depth : 16u;
    float scale = float((1u << d) - 1u);
    float black = v[BLACK], range = v[WHITE] - v[BLACK];
    if (!(range > 0.5)) {  // no stamp: as read
        black = 0.0;
        range = 1.0;
    }
    uint code[CODES];
    for (int i = 0; i < CODES; i++)
        code[i] = uint(round(clamp((v[i] - black) / range, 0.0, 1.0) * scale));
    if (row >= 17)
        return (code[2 * (row - 17)] << 16) | code[2 * (row - 17) + 1];

    // The fine codes' levels
    uint s = d - min(bits, d);
    uint level[CODES];
    for (int i = 0; i < CODES; i++)
        level[i] = code[i] >> s;
    uint mask = (1u << bits) - 1u;
    uint sum = 0u;
    for (int i = HEADER; i < CODES - 1; i++)
        sum += level[i];
    bool check = (sum & mask) == level[CODES - 1];

    if (row == 0) return 0xF0F0F0F0u;
    if (row == 1) return (magic ? 1u << 31 : 0u) | (check ? 1u << 30 : 0u) | (all_alike ? 1u << 29 : 0u)
                         | (version << 16) | (bits << 8) | depth;
    if (row == 2 || row == 3) {
        // 4 codes into 64 bits
        uint hi = 0u, lo = 0u;
        for (int i = HEADER; i < HEADER + 4; i++) {
            hi = (hi << bits) | (lo >> (32u - bits));
            lo = (lo << bits) | level[i];
        }
        return row == 2 ? hi : lo;
    }
    if (row < 8) {
        int i = HEADER + 4 + 2 * (row - 4);
        return (level[i] << bits) | level[i + 1];
    }
    if (row < 12) {
        int i = HEADER + 12 + 3 * (row - 8);
        return (level[i] << bits) | level[i + 1];
    }
    if (row < 16)
        return level[HEADER + 12 + 3 * (row - 12) + 2];
    return level[CODES - 1];
}

vec4 hook() {
    ivec2 p = ivec2(HOOKED_pos * HOOKED_size) - ORIGIN;
    if (p.x < 0 || p.y < 0 || p.x >= 32 * CELL || p.y >= ROWS * CELL)
        return HOOKED_texOff(0);
    uint w = word(p.y / CELL);
    uint bit = (w >> uint(31 - p.x / CELL)) & 1u;
    return vec4(float(bit));
}
