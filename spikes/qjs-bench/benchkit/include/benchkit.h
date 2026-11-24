#ifndef BENCHKIT_H
#define BENCHKIT_H

#include <stdint.h>

// Runs one measurement on a fresh thread and returns one JSON line. Free it
// with benchkit_free. See benchkit/src/lib.rs for the engines and modes.
char *benchkit_run(const char *engine, const char *mode, const char *resources, uint32_t count);

void benchkit_free(char *line);

// The wasm2c variant the library was built with, such as "wasm2c-bounds-O2".
const char *benchkit_variant(void);

#endif
