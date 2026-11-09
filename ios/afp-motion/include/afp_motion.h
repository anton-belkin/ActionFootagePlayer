// The iPhone app's motion side (ios/afp-motion, Rust): the clip's warp as Metal source and its table
#pragma once
#include <stdint.h>

typedef struct AfpWarp {
    char *msl;      // the Metal source, or NULL with error set
    char *error;
    char *note;     // why it plays unstabilized, when it does
    float *table;   // rgba32f texels, table_w x table_h
    uint32_t table_w, table_h;
    uint32_t src_w, src_h, out_w, out_h;
    double fps;
    uint32_t frames;
} AfpWarp;

AfpWarp *afp_warp_build(const char *json);
void afp_warp_free(AfpWarp *w);
char *afp_plain_msl(void);
void afp_string_free(char *s);
// The mpv user shader for the player's JSON written to out (gyroflow-gpu-table --player); the reply as JSON
char *afp_mpv_shader(const char *json, const char *out);
