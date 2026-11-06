//!HOOK LUMA
//!BIND HOOKED
//!WIDTH OUTPUT.w
//!HEIGHT OUTPUT.h
//!DESC floor luma
vec4 hook() { return HOOKED_tex(HOOKED_pos); }

//!HOOK CHROMA
//!BIND HOOKED
//!WIDTH OUTPUT.w
//!HEIGHT OUTPUT.h
//!DESC floor chroma
vec4 hook() { return HOOKED_tex(HOOKED_pos); }
