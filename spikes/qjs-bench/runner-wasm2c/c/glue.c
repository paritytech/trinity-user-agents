// The guest's imports and a trap-catching wrapper around each export.
//
// Every pointer the guest hands over is checked against its linear memory
// before the host touches it; a bad one traps the guest, never the host.
// The `truapi` imports forward to the Rust host (`qjs_host_*` in main.rs).

#include <stdlib.h>
#include <string.h>
#include <time.h>

#include "qjs_guest.h"
#include "wasm-rt-exceptions.h"
#include "wasm-rt-impl.h"

struct w2c_truapi {
  void* host;
  w2c_qjs__guest* guest;
};

struct w2c_wasi__snapshot__preview1 {
  w2c_qjs__guest* guest;
};

typedef struct {
  w2c_qjs__guest guest;
  struct w2c_truapi truapi;
  struct w2c_wasi__snapshot__preview1 wasi;
} qjs_instance;

enum { ERRNO_SUCCESS = 0, ERRNO_BADF = 8, ERRNO_INVAL = 28 };

void qjs_host_frame_out(void* host, const uint8_t* bytes, size_t len);
void qjs_host_log(void* host, uint32_t level, const uint8_t* bytes, size_t len);
double qjs_host_now_ms(void* host);
void qjs_host_random_fill(uint8_t* bytes, size_t len);
void qjs_host_timer_set(void* host, uint32_t id, double delay_ms);
void qjs_host_timer_clear(void* host, uint32_t id);

static uint8_t* guest_bytes(w2c_qjs__guest* guest, u32 pointer, u32 len) {
  if ((uint64_t)pointer + len > guest->w2c_memory.size) {
    wasm_rt_trap(WASM_RT_TRAP_OOB);
  }
  return guest->w2c_memory.data + pointer;
}

void w2c_truapi_frame_out(struct w2c_truapi* ctx, u32 pointer, u32 len) {
  qjs_host_frame_out(ctx->host, guest_bytes(ctx->guest, pointer, len), len);
}

void w2c_truapi_log(struct w2c_truapi* ctx, u32 level, u32 pointer, u32 len) {
  qjs_host_log(ctx->host, level, guest_bytes(ctx->guest, pointer, len), len);
}

f64 w2c_truapi_now_ms(struct w2c_truapi* ctx) {
  return qjs_host_now_ms(ctx->host);
}

void w2c_truapi_random_fill(struct w2c_truapi* ctx, u32 pointer, u32 len) {
  qjs_host_random_fill(guest_bytes(ctx->guest, pointer, len), len);
}

void w2c_truapi_timer_set(struct w2c_truapi* ctx, u32 id, f64 delay_ms) {
  qjs_host_timer_set(ctx->host, id, delay_ms);
}

void w2c_truapi_timer_clear(struct w2c_truapi* ctx, u32 id) {
  qjs_host_timer_clear(ctx->host, id);
}

static void put_u32(w2c_qjs__guest* guest, u32 pointer, uint32_t value) {
  memcpy(guest_bytes(guest, pointer, 4), &value, 4);
}

static void put_u64(w2c_qjs__guest* guest, u32 pointer, uint64_t value) {
  memcpy(guest_bytes(guest, pointer, 8), &value, 8);
}

u32 w2c_wasi__snapshot__preview1_clock_time_get(struct w2c_wasi__snapshot__preview1* ctx, u32 clock_id,
                                                 u64 precision, u32 time_out) {
  struct timespec now;
  if (clock_id > 1 || clock_gettime(clock_id == 0 ? CLOCK_REALTIME : CLOCK_MONOTONIC, &now) != 0) {
    return ERRNO_INVAL;
  }
  put_u64(ctx->guest, time_out, (uint64_t)now.tv_sec * 1000000000u + (uint64_t)now.tv_nsec);
  return ERRNO_SUCCESS;
}

u32 w2c_wasi__snapshot__preview1_environ_sizes_get(struct w2c_wasi__snapshot__preview1* ctx, u32 count_out,
                                                    u32 size_out) {
  put_u32(ctx->guest, count_out, 0);
  put_u32(ctx->guest, size_out, 0);
  return ERRNO_SUCCESS;
}

u32 w2c_wasi__snapshot__preview1_environ_get(struct w2c_wasi__snapshot__preview1* ctx, u32 environ,
                                              u32 buffer) {
  return ERRNO_SUCCESS;
}

u32 w2c_wasi__snapshot__preview1_fd_write(struct w2c_wasi__snapshot__preview1* ctx, u32 fd, u32 iovs,
                                           u32 iovs_len, u32 written_out) {
  if (fd != 1 && fd != 2) {
    return ERRNO_BADF;
  }
  uint32_t written = 0;
  for (u32 index = 0; index < iovs_len; index++) {
    uint32_t entry[2];
    memcpy(entry, guest_bytes(ctx->guest, iovs + index * 8, 8), 8);
    qjs_host_log(NULL, 3, guest_bytes(ctx->guest, entry[0], entry[1]), entry[1]);
    written += entry[1];
  }
  put_u32(ctx->guest, written_out, written);
  return ERRNO_SUCCESS;
}

// Declared by wasi-libc and never called: there are no files.
u32 w2c_wasi__snapshot__preview1_fd_close(struct w2c_wasi__snapshot__preview1* ctx, u32 fd) {
  return ERRNO_BADF;
}

u32 w2c_wasi__snapshot__preview1_fd_fdstat_get(struct w2c_wasi__snapshot__preview1* ctx, u32 fd, u32 out) {
  return ERRNO_BADF;
}

u32 w2c_wasi__snapshot__preview1_fd_prestat_get(struct w2c_wasi__snapshot__preview1* ctx, u32 fd, u32 out) {
  return ERRNO_BADF;
}

u32 w2c_wasi__snapshot__preview1_fd_prestat_dir_name(struct w2c_wasi__snapshot__preview1* ctx, u32 fd,
                                                      u32 path, u32 len) {
  return ERRNO_BADF;
}

u32 w2c_wasi__snapshot__preview1_fd_seek(struct w2c_wasi__snapshot__preview1* ctx, u32 fd, u64 offset,
                                          u32 whence, u32 out) {
  return ERRNO_BADF;
}

void w2c_wasi__snapshot__preview1_proc_exit(struct w2c_wasi__snapshot__preview1* ctx, u32 code) {
  wasm_rt_trap(WASM_RT_TRAP_UNREACHABLE);
}

// Every function below returns the trap that stopped the call, or 0.

void qjs_runtime_init(void) {
  wasm_rt_init();
}

int qjs_new(void* host, uint64_t max_pages, qjs_instance** out) {
  qjs_instance* instance = calloc(1, sizeof(qjs_instance));
  instance->truapi.host = host;
  instance->truapi.guest = &instance->guest;
  instance->wasi.guest = &instance->guest;
  wasm_rt_trap_t trap = wasm_rt_impl_try();
  if (trap != 0) {
    free(instance);
    return trap;
  }
  wasm2c_qjs__guest_instantiate(&instance->guest, &instance->truapi, &instance->wasi);
  if (max_pages < instance->guest.w2c_memory.max_pages) {
    instance->guest.w2c_memory.max_pages = max_pages;
  }
  *out = instance;
  return 0;
}

void qjs_free(qjs_instance* instance) {
  wasm2c_qjs__guest_free(&instance->guest);
  free(instance);
}

uint64_t qjs_memory_bytes(qjs_instance* instance) {
  return instance->guest.w2c_memory.size;
}

int qjs_write(qjs_instance* instance, u32 pointer, const uint8_t* bytes, u32 len) {
  wasm_rt_trap_t trap = wasm_rt_impl_try();
  if (trap != 0) return trap;
  memcpy(guest_bytes(&instance->guest, pointer, len), bytes, len);
  return 0;
}

int qjs_alloc(qjs_instance* instance, u32 len, u32* pointer) {
  wasm_rt_trap_t trap = wasm_rt_impl_try();
  if (trap != 0) return trap;
  *pointer = w2c_qjs__guest_alloc(&instance->guest, len);
  return 0;
}

int qjs_dealloc(qjs_instance* instance, u32 pointer, u32 len) {
  wasm_rt_trap_t trap = wasm_rt_impl_try();
  if (trap != 0) return trap;
  w2c_qjs__guest_dealloc(&instance->guest, pointer, len);
  return 0;
}

int qjs_init(qjs_instance* instance, u32 pointer, u32 len, u32* status) {
  wasm_rt_trap_t trap = wasm_rt_impl_try();
  if (trap != 0) return trap;
  *status = w2c_qjs__guest_init(&instance->guest, pointer, len);
  return 0;
}

int qjs_on_frame(qjs_instance* instance, u32 pointer, u32 len) {
  wasm_rt_trap_t trap = wasm_rt_impl_try();
  if (trap != 0) return trap;
  w2c_qjs__guest_on_frame(&instance->guest, pointer, len);
  return 0;
}

int qjs_on_timer(qjs_instance* instance, u32 id) {
  wasm_rt_trap_t trap = wasm_rt_impl_try();
  if (trap != 0) return trap;
  w2c_qjs__guest_on_timer(&instance->guest, id);
  return 0;
}

int qjs_run_jobs(qjs_instance* instance, u32* ran) {
  wasm_rt_trap_t trap = wasm_rt_impl_try();
  if (trap != 0) return trap;
  *ran = w2c_qjs__guest_run_jobs(&instance->guest);
  return 0;
}

const char* qjs_trap_name(int trap) {
  return wasm_rt_strerror((wasm_rt_trap_t)trap);
}
