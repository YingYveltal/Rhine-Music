import type { WebGLRenderer } from "three";
import { isNative, nativeInvoke } from "./native";
import type { LocationName, Session } from "./metal-live";

// Developer-only, one-shot command capture. The normal path only remembers
// texture allocation sizes; it never reads pixels, geometry or uniforms.
// Captured shaders are the actual expanded Three shaders, including our lights.
export class GLFrameCapture {
  private sizes = new Map<
    WebGLTexture,
    { width: number; height: number; format: number; levels: number }
  >();
  private gl: WebGL2RenderingContext;
  private renderTargets = new WeakSet<WebGLTexture>();
  private exportingPixels = false;
  readonly locations = new WeakMap<WebGLUniformLocation, LocationName>();
  session?: Session;
  constructor(private renderer: WebGLRenderer) {
    const gl = (this.gl = renderer.getContext() as WebGL2RenderingContext);
    const getLocation = gl.getUniformLocation.bind(gl);
    gl.getUniformLocation = (program, name) => {
      const location = getLocation(program, name);
      if (location) this.locations.set(location, { program, name });
      return location;
    };
    const attach = gl.framebufferTexture2D.bind(gl);
    gl.framebufferTexture2D = (
      target,
      attachment,
      texTarget,
      texture,
      level,
    ) => {
      if (texture && !this.exportingPixels) this.renderTargets.add(texture);
      return attach(target, attachment, texTarget, texture, level);
    };
    const remove = gl.deleteTexture.bind(gl);
    gl.deleteTexture = (texture) => {
      if (texture) this.sizes.delete(texture);
      return remove(texture);
    };
    const remember = (args: any[], storage: boolean) => {
      const target = args[0];
      if (target !== gl.TEXTURE_2D) return;
      const texture = gl.getParameter(
        gl.TEXTURE_BINDING_2D,
      ) as WebGLTexture | null;
      if (!texture || (!storage && args[1] !== 0)) return;
      const source = args[args.length - 1];
      this.sizes.set(texture, {
        width: storage ? args[3] : args.length >= 9 ? args[3] : source.width,
        height: storage ? args[4] : args.length >= 9 ? args[4] : source.height,
        format: args[2],
        levels: storage ? args[1] : 1,
      });
    };
    for (const name of ["texImage2D", "texStorage2D"] as const) {
      const original = gl[name].bind(gl) as (...args: any[]) => void;
      (gl as any)[name] = (...args: any[]) => {
        original(...args);
        remember(args, name === "texStorage2D");
      };
    }
  }
  async capture(render: () => void, metadata: Record<string, unknown>) {
    if (!isNative) throw new Error("Native capture destination required");
    const gl = this.gl;
    const id = `${new Date().toISOString().replace(/[^0-9]/g, "")}-gl-${metadata.theme}-${metadata.phase}`;
    const chunks: Uint8Array[] = [];
    let byteLength = 0;
    const bytes = (data: ArrayBufferView) => {
      const pad = (16 - (byteLength % 16)) % 16;
      if (pad) {
        chunks.push(new Uint8Array(pad));
        byteLength += pad;
      }
      const range = { offset: byteLength, length: data.byteLength };
      chunks.push(
        new Uint8Array(data.buffer, data.byteOffset, data.byteLength).slice(),
      );
      byteLength += data.byteLength;
      return range;
    };
    const programs: any[] = [],
      buffers: any[] = [],
      textures: any[] = [],
      targets: any[] = [],
      commands: any[] = [];
    const programIds = new Map<WebGLProgram, number>(),
      bufferIds = new Map<WebGLBuffer, number>();
    const textureIds = new Map<WebGLTexture, number>(),
      targetIds = new Map<WebGLFramebuffer | null, number>();
    const written = new Set<number>();
    const texture = (t: WebGLTexture) => {
      if (textureIds.has(t)) return textureIds.get(t)!;
      const id = textures.length;
      textureIds.set(t, id);
      const info = this.sizes.get(t);
      if (!info) throw new Error("Texture allocated before capture tracker");
      textures.push({ ...info });
      return id;
    };
    const attachment = (point: number, framebufferTarget: number): any => {
      const type = gl.getFramebufferAttachmentParameter(
        framebufferTarget,
        point,
        gl.FRAMEBUFFER_ATTACHMENT_OBJECT_TYPE,
      );
      if (type === gl.NONE) return null;
      const obj = gl.getFramebufferAttachmentParameter(
        framebufferTarget,
        point,
        gl.FRAMEBUFFER_ATTACHMENT_OBJECT_NAME,
      );
      if (type === gl.TEXTURE) {
        const tid = texture(obj);
        written.add(tid);
        return {
          texture: tid,
          ...textures[tid],
          level: gl.getFramebufferAttachmentParameter(
            framebufferTarget,
            point,
            gl.FRAMEBUFFER_ATTACHMENT_TEXTURE_LEVEL,
          ),
          samples: 1,
        };
      }
      const old = gl.getParameter(gl.RENDERBUFFER_BINDING);
      gl.bindRenderbuffer(gl.RENDERBUFFER, obj);
      const result = {
        width: gl.getRenderbufferParameter(
          gl.RENDERBUFFER,
          gl.RENDERBUFFER_WIDTH,
        ),
        height: gl.getRenderbufferParameter(
          gl.RENDERBUFFER,
          gl.RENDERBUFFER_HEIGHT,
        ),
        format: gl.getRenderbufferParameter(
          gl.RENDERBUFFER,
          gl.RENDERBUFFER_INTERNAL_FORMAT,
        ),
        samples:
          gl.getRenderbufferParameter(
            gl.RENDERBUFFER,
            gl.RENDERBUFFER_SAMPLES,
          ) || 1,
      };
      gl.bindRenderbuffer(gl.RENDERBUFFER, old);
      return result;
    };
    const target = (which: number = gl.DRAW_FRAMEBUFFER): number => {
      const obj = gl.getParameter(
        which === gl.READ_FRAMEBUFFER
          ? gl.READ_FRAMEBUFFER_BINDING
          : gl.DRAW_FRAMEBUFFER_BINDING,
      );
      if (targetIds.has(obj)) return targetIds.get(obj)!;
      const id = targets.length;
      targetIds.set(obj, id);
      targets.push(
        obj
          ? {
              color: attachment(gl.COLOR_ATTACHMENT0, which),
              depth: attachment(gl.DEPTH_ATTACHMENT, which),
            }
          : {
              screen: true,
              color: {
                width: gl.drawingBufferWidth,
                height: gl.drawingBufferHeight,
                format: gl.RGBA8,
                samples: 1,
              },
              depth: null,
            },
      );
      return id;
    };
    const buffer = (obj: WebGLBuffer) => {
      if (bufferIds.has(obj)) return bufferIds.get(obj)!;
      const old = gl.getParameter(gl.COPY_READ_BUFFER_BINDING);
      gl.bindBuffer(gl.COPY_READ_BUFFER, obj);
      const data = new Uint8Array(
        gl.getBufferParameter(gl.COPY_READ_BUFFER, gl.BUFFER_SIZE),
      );
      gl.getBufferSubData(gl.COPY_READ_BUFFER, 0, data);
      gl.bindBuffer(gl.COPY_READ_BUFFER, old);
      const id = buffers.length;
      bufferIds.set(obj, id);
      buffers.push(bytes(data));
      return id;
    };
    const plain = (v: any) =>
      ArrayBuffer.isView(v) ? Array.from(v as any) : v;
    const program = (obj: WebGLProgram) => {
      if (programIds.has(obj)) return programIds.get(obj)!;
      const shaders = gl.getAttachedShaders(obj)!;
      const attributes = Array.from(
        { length: gl.getProgramParameter(obj, gl.ACTIVE_ATTRIBUTES) },
        (_, i) => {
          const a = gl.getActiveAttrib(obj, i)!;
          return {
            name: a.name,
            type: a.type,
            size: a.size,
            location: gl.getAttribLocation(obj, a.name),
          };
        },
      );
      const uniforms = Array.from(
        { length: gl.getProgramParameter(obj, gl.ACTIVE_UNIFORMS) },
        (_, i) => {
          const u = gl.getActiveUniform(obj, i)!;
          return { name: u.name, type: u.type, size: u.size };
        },
      );
      const id = programs.length;
      programIds.set(obj, id);
      programs.push({
        vertex: gl.getShaderSource(
          shaders.find(
            (s) =>
              gl.getShaderParameter(s, gl.SHADER_TYPE) === gl.VERTEX_SHADER,
          )!,
        )!,
        fragment: gl.getShaderSource(
          shaders.find(
            (s) =>
              gl.getShaderParameter(s, gl.SHADER_TYPE) === gl.FRAGMENT_SHADER,
          )!,
        )!,
        attributes,
        uniforms,
      });
      return id;
    };
    const state = () => ({
      viewport: plain(gl.getParameter(gl.VIEWPORT)),
      scissor: gl.isEnabled(gl.SCISSOR_TEST)
        ? plain(gl.getParameter(gl.SCISSOR_BOX))
        : null,
      depthTest: gl.isEnabled(gl.DEPTH_TEST),
      depthWrite: gl.getParameter(gl.DEPTH_WRITEMASK),
      depthFunc: gl.getParameter(gl.DEPTH_FUNC),
      cull: gl.isEnabled(gl.CULL_FACE) ? gl.getParameter(gl.CULL_FACE_MODE) : 0,
      frontFace: gl.getParameter(gl.FRONT_FACE),
      blend: gl.isEnabled(gl.BLEND),
      blendSrc: gl.getParameter(gl.BLEND_SRC_RGB),
      blendDst: gl.getParameter(gl.BLEND_DST_RGB),
      blendSrcAlpha: gl.getParameter(gl.BLEND_SRC_ALPHA),
      blendDstAlpha: gl.getParameter(gl.BLEND_DST_ALPHA),
      blendEquation: gl.getParameter(gl.BLEND_EQUATION_RGB),
      blendEquationAlpha: gl.getParameter(gl.BLEND_EQUATION_ALPHA),
      colorMask: gl.getParameter(gl.COLOR_WRITEMASK),
      polygonOffset: gl.isEnabled(gl.POLYGON_OFFSET_FILL)
        ? [
            gl.getParameter(gl.POLYGON_OFFSET_FACTOR),
            gl.getParameter(gl.POLYGON_OFFSET_UNITS),
          ]
        : null,
    });
    const draw = (
      mode: number,
      count: number,
      first: number,
      indexType: number,
      instances: number,
    ) => {
      const obj = gl.getParameter(gl.CURRENT_PROGRAM) as WebGLProgram;
      const pid = program(obj),
        p = programs[pid];
      const active = gl.getParameter(gl.ACTIVE_TEXTURE);
      const anisotropy = gl.getExtension("EXT_texture_filter_anisotropic");
      const uniforms: Record<string, any> = {},
        samplers: Record<string, any> = {};
      for (const u of p.uniforms) {
        // WebGL getUniform returns one array element, even when ACTIVE_UNIFORM
        // reports the whole array. Fetch every element (AO kernel, light arrays).
        const value =
          u.size > 1
            ? Array.from({ length: u.size }, (_, i) => {
                const name = u.name.replace("[0]", `[${i}]`);
                const part = gl.getUniform(
                  obj,
                  gl.getUniformLocation(obj, name)!,
                );
                return ArrayBuffer.isView(part)
                  ? Array.from(part as any)
                  : [part];
              }).flat()
            : gl.getUniform(obj, gl.getUniformLocation(obj, u.name)!);
        uniforms[u.name] = plain(value);
        if (
          [
            gl.SAMPLER_2D,
            gl.SAMPLER_2D_SHADOW,
            gl.INT_SAMPLER_2D,
            gl.UNSIGNED_INT_SAMPLER_2D,
          ].includes(u.type)
        ) {
          samplers[u.name] = [];
          for (const unit of typeof value === "number"
            ? [value]
            : Array.from(value as ArrayLike<number>)) {
            gl.activeTexture(gl.TEXTURE0 + unit);
            const t = gl.getParameter(gl.TEXTURE_BINDING_2D) as WebGLTexture;
            if (!t) throw new Error(`Missing texture: ${u.name}`);
            samplers[u.name].push({
              texture: texture(t),
              min: gl.getTexParameter(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER),
              mag: gl.getTexParameter(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER),
              wrapS: gl.getTexParameter(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S),
              wrapT: gl.getTexParameter(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T),
              compare: gl.getTexParameter(
                gl.TEXTURE_2D,
                gl.TEXTURE_COMPARE_MODE,
              ),
              compareFunc: gl.getTexParameter(
                gl.TEXTURE_2D,
                gl.TEXTURE_COMPARE_FUNC,
              ),
              anisotropy: anisotropy
                ? gl.getTexParameter(
                    gl.TEXTURE_2D,
                    anisotropy.TEXTURE_MAX_ANISOTROPY_EXT,
                  )
                : 1,
            });
          }
        } else if (u.type === gl.SAMPLER_CUBE)
          throw new Error("Cube texture capture not implemented");
      }
      gl.activeTexture(active);
      const attributes: any[] = [];
      for (const a of p.attributes) {
        const columns =
          a.type === gl.FLOAT_MAT4 ? 4 : a.type === gl.FLOAT_MAT3 ? 3 : 1;
        for (let column = 0; column < columns; column++) {
          const loc = a.location + column;
          const enabled = gl.getVertexAttrib(
            loc,
            gl.VERTEX_ATTRIB_ARRAY_ENABLED,
          );
          attributes.push({
            name: a.name,
            column,
            location: loc,
            enabled,
            ...(enabled
              ? {
                  buffer: buffer(
                    gl.getVertexAttrib(
                      loc,
                      gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING,
                    ),
                  ),
                  size: gl.getVertexAttrib(loc, gl.VERTEX_ATTRIB_ARRAY_SIZE),
                  type: gl.getVertexAttrib(loc, gl.VERTEX_ATTRIB_ARRAY_TYPE),
                  normalized: gl.getVertexAttrib(
                    loc,
                    gl.VERTEX_ATTRIB_ARRAY_NORMALIZED,
                  ),
                  stride: gl.getVertexAttrib(
                    loc,
                    gl.VERTEX_ATTRIB_ARRAY_STRIDE,
                  ),
                  offset: gl.getVertexAttribOffset(
                    loc,
                    gl.VERTEX_ATTRIB_ARRAY_POINTER,
                  ),
                  divisor: gl.getVertexAttrib(
                    loc,
                    gl.VERTEX_ATTRIB_ARRAY_DIVISOR,
                  ),
                }
              : {
                  value: plain(
                    gl.getVertexAttrib(loc, gl.CURRENT_VERTEX_ATTRIB),
                  ),
                }),
          });
        }
      }
      commands.push({
        op: "draw",
        target: target(),
        program: pid,
        mode,
        count,
        first,
        indexType,
        instances,
        index: indexType
          ? buffer(gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING))
          : null,
        uniforms,
        samplers,
        attributes,
        state: state(),
      });
    };
    const originals = new Map<string, any>();
    const hook = (name: string, capture: (...args: any[]) => void) => {
      const original = (gl as any)[name].bind(gl);
      originals.set(name, original);
      (gl as any)[name] = (...args: any[]) => {
        capture(...args);
        return original(...args);
      };
    };
    let reference: Uint8Array;
    try {
      hook("clear", (mask) =>
        commands.push({
          op: "clear",
          target: target(),
          mask,
          color: plain(gl.getParameter(gl.COLOR_CLEAR_VALUE)),
          depth: gl.getParameter(gl.DEPTH_CLEAR_VALUE),
          state: state(),
        }),
      );
      hook("drawElements", (mode, count, type, offset) =>
        draw(mode, count, offset, type, 1),
      );
      hook("drawElementsInstanced", (mode, count, type, offset, n) =>
        draw(mode, count, offset, type, n),
      );
      hook("drawArrays", (mode, first, count) =>
        draw(mode, count, first, 0, 1),
      );
      hook("drawArraysInstanced", (mode, first, count, n) =>
        draw(mode, count, first, 0, n),
      );
      hook("blitFramebuffer", (...args) =>
        commands.push({
          op: "blit",
          from: target(gl.READ_FRAMEBUFFER),
          to: target(),
          args,
        }),
      );
      hook("generateMipmap", (which) => {
        if (which !== gl.TEXTURE_2D) throw new Error("Non-2D mip capture");
        const tid = texture(gl.getParameter(gl.TEXTURE_BINDING_2D));
        textures[tid].mipmap = true;
        commands.push({ op: "mipmap", texture: tid });
      });
      render();
      reference = new Uint8Array(
        gl.drawingBufferWidth * gl.drawingBufferHeight * 4,
      );
      gl.readPixels(
        0,
        0,
        gl.drawingBufferWidth,
        gl.drawingBufferHeight,
        gl.RGBA,
        gl.UNSIGNED_BYTE,
        reference,
      );
    } finally {
      for (const [name, original] of originals) (gl as any)[name] = original;
    }

    // Include cached cover textures that are not visible in this particular
    // pose, so switching back to a warm album only changes a sampler binding.
    for (const object of this.sizes.keys())
      if (!this.renderTargets.has(object)) texture(object);
    // Export only immutable input textures. All render targets are rebuilt by
    // Metal commands, never replaced with their captured WebGL output image.
    const oldRead = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING),
      oldDraw = gl.getParameter(gl.DRAW_FRAMEBUFFER_BINDING);
    const fbo = gl.createFramebuffer()!;
    this.exportingPixels = true;
    try {
      gl.bindFramebuffer(gl.FRAMEBUFFER, fbo);
      for (const [obj, tid] of textureIds) {
        const t = textures[tid];
        t.renderTarget = written.has(tid);
        if (t.renderTarget) continue;
        gl.framebufferTexture2D(
          gl.FRAMEBUFFER,
          gl.COLOR_ATTACHMENT0,
          gl.TEXTURE_2D,
          obj,
          0,
        );
        if (
          gl.checkFramebufferStatus(gl.FRAMEBUFFER) !== gl.FRAMEBUFFER_COMPLETE
        )
          throw new Error(`Unreadable texture ${tid}`);
        const floating = [
          gl.RGBA16F,
          gl.RGBA32F,
          gl.RGB16F,
          gl.RGB32F,
          gl.R16F,
          gl.R32F,
          gl.RG16F,
          gl.RG32F,
        ].includes(t.format);
        const pixels = floating
          ? new Float32Array(t.width * t.height * 4)
          : new Uint8Array(t.width * t.height * 4);
        gl.readPixels(
          0,
          0,
          t.width,
          t.height,
          gl.RGBA,
          floating ? gl.FLOAT : gl.UNSIGNED_BYTE,
          pixels,
        );
        const error = gl.getError();
        if (error) throw new Error(`Texture ${tid} readback GL ${error}`);
        t.data = bytes(pixels);
        t.dataType = floating ? "float32" : "uint8";
      }
    } finally {
      gl.bindFramebuffer(gl.READ_FRAMEBUFFER, oldRead);
      gl.bindFramebuffer(gl.DRAW_FRAMEBUFFER, oldDraw);
      gl.deleteFramebuffer(fbo);
      this.exportingPixels = false;
    }
    const ref = bytes(reference!);
    const data = new Uint8Array(byteLength);
    let cursor = 0;
    for (const chunk of chunks) {
      data.set(chunk, cursor);
      cursor += chunk.length;
    }
    const size = 8 * 1024 * 1024;
    const files: string[] = [];
    const save = async (name: string, b: Uint8Array) => {
      let binary = "";
      for (let i = 0; i < b.length; i += 32768)
        binary += String.fromCharCode(...b.subarray(i, i + 32768));
      await nativeInvoke("save_render_capture", {
        id,
        name,
        data: btoa(binary),
      });
    };
    for (let start = 0; start < data.length; start += size) {
      const name = `data-${String(files.length).padStart(3, "0")}.bin`;
      files.push(name);
      await save(name, data.subarray(start, start + size));
    }
    const manifest = {
      version: 1,
      id,
      ...metadata,
      width: gl.drawingBufferWidth,
      height: gl.drawingBufferHeight,
      reference: ref,
      files,
      byteLength,
      programs,
      buffers,
      textures,
      targets,
      commands,
      rowOrder: "bottom-up",
      pageVisibility: document.visibilityState,
      pageFocused: document.hasFocus(),
    };
    await save(
      "gl-frame.json",
      new TextEncoder().encode(JSON.stringify(manifest)),
    );
    this.session = {
      id,
      manifest,
      programs: programIds,
      buffers: bufferIds,
      textures: textureIds,
      targets: targetIds,
      locations: this.locations,
    };
    return id;
  }
}
