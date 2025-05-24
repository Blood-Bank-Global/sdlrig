#ifndef GFXLOWLEVEL_H
#define GFXLOWLEVEL_H

#include <SDL2/SDL.h>
#include <SDL2/SDL_vulkan.h>
#include <libavformat/avformat.h>
#include <libavutil/pixfmt.h>
#include <libplacebo/dispatch.h>
#include <libplacebo/gpu.h>
#include <libplacebo/renderer.h>
#include <libplacebo/swapchain.h>
#include <libplacebo/utils/upload.h>
#include <libplacebo/vulkan.h>
#include <libswscale/swscale.h>
#include <stdlib.h>

struct gfx_lowlevel_frame_ctx {
  bool is_mapped;
  struct pl_frame pl_frame;
  pl_tex tex[4];
  struct gfx_lowlevel_gpu_ctx* ctx_backref;
  struct SwsContext* to_rgba;
};

struct gfx_lowlevel_gpu_ctx {
  SDL_Window* shared_window;
  pl_vulkan vk;
  VkSurfaceKHR vk_surface;
  pl_swapchain swchain;
  struct pl_swapchain_frame swap_frame;
  struct pl_frame window_frame;
  pl_renderer renderer;
  pl_log log;
  bool started;
};

struct gfx_lowlevel_filter_params {
  pl_rect2df src;
  pl_rect2df dst;
  float rotation;
  const char* prelude;
  const char* header;
  const char* body;
  struct pl_shader_var* vars;
  int num_vars;
};

struct gfx_lowlevel_mix_ctx {
  struct gfx_lowlevel_gpu_ctx* ctx;
  char* prelude;
  char* header;
  char* body;
  struct pl_shader_var* vars;
  int num_vars;
  pl_dispatch dispatch;
};

struct gfx_lowlevel_lut {
  char* lut_filename;
  struct pl_custom_lut* lut;
  pl_shader_obj lut_state;
};

#define GFX_EAGAIN 35
struct gfx_lowlevel_gpu_ctx* gfx_lowlevel_gpu_ctx_init(
    struct SDL_Window* window);
void gfx_lowlevel_gpu_ctx_destroy(struct gfx_lowlevel_gpu_ctx** ctx);
int gfx_lowlevel_gpu_ctx_handle_resize(struct gfx_lowlevel_gpu_ctx* ctx,
                                       int width, int height);
bool gfx_lowlevel_gpu_ctx_start_frame(struct gfx_lowlevel_gpu_ctx* ctx);

struct gfx_lowlevel_frame_ctx* gfx_lowlevel_frame_ctx_init(
    struct gfx_lowlevel_gpu_ctx* ctx);
void gfx_lowlevel_frame_ctx_destroy(struct gfx_lowlevel_frame_ctx** frame);

int gfx_lowlevel_map_frame_ctx(struct gfx_lowlevel_gpu_ctx* ctx,
                               struct gfx_lowlevel_frame_ctx* dst,
                               AVFrame* src);
int gfx_lowlevel_frame_clear(struct gfx_lowlevel_gpu_ctx* ctx,
                             struct pl_frame* dst_frame, float r, float g,
                             float b, float a);

int gfx_lowlevel_gpu_ctx_render(struct gfx_lowlevel_gpu_ctx* ctx,
                                struct gfx_lowlevel_mix_ctx* mix_ctx,
                                struct gfx_lowlevel_filter_params const* params,
                                struct pl_frame* dst_frame,
                                struct pl_frame** src_frames, int num_frames,
                                struct gfx_lowlevel_lut* lut, bool debug);

int gfx_lowlevel_gpu_ctx_finish_frame(struct gfx_lowlevel_gpu_ctx* ctx);

struct gfx_lowlevel_mix_ctx* gfx_lowlevel_mix_ctx_init(
    struct gfx_lowlevel_gpu_ctx* ctx, const char* prelude, const char* header,
    const char* body, struct pl_shader_var* vars, int num_vars);

void gfx_lowlevel_mix_ctx_destroy(struct gfx_lowlevel_mix_ctx** ctx);

int gfx_lowlevel_frame_create_texture(struct gfx_lowlevel_gpu_ctx* ctx,
                                      struct gfx_lowlevel_frame_ctx* frame,
                                      int width, int height);
struct gfx_lowlevel_lut* gfx_lowlevel_init_lut(struct gfx_lowlevel_gpu_ctx* ctx,
                                               const char* lut_filename);
int gfx_lowlevel_destroy_lut(struct gfx_lowlevel_lut** lut);

#endif  // GFXLOWLEVEL_H