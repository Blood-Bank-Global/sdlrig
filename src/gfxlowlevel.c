#include "gfxlowlevel.h"

#include <SDL2/SDL.h>
#include <SDL2/SDL_vulkan.h>
#include <libavformat/avformat.h>
#include <libplacebo/gpu.h>
#include <libplacebo/renderer.h>
#include <libplacebo/shaders.h>
#include <libplacebo/shaders/custom.h>
#include <libplacebo/shaders/lut.h>
#include <libplacebo/shaders/sampling.h>
#include <libplacebo/vulkan.h>
#pragma GCC diagnostic push
#pragma GCC diagnostic ignored "-Wswitch"
#include <libplacebo/utils/libav.h>
#pragma GCC diagnostic pop
#include <libavutil/pixfmt.h>
#include <libplacebo/utils/upload.h>
#include <stdio.h>
#include <stdlib.h>

void gfx_lowlevel_gpu_ctx_destroy(struct gfx_lowlevel_gpu_ctx **ctx) {
  if (ctx == NULL || *ctx == NULL) {
    return;
  }
  if ((*ctx)->renderer != NULL) {
    pl_renderer_destroy(&((*ctx)->renderer));
  }
  if ((*ctx)->swchain != NULL) {
    pl_swapchain_destroy(&((*ctx)->swchain));
  }
  if ((*ctx)->vk_surface != NULL) {
    vkDestroySurfaceKHR((*ctx)->vk->instance, (*ctx)->vk_surface, NULL);
  }
  if ((*ctx)->vk != NULL) {
    pl_vulkan_destroy(&((*ctx)->vk));
  }
  if ((*ctx)->log != NULL) {
    pl_log_destroy(&((*ctx)->log));
  }
  free(*ctx);
  *ctx = NULL;
}

// Define a simple logging function for libplacebo
void log_callback(void *priv __attribute__((unused)),
                  enum pl_log_level level __attribute__((unused)),
                  const char *msg) {
  fprintf(stderr, "[libplacebo] %s\n", msg);
}

// Function to swap buffers for the SDL window
void gfx_lowlevel_swapwindow(struct gfx_lowlevel_gpu_ctx *ctx) {
  if (ctx && ctx->shared_window) {
    SDL_GL_SwapWindow(ctx->shared_window);
  }
}

struct gfx_lowlevel_gpu_ctx *gfx_lowlevel_gpu_ctx_init(
    struct SDL_Window *window) {
  struct gfx_lowlevel_gpu_ctx *ctx =
      malloc(sizeof(struct gfx_lowlevel_gpu_ctx));
  if (!ctx) {
    fprintf(stderr, "Failed to allocate memory for gfx_ctx\n");
    return NULL;
  }
  memset(ctx, 0, sizeof(struct gfx_lowlevel_gpu_ctx));

  ctx->shared_window = window;

  struct pl_log_params log_params = {
      .log_cb = log_callback,
      .log_priv = NULL,
      .log_level = PL_LOG_WARN,
  };
  ctx->log = pl_log_create(PL_API_VER, &log_params);
  if (ctx->log == NULL) {
    fprintf(stderr, "Failed to create libplacebo log\n");
    gfx_lowlevel_gpu_ctx_destroy(&ctx);
    return NULL;
  }

  const char *extensions[] = {
      "VK_MVK_moltenvk",
      "VK_MVK_macos_surface",
      "VK_EXT_metal_surface",
  };
  unsigned int num_extensions = sizeof(extensions) / sizeof(extensions[0]);

  struct pl_vulkan_params vk_params = {
      .async_transfer = 1,
      .async_compute = 1,
      .queue_count = 1,
      .instance_params =
          &(struct pl_vk_inst_params){
              .extensions = (const char **)extensions,
              .num_extensions = num_extensions,
          },
      .get_proc_addr = SDL_Vulkan_GetVkGetInstanceProcAddr(),
  };

  ctx->vk = pl_vulkan_create(ctx->log, &vk_params);
  if (ctx->vk == NULL) {
    fprintf(stderr, "Failed to create libplacebo Vulkan context\n");
    gfx_lowlevel_gpu_ctx_destroy(&ctx);
    return NULL;
  }

  if (!SDL_Vulkan_CreateSurface(window, ctx->vk->instance, &ctx->vk_surface)) {
    fprintf(stderr, "Failed to create Vulkan surface\n");
    return NULL;
  }

  // Create a swapchain
  struct pl_vulkan_swapchain_params swapchain_params = {
      .surface = ctx->vk_surface,
      .present_mode = VK_PRESENT_MODE_FIFO_KHR,
  };

  ctx->swchain = pl_vulkan_create_swapchain(ctx->vk, &swapchain_params);

  if (ctx->swchain == NULL) {
    fprintf(stderr, "Failed to create libplacebo swapchain\n");
    gfx_lowlevel_gpu_ctx_destroy(&ctx);
    return NULL;
  }

  int width, height;
  SDL_GetWindowSize(ctx->shared_window, &width, &height);
  if (!pl_swapchain_resize(ctx->swchain, &width, &height)) {
    fprintf(stderr, "Failed to resize swapchain\n");
    gfx_lowlevel_gpu_ctx_destroy(&ctx);
    return NULL;
  }

  // Create a renderer
  ctx->renderer = pl_renderer_create(ctx->log, ctx->vk->gpu);
  if (ctx->renderer == NULL) {
    fprintf(stderr, "Failed to create libplacebo renderer\n");
    gfx_lowlevel_gpu_ctx_destroy(&ctx);
    return NULL;
  }

  return ctx;
}

int gfx_lowlevel_gpu_ctx_handle_resize(struct gfx_lowlevel_gpu_ctx *ctx,
                                       int width, int height) {
  if (!ctx || !ctx->swchain) {
    fprintf(stderr, "Invalid context or swapchain\n");
    return -1;
  }

  if (!pl_swapchain_resize(ctx->swchain, &width, &height)) {
    fprintf(stderr, "Failed to resize swapchain\n");
    return -1;
  }

  return 0;
}

// This may return and need to be rerun after window events are drained
bool gfx_lowlevel_gpu_ctx_start_frame(struct gfx_lowlevel_gpu_ctx *ctx) {
  assert(ctx != NULL);
  assert(ctx->swchain != NULL);
  assert(!ctx->started);

  if (pl_swapchain_start_frame(ctx->swchain, &ctx->swap_frame)) {
    ctx->started = true;
    pl_frame_from_swapchain(&ctx->window_frame, &ctx->swap_frame);
    return true;
  }

  return false;
}

int gfx_lowlevel_map_frame_ctx(struct gfx_lowlevel_gpu_ctx *ctx,
                               struct gfx_lowlevel_frame_ctx *dst,
                               AVFrame *src) {
  if (!ctx || !dst || !src) {
    fprintf(stderr, "Invalid context or frame\n");
    return EINVAL;
  }

  if (dst->to_rgba == NULL) {
    enum AVPixelFormat src_format = src->format;
    if (src_format == AV_PIX_FMT_VIDEOTOOLBOX) {
      src_format = AV_PIX_FMT_NV12;
    }
    struct SwsContext *sws_ctx = sws_getContext(
        src->width, src->height, src_format, src->width, src->height,
        AV_PIX_FMT_RGBA, SWS_BILINEAR, NULL, NULL, NULL);
    if (!sws_ctx) {
      fprintf(stderr, "Failed to create sws context\n");
      return ENOMEM;
    }
    dst->to_rgba = sws_ctx;
  }

  if (dst->is_mapped) {
    pl_unmap_avframe(ctx->vk->gpu, &dst->pl_frame);
  }

  int ret = 0;
  AVFrame *tmp = NULL;
  if (src->format == AV_PIX_FMT_VIDEOTOOLBOX) {
    tmp = av_frame_alloc();
    if (!tmp) {
      fprintf(stderr, "Failed to allocate temporary AVFrame\n");
      return ENOMEM;
    }
    tmp->format = AV_PIX_FMT_NV12;
    ret = av_hwframe_transfer_data(tmp, src, 0);
    if (ret < 0) {
      fprintf(stderr, "Failed to transfer data %d\n", ret);
      exit(1);
      av_frame_free(&tmp);
      return ret;
    }
    src = tmp;
  }

  AVFrame *map_src, *rgba_frame = NULL;
  if (dst->to_rgba != NULL) {
    rgba_frame = av_frame_alloc();
    if (!rgba_frame) {
      fprintf(stderr, "Failed to allocate temporary AVFrame\n");
      av_frame_free(&tmp);
      return ENOMEM;
    }

    rgba_frame->width = src->width;
    rgba_frame->height = src->height;
    rgba_frame->format = AV_PIX_FMT_RGBA;
    ret = av_frame_get_buffer(rgba_frame, 32);
    if (ret < 0) {
      fprintf(stderr, "Failed to allocate hw frame buffer %d\n", ret);
      av_frame_free(&rgba_frame);
      av_frame_free(&tmp);
      return ret;
    }

    ret = sws_scale(dst->to_rgba, (const uint8_t *const *)src->data,
                    src->linesize, 0, src->height, rgba_frame->data,
                    rgba_frame->linesize);
    if (ret < 0) {
      fprintf(stderr, "Failed to scale frame %d\n", ret);
      av_frame_free(&rgba_frame);
      av_frame_free(&tmp);
      return ret;
    }
    map_src = rgba_frame;
  } else {
    map_src = src;
  }

  struct pl_avframe_params params = {.frame = map_src, .tex = dst->tex};
  ret = pl_map_avframe_ex(ctx->vk->gpu, &dst->pl_frame, &params);
  if (ret < 0) {
    fprintf(stderr, "Failed to map AVFrame to libplacebo frame\n");
    return ret;
  }
  dst->is_mapped = true;

  if (tmp) {
    av_frame_free(&tmp);
  }

  if (rgba_frame) {
    av_frame_free(&rgba_frame);
  }

  return 0;
}

int gfx_lowlevel_frame_create_texture(struct gfx_lowlevel_gpu_ctx *ctx,
                                      struct gfx_lowlevel_frame_ctx *frame,
                                      int width, int height) {
  if (!ctx || !frame) {
    fprintf(stderr, "Invalid context or frame\n");
    return EINVAL;
  }

  pl_fmt fmt = pl_find_named_fmt(ctx->vk->gpu, "rgba8");
  if (!fmt) {
    fprintf(stderr, "Failed to find format\n");
    return EINVAL;
  }

  struct pl_tex_params tex_params = {
      .w = width,
      .h = height,
      .d = 0,
      .format = fmt,
      .sampleable = true,
      .renderable = true,
      .blit_src = true,
      .blit_dst = true,
  };

  frame->tex[0] = pl_tex_create(ctx->vk->gpu, &tex_params);
  if (!frame->tex[0]) {
    fprintf(stderr, "Failed to create texture\n");
    return EINVAL;
  }

  struct pl_plane plane = {
      .texture = frame->tex[0],
      .components = fmt->num_components,
      .component_mapping = {fmt->sample_order[0], fmt->sample_order[1],
                            fmt->sample_order[2], fmt->sample_order[3]},
  };

  struct pl_frame *f = &frame->pl_frame;
  f->num_planes = 1;
  f->planes[0] = plane;
  f->repr = pl_color_repr_unknown;
  f->color = pl_color_space_unknown;

  return 0;
}

struct gfx_lowlevel_frame_ctx *gfx_lowlevel_frame_ctx_init(
    struct gfx_lowlevel_gpu_ctx *ctx) {
  if (!ctx) {
    fprintf(stderr, "Invalid context\n");
    return NULL;
  }
  struct gfx_lowlevel_frame_ctx *frame =
      malloc(sizeof(struct gfx_lowlevel_frame_ctx));
  if (!frame) {
    fprintf(stderr, "Failed to allocate memory for gfx_lowlevel_frame\n");
    return NULL;
  }
  memset(frame, 0, sizeof(struct gfx_lowlevel_frame_ctx));
  frame->ctx_backref = ctx;
  return frame;
}

void gfx_lowlevel_frame_ctx_destroy(struct gfx_lowlevel_frame_ctx **frame) {
  if (frame && *frame && (*frame)->ctx_backref &&
      (*frame)->ctx_backref->swchain && (*frame)->ctx_backref->vk->gpu) {
    if ((*frame)->is_mapped) {
      pl_unmap_avframe((*frame)->ctx_backref->vk->gpu, &(*frame)->pl_frame);
    }

    for (int i = 0; i < 4; i++) {
      if ((*frame)->tex[i]) {
        pl_tex_destroy((*frame)->ctx_backref->vk->gpu, &(*frame)->tex[i]);
      }
    }

    if ((*frame)->to_rgba) {
      sws_freeContext((*frame)->to_rgba);
    }

    (*frame)->is_mapped = false;
    (*frame)->ctx_backref = NULL;
    (*frame)->pl_frame = (struct pl_frame){0};
    free(*frame);
    *frame = NULL;
  }
}

int gfx_lowlevel_frame_clear(struct gfx_lowlevel_gpu_ctx *ctx,
                             struct pl_frame *dst_frame, float r, float g,
                             float b, float a) {
  if (!ctx || !dst_frame) {
    fprintf(stderr, "Invalid context or frame\n");
    return EINVAL;
  }

  pl_frame_clear_rgba(ctx->vk->gpu, dst_frame, (float[4]){r, g, b, a});
  return 0;
}

int gfx_lowlevel_gpu_ctx_render(struct gfx_lowlevel_gpu_ctx *ctx,
                                struct gfx_lowlevel_mix_ctx *mix_ctx,
                                struct gfx_lowlevel_filter_params const *params,
                                struct pl_frame *dst_frame,
                                struct pl_frame **src_frames, int num_frames,
                                struct gfx_lowlevel_lut *lut, bool debug) {
  if (!ctx || !src_frames || !dst_frame || !params) {
    fprintf(stderr, "Invalid context or frame\n");
    return EINVAL;
  }

  // Render the image and run shaders
  pl_shader sh = pl_dispatch_begin(mix_ctx->dispatch);
  if (!sh) {
    fprintf(stderr, "Failed to begin dispatch\n");
    return EINVAL;
  }

  struct pl_shader_desc *descs =
      malloc(sizeof(struct pl_shader_desc) * num_frames);
  if (!descs) {
    fprintf(stderr, "Failed to allocate memory for shader descriptors\n");
    return ENOMEM;
  }
  memset(descs, 0, sizeof(struct pl_shader_desc) * num_frames);

  for (int i = 0; i < num_frames; i++) {
    char *name = malloc(32);
    if (!name) {
      fprintf(stderr, "Failed to allocate memory for shader name\n");
      free(descs);
      return ENOMEM;
    }
    snprintf(name, 32, "src_tex%d", i);
    descs[i] = (struct pl_shader_desc){
        .desc = {.name = name,
                 .type = PL_DESC_SAMPLED_TEX,
                 .binding = i,
                 .access = PL_DESC_ACCESS_READONLY},
        .binding =
            {
                .object = src_frames[i]->planes[0].texture,
                .address_mode = PL_TEX_ADDRESS_REPEAT,
                .sample_mode = PL_TEX_SAMPLE_LINEAR,
            },
    };
  }
  int num_descs = num_frames;

  struct pl_shader_va *attribs =
      malloc(sizeof(struct pl_shader_va) * num_frames);
  if (!attribs) {
    fprintf(stderr, "Failed to allocate memory for shader attributes\n");
    free(descs);
    return ENOMEM;
  }
  memset(attribs, 0, sizeof(struct pl_shader_va) * num_frames);
  for (int i = 0; i < num_frames; i++) {
    char *name = malloc(32);
    if (!name) {
      fprintf(stderr, "Failed to allocate memory for shader name\n");
      free(attribs);
      free(descs);
      return ENOMEM;
    }
    snprintf(name, 32, "src_coord%d", i);

    float *verts = malloc(sizeof(float) * 4 * 2);
    if (!verts) {
      fprintf(stderr, "Failed to allocate memory for shader vertices\n");
      free(name);
      free(attribs);
      free(descs);
      return ENOMEM;
    }

    verts[0] = params->src.x0;
    verts[1] = params->src.y0;
    verts[2] = params->src.x1;
    verts[3] = params->src.y0;
    verts[4] = params->src.x0;
    verts[5] = params->src.y1;
    verts[6] = params->src.x1;
    verts[7] = params->src.y1;
    attribs[i] = (struct pl_shader_va){
        .attr =
            {
                .name = name,
                .offset = 0,
                .fmt = pl_find_vertex_fmt(ctx->vk->gpu, PL_FMT_FLOAT, 2),
            },
        .data = {verts, (verts + 2), (verts + 4), (verts + 6)},
    };
  }

  int num_verts = num_frames;

  struct pl_custom_shader sh_params = {
      .description = "Return src tex",
      .prelude = params->prelude,
      .header = params->header,
      .body = params->body,
      .input = PL_SHADER_SIG_NONE,
      .output = PL_SHADER_SIG_COLOR,
      .descriptors = descs,
      .num_descriptors = num_descs,
      .variables = params->vars,
      .num_variables = params->num_vars,
      .vertex_attribs = attribs,
      .num_vertex_attribs = num_verts,
      .constants = NULL,
      .num_constants = 0,
      .compute = false,
  };

  if (!pl_shader_custom(sh, &sh_params)) {
    fprintf(stderr, "Failed to create custom shader\n");
    return EINVAL;
  }

  if (lut && lut->lut) {
    pl_shader_custom_lut(sh, lut->lut, &lut->lut_state);
  }

  if (debug) {
    pl_dispatch_abort(mix_ctx->dispatch, &sh);
    const struct pl_shader_res *res = pl_shader_finalize(sh);
    if (!res) {
      fprintf(stderr, "Failed to finalize shader\n");
      free(attribs);
      free(descs);
      exit(1);
    } else {
      fprintf(stderr, "Shader finalized successfully\n");
      fprintf(stderr, "Shader input signature: %d\n", res->input);
      fprintf(stderr, "Shader output signature: %d\n", res->output);
      fprintf(stderr, "Shader num descriptors: %d\n", res->num_descriptors);
      fprintf(stderr, "Shader num variables: %d\n", res->num_variables);
      fprintf(stderr, "Shader num vertex attributes: %d\n",
              res->num_vertex_attribs);
      fprintf(stderr, "Shader num constants: %d\n", res->num_constants);
      fprintf(stderr, "Input signature: %d\n", res->input);
      fprintf(stderr, "Output signature: %d\n", res->output);
      // print the name of the steps in the shader info
      for (int i = 0; i < res->info->num_steps; i++) {
        fprintf(stderr, "Step %d: %s\n", i, res->info->steps[i]);
      }
      fprintf(stderr, "GLSL code:\n%s\n", res->glsl);
    }

  } else {
    if (!pl_dispatch_finish(
            mix_ctx->dispatch,
            &(struct pl_dispatch_params){
                .shader = &sh,
                .target = dst_frame->planes[0].texture,
                .rect =
                    {
                        .x0 = params->dst.x0 *
                              dst_frame->planes[0].texture->params.w,
                        .y0 = params->dst.y0 *
                              dst_frame->planes[0].texture->params.h,
                        .x1 = params->dst.x1 *
                              dst_frame->planes[0].texture->params.w,
                        .y1 = params->dst.y1 *
                              dst_frame->planes[0].texture->params.h,
                    },
            })) {
      fprintf(stderr, "Failed to finish dispatch\n");
      return EINVAL;
    }
  }
  for (int i = 0; i < num_verts; i++) {
    free((void *)attribs[i].attr.name);
  }
  free(attribs);
  for (int i = 0; i < num_descs; i++) {
    free((void *)descs[i].desc.name);
  }
  free((void *)descs);

  return 0;
}

int gfx_lowlevel_gpu_ctx_finish_frame(struct gfx_lowlevel_gpu_ctx *ctx) {
  assert(ctx != NULL);
  assert(ctx->swchain != NULL);
  assert(ctx->started);
  pl_swapchain_swap_buffers(ctx->swchain);
  pl_swapchain_submit_frame(ctx->swchain);
  ctx->started = false;
  return 0;
}

struct gfx_lowlevel_mix_ctx *gfx_lowlevel_mix_ctx_init(
    struct gfx_lowlevel_gpu_ctx *ctx, const char *prelude, const char *header,
    const char *body, struct pl_shader_var *vars, int num_vars) {
  if (!ctx) {
    fprintf(stderr, "Invalid context\n");
    return NULL;
  }
  struct gfx_lowlevel_mix_ctx *mix_ctx =
      malloc(sizeof(struct gfx_lowlevel_mix_ctx));
  if (!mix_ctx) {
    fprintf(stderr, "Failed to allocate memory for gfx_lowlevel_mix_ctx\n");
    return NULL;
  }
  memset(mix_ctx, 0, sizeof(struct gfx_lowlevel_mix_ctx));
  mix_ctx->ctx = ctx;

  if (prelude) {
    mix_ctx->prelude = malloc(strlen(prelude) + 1);
    strncpy(mix_ctx->prelude, prelude, strlen(prelude) + 1);
  }
  if (header) {
    mix_ctx->header = malloc(strlen(header) + 1);
    strncpy(mix_ctx->header, header, strlen(header) + 1);
  }
  if (body) {
    mix_ctx->body = malloc(strlen(body) + 1);
    strncpy(mix_ctx->body, body, strlen(body) + 1);
  }

  struct pl_shader_var *var_copy =
      malloc(sizeof(struct pl_shader_var) * num_vars);
  if (!var_copy) {
    fprintf(stderr, "Failed to allocate memory for shader variables\n");
    gfx_lowlevel_mix_ctx_destroy(&mix_ctx);
    return NULL;
  }

  for (int i = 0; i < num_vars; i++) {
    var_copy[i] = vars[i];
  }
  mix_ctx->vars = var_copy;
  mix_ctx->num_vars = num_vars;
  mix_ctx->dispatch = pl_dispatch_create(ctx->log, ctx->vk->gpu);
  if (!mix_ctx->dispatch) {
    fprintf(stderr, "Failed to create dispatch\n");
    gfx_lowlevel_mix_ctx_destroy(&mix_ctx);
    return NULL;
  }

  return mix_ctx;
}

void gfx_lowlevel_mix_ctx_destroy(struct gfx_lowlevel_mix_ctx **mix_ctx) {
  if (mix_ctx && *mix_ctx) {
    free((void *)(*mix_ctx)->prelude);
    free((void *)(*mix_ctx)->header);
    free((void *)(*mix_ctx)->body);
    for (int i = 0; i < (*mix_ctx)->num_vars; i++) {
      free((void *)(*mix_ctx)->vars[i].var.name);
      free((void *)(*mix_ctx)->vars[i].data);
    }
    free((void *)(*mix_ctx)->vars);
    if ((*mix_ctx)->dispatch) {
      pl_dispatch_destroy(&(*mix_ctx)->dispatch);
    }

    free((void *)(*mix_ctx));
    *mix_ctx = NULL;
  }
}

struct gfx_lowlevel_lut *gfx_lowlevel_init_lut(struct gfx_lowlevel_gpu_ctx *ctx,
                                               const char *lut_filename) {
  if (!ctx || !lut_filename) {
    fprintf(stderr, "Invalid context or LUT filename\n");
    return NULL;
  }
  struct gfx_lowlevel_lut *lut = malloc(sizeof(struct gfx_lowlevel_lut));
  if (!lut) {
    fprintf(stderr, "Failed to allocate memory for LUT\n");
    return NULL;
  }
  memset(lut, 0, sizeof(struct gfx_lowlevel_lut));
  lut->lut_filename = malloc(strlen(lut_filename) + 1);
  if (!lut->lut_filename) {
    fprintf(stderr, "Failed to allocate memory for LUT filename\n");
    free(lut);
    return NULL;
  }
  strncpy(lut->lut_filename, lut_filename, strlen(lut_filename) + 1);
  FILE *file = fopen(lut->lut_filename, "r");
  if (!file) {
    fprintf(stderr, "Failed to open LUT file\n");
    free(lut->lut_filename);
    free(lut);
    return NULL;
  }
  fseek(file, 0, SEEK_END);
  long file_size = ftell(file);
  fseek(file, 0, SEEK_SET);
  char *file_contents = malloc(file_size + 1);
  if (!file_contents) {
    fprintf(stderr, "Failed to allocate memory for LUT file contents\n");
    fclose(file);
    free(lut->lut_filename);
    free(lut);
    return NULL;
  }
  fread(file_contents, 1, file_size, file);
  file_contents[file_size] = '\0';
  fclose(file);

  // parse the LUT file
  lut->lut = pl_lut_parse_cube(ctx->log, file_contents, file_size);
  free(file_contents);

  if (!lut->lut) {
    fprintf(stderr, "Failed to parse LUT file\n");
    gfx_lowlevel_destroy_lut(&lut);
    return NULL;
  }

  return lut;
}

int gfx_lowlevel_destroy_lut(struct gfx_lowlevel_lut **lut) {
  if (lut && *lut) {
    free((void *)(*lut)->lut_filename);
    if ((*lut)->lut) {
      pl_lut_free(&(*lut)->lut);
    }
    if ((*lut)->lut_state) {
      pl_shader_obj_destroy(&(*lut)->lut_state);
    }
    free((void *)(*lut));
    *lut = NULL;
  }
  return 0;
}