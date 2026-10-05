import { rgbaRows, encodeBytes } from "./metal-data";
// Fast command recording after a one-shot GL resource capture. There are no
// getParameter/getUniform/readPixels calls in the steady-state draw path.
// Three still owns the original animation, culling, material and input logic.
type GL = WebGL2RenderingContext;
export type LocationName = { program: WebGLProgram; name: string };
export type Session = {
  id: string;
  manifest: any;
  programs: Map<WebGLProgram, number>;
  buffers: Map<WebGLBuffer, number>;
  textures: Map<WebGLTexture, number>;
  targets: Map<WebGLFramebuffer | null, number>;
  locations: WeakMap<WebGLUniformLocation, LocationName>;
};
const plain = (v: any): any =>
  ArrayBuffer.isView(v) ? Array.from(v as any) : v;
export class MetalLiveRecorder {
  private originals = new Map<string, any>();
  private uniforms = new Map<WebGLProgram, Record<string, any>>();
  private currentProgram: WebGLProgram;
  private drawTarget: WebGLFramebuffer | null;
  private readTarget: WebGLFramebuffer | null;
  private activeUnit: number;
  private boundTextures = new Map<number, WebGLTexture>();
  private vao: WebGLVertexArrayObject | null;
  private vaos = new Map<
    WebGLVertexArrayObject | null,
    { attributes: any[]; index: number | null }
  >();
  private bufferBindings = new Map<number, WebGLBuffer>();
  private params = new Map<WebGLTexture, any>();
  private state: any;
  private clearColor: number[];
  private clearDepth: number;
  private recording = false;
  private firstFrame = false; // Explicit parity audit only; no steady-state GL reads.
  private uniformMismatches: any[] = [];
  private commands: any[] = [];
  private uploads: any[] = [];
  private textureUploads: any[] = [];
  private allocations: any[] = [];
  private deletions: any[] = [];
  private nextBuffer = 0;
  private nextTexture = 0;
  private firstProblem?: string;
  private get problem() {
    return this.firstProblem;
  }
  private set problem(value: string | undefined) {
    if (value === undefined) this.firstProblem = undefined;
    else this.firstProblem ??= value;
  }
  private changedResource?: string;
  constructor(
    private gl: GL,
    readonly session: Session,
  ) {
    this.nextBuffer = session.buffers.size;
    this.nextTexture = session.textures.size;
    this.currentProgram = gl.getParameter(gl.CURRENT_PROGRAM);
    this.drawTarget = gl.getParameter(gl.DRAW_FRAMEBUFFER_BINDING);
    this.readTarget = gl.getParameter(gl.READ_FRAMEBUFFER_BINDING);
    this.activeUnit = gl.getParameter(gl.ACTIVE_TEXTURE) - gl.TEXTURE0;
    this.vao = gl.getParameter(gl.VERTEX_ARRAY_BINDING);
    for (
      let unit = 0;
      unit < gl.getParameter(gl.MAX_COMBINED_TEXTURE_IMAGE_UNITS);
      unit++
    ) {
      gl.activeTexture(gl.TEXTURE0 + unit);
      this.boundTextures.set(unit, gl.getParameter(gl.TEXTURE_BINDING_2D));
    }
    gl.activeTexture(gl.TEXTURE0 + this.activeUnit);
    this.bufferBindings.set(
      gl.ARRAY_BUFFER,
      gl.getParameter(gl.ARRAY_BUFFER_BINDING),
    );
    this.bufferBindings.set(
      gl.ELEMENT_ARRAY_BUFFER,
      gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING),
    );
    for (const [program, id] of session.programs) {
      const values: Record<string, any> = {};
      for (const u of session.manifest.programs[id].uniforms) {
        values[u.name] =
          u.size > 1
            ? Array.from({ length: u.size }, (_, i) => {
                const v = gl.getUniform(
                  program,
                  gl.getUniformLocation(
                    program,
                    u.name.replace("[0]", `[${i}]`),
                  )!,
                );
                return ArrayBuffer.isView(v) ? Array.from(v as any) : [v];
              }).flat()
            : plain(
                gl.getUniform(program, gl.getUniformLocation(program, u.name)!),
              );
      }
      this.uniforms.set(program, values);
    }
    this.state = {
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
      polygonOffset: null,
    };
    this.clearColor = plain(gl.getParameter(gl.COLOR_CLEAR_VALUE));
    this.clearDepth = gl.getParameter(gl.DEPTH_CLEAR_VALUE);
    for (const c of session.manifest.commands)
      if (c.op === "draw")
        for (const samplers of Object.values(c.samplers) as any[])
          for (const value of samplers) {
            const object = [...session.textures].find(
              ([, id]) => id === value.texture,
            )?.[0];
            if (object) this.params.set(object, value);
          }
    const hook = (
      name: string,
      track: (...args: any[]) => void,
      skip = false,
    ) => {
      const original = (gl as any)[name].bind(gl);
      this.originals.set(name, original);
      (gl as any)[name] = (...args: any[]) => {
        track(...args);
        if (!skip || !this.recording) return original(...args);
      };
    };
    hook("useProgram", (p) => {
      this.currentProgram = p;
    });
    hook("bindFramebuffer", (target, obj) => {
      if (target !== gl.READ_FRAMEBUFFER) this.drawTarget = obj;
      if (target !== gl.DRAW_FRAMEBUFFER) this.readTarget = obj;
    });
    hook("activeTexture", (v) => {
      this.activeUnit = v - gl.TEXTURE0;
    });
    hook("bindTexture", (target, t) => {
      if (target === gl.TEXTURE_2D) this.boundTextures.set(this.activeUnit, t);
    });
    hook("bindVertexArray", (v) => {
      this.vao = v;
    });
    hook("bindBuffer", (target, buffer) => {
      this.bufferBindings.set(target, buffer);
    });
    for (const name of [
      "vertexAttribPointer",
      "vertexAttribIPointer",
      "vertexAttribDivisor",
      "enableVertexAttribArray",
      "disableVertexAttribArray",
    ])
      hook(name, () => this.vaos.delete(this.vao));
    for (const name of [
      "uniform1f",
      "uniform2f",
      "uniform3f",
      "uniform4f",
      "uniform1i",
      "uniform2i",
      "uniform3i",
      "uniform4i",
      "uniform1ui",
      "uniform2ui",
      "uniform3ui",
      "uniform4ui",
      "uniform1fv",
      "uniform2fv",
      "uniform3fv",
      "uniform4fv",
      "uniform1iv",
      "uniform2iv",
      "uniform3iv",
      "uniform4iv",
      "uniform1uiv",
      "uniform2uiv",
      "uniform3uiv",
      "uniform4uiv",
      "uniformMatrix2fv",
      "uniformMatrix3fv",
      "uniformMatrix4fv",
    ])
      hook(name, (location, ...args) => {
        if (!location) return;
        const key = session.locations.get(location);
        if (!key) {
          this.problem = "Untracked uniform location";
          return;
        }
        let values = this.uniforms.get(key.program);
        if (!values) {
          values = {};
          this.uniforms.set(key.program, values);
        }
        const matrix = name.startsWith("uniformMatrix");
        const vector = name.endsWith("v");
        if (vector) {
          const source = args[matrix ? 1 : 0];
          const offset = args[matrix ? 2 : 1] ?? 0;
          const length = args[matrix ? 3 : 2] ?? source.length - offset;
          values[key.name] = Array.from(source).slice(offset, offset + length);
        } else values[key.name] = args.length === 1 ? args[0] : args;
      });
    let cull = gl.getParameter(gl.CULL_FACE_MODE),
      scissor = plain(gl.getParameter(gl.SCISSOR_BOX)),
      offset = [
        gl.getParameter(gl.POLYGON_OFFSET_FACTOR),
        gl.getParameter(gl.POLYGON_OFFSET_UNITS),
      ];
    const enable = (cap: number, on: boolean) => {
      if (cap === gl.DEPTH_TEST) this.state.depthTest = on;
      if (cap === gl.BLEND) this.state.blend = on;
      if (cap === gl.CULL_FACE) this.state.cull = on ? cull : 0;
      if (cap === gl.SCISSOR_TEST) this.state.scissor = on ? scissor : null;
      if (cap === gl.POLYGON_OFFSET_FILL)
        this.state.polygonOffset = on ? offset : null;
    };
    hook("enable", (cap) => enable(cap, true));
    hook("disable", (cap) => enable(cap, false));
    hook("cullFace", (v) => {
      cull = v;
      if (this.state.cull) this.state.cull = v;
    });
    hook("scissor", (...v) => {
      scissor = v;
      if (this.state.scissor) this.state.scissor = v;
    });
    hook("polygonOffset", (...v) => {
      offset = v;
      if (this.state.polygonOffset) this.state.polygonOffset = v;
    });
    for (const [name, key] of [
      ["depthMask", "depthWrite"],
      ["depthFunc", "depthFunc"],
      ["frontFace", "frontFace"],
    ])
      hook(name, (v) => (this.state[key] = v));
    hook("viewport", (...v) => (this.state.viewport = v));
    hook("colorMask", (...v) => (this.state.colorMask = v));
    hook("blendFunc", (src, dst) =>
      Object.assign(this.state, {
        blendSrc: src,
        blendDst: dst,
        blendSrcAlpha: src,
        blendDstAlpha: dst,
      }),
    );
    hook(
      "blendFuncSeparate",
      (blendSrc, blendDst, blendSrcAlpha, blendDstAlpha) =>
        Object.assign(this.state, {
          blendSrc,
          blendDst,
          blendSrcAlpha,
          blendDstAlpha,
        }),
    );
    hook("blendEquation", (v) =>
      Object.assign(this.state, { blendEquation: v, blendEquationAlpha: v }),
    );
    hook("blendEquationSeparate", (blendEquation, blendEquationAlpha) =>
      Object.assign(this.state, { blendEquation, blendEquationAlpha }),
    );
    hook("clearColor", (...v) => (this.clearColor = v));
    hook("clearDepth", (v) => (this.clearDepth = v));
    hook(
      "clear",
      (mask) => {
        if (this.recording)
          this.commands.push({
            op: "clear",
            target: this.target(this.drawTarget),
            mask,
            color: this.clearColor,
            depth: this.clearDepth,
            state: { ...this.state },
          });
      },
      true,
    );
    hook(
      "drawElements",
      (mode, count, type, first) => this.draw(mode, count, first, type, 1),
      true,
    );
    hook(
      "drawElementsInstanced",
      (mode, count, type, first, n) => this.draw(mode, count, first, type, n),
      true,
    );
    hook(
      "drawArrays",
      (mode, first, count) => this.draw(mode, count, first, 0, 1),
      true,
    );
    hook(
      "drawArraysInstanced",
      (mode, first, count, n) => this.draw(mode, count, first, 0, n),
      true,
    );
    hook(
      "blitFramebuffer",
      (...args) => {
        if (this.recording)
          this.commands.push({
            op: "blit",
            from: this.target(this.readTarget),
            to: this.target(this.drawTarget),
            args,
          });
      },
      true,
    );
    hook(
      "generateMipmap",
      (target) => {
        if (this.recording) {
          const id = session.textures.get(
            this.boundTextures.get(this.activeUnit)!,
          );
          if (target !== gl.TEXTURE_2D || id === undefined)
            this.problem = "New mip texture";
          else this.commands.push({ op: "mipmap", texture: id });
        }
      },
      true,
    );
    hook(
      "bufferSubData",
      (target, dst, source, srcOffset = 0, length?: number) => {
        const id = session.buffers.get(this.bufferBindings.get(target)!);
        if (id === undefined) {
          this.problem = "New buffer";
          return;
        }
        const view = source as ArrayBufferView & { BYTES_PER_ELEMENT?: number };
        const unit = view.BYTES_PER_ELEMENT ?? 1;
        const bytes = new Uint8Array(
          view.buffer,
          view.byteOffset + srcOffset * unit,
          (length ?? view.byteLength / unit - srcOffset) * unit,
        );
        // Binary encoding keeps large instance matrices out of numeric JSON arrays.
        let binary = "";
        for (let i = 0; i < bytes.length; i += 32768)
          binary += String.fromCharCode(...bytes.subarray(i, i + 32768));
        this.uploads.push({ buffer: id, offset: dst, data: btoa(binary) });
      },
    );
    const unpack = new Map<number, number>();
    for (const p of [
      gl.UNPACK_FLIP_Y_WEBGL,
      gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL,
      gl.UNPACK_ROW_LENGTH,
      gl.UNPACK_SKIP_PIXELS,
      gl.UNPACK_SKIP_ROWS,
    ])
      unpack.set(p, gl.getParameter(p));
    hook("pixelStorei", (p, v) => unpack.set(p, v));
    hook("texSubImage2D", (...args: any[]) => {
      const [target, level, x, y] = args;
      const object = this.boundTextures.get(this.activeUnit)!;
      const texture = session.textures.get(object);
      if (texture === undefined) return;
      try {
        if (target !== gl.TEXTURE_2D || level !== 0)
          throw new Error("Unsupported texture upload target/level");
        const explicit = args.length >= 9;
        const format = args[explicit ? 6 : 4],
          type = args[explicit ? 7 : 5],
          source = args[explicit ? 8 : 6];
        if (
          format !== gl.RGBA ||
          type !== gl.UNSIGNED_BYTE ||
          unpack.get(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL)
        )
          throw new Error("Unsupported texture upload format");
        let width = explicit ? args[4] : source.width,
          height = explicit ? args[5] : source.height,
          rows: Uint8Array;
        const skipX = unpack.get(gl.UNPACK_SKIP_PIXELS) || 0,
          skipY = unpack.get(gl.UNPACK_SKIP_ROWS) || 0;
        if (ArrayBuffer.isView(source)) {
          const bytes = new Uint8Array(
            source.buffer,
            source.byteOffset + (args[9] || 0),
            source.byteLength - (args[9] || 0),
          );
          rows = rgbaRows(
            bytes,
            width,
            height,
            unpack.get(gl.UNPACK_ROW_LENGTH) || width,
            skipX,
            skipY,
          );
        } else {
          const canvas = document.createElement("canvas");
          canvas.width = source.width;
          canvas.height = source.height;
          const context = canvas.getContext("2d", {
            willReadFrequently: true,
          })!;
          context.drawImage(source, 0, 0);
          const data = context.getImageData(
            0,
            0,
            canvas.width,
            canvas.height,
          ).data;
          rows = rgbaRows(
            data,
            width,
            height,
            canvas.width,
            skipX,
            skipY,
            !!unpack.get(gl.UNPACK_FLIP_Y_WEBGL),
          );
        }
        this.textureUploads.push({
          texture,
          x,
          y,
          width,
          height,
          data: encodeBytes(rows),
        });
      } catch (error) {
        this.changedResource = String(error);
        if (this.recording) this.problem = this.changedResource;
      }
    });
    hook("bufferData", (target, source) => {
      const object = this.bufferBindings.get(target);
      if (!object) return;
      let id = session.buffers.get(object);
      if (id === undefined) {
        id = this.nextBuffer++;
        session.buffers.set(object, id);
      }
      const bytes =
        typeof source === "number"
          ? new Uint8Array(source)
          : ArrayBuffer.isView(source)
            ? new Uint8Array(
                source.buffer,
                source.byteOffset,
                source.byteLength,
              )
            : new Uint8Array(source);
      this.allocations.push({ kind: "buffer", id, length: bytes.byteLength });
      this.uploads.push({ buffer: id, offset: 0, data: encodeBytes(bytes) });
    });
    hook("texStorage2D", (target, levels, format, width, height) => {
      const object = this.boundTextures.get(this.activeUnit);
      if (!object || target !== gl.TEXTURE_2D) return;
      let id = session.textures.get(object);
      if (id !== undefined) {
        this.changedResource = "Render texture allocation changed";
        return;
      }
      id = this.nextTexture++;
      session.textures.set(object, id);
      this.allocations.push({
        kind: "texture",
        id,
        width,
        height,
        format,
        levels,
      });
    });
    hook("texImage2D", () => {
      this.changedResource = "Mutable texture allocation changed";
    });
    hook("deleteTexture", (object) => {
      const id = session.textures.get(object);
      if (id !== undefined) {
        this.deletions.push({ kind: "texture", id });
        session.textures.delete(object);
        this.params.delete(object);
      }
    });
    hook("deleteBuffer", (object) => {
      const id = session.buffers.get(object);
      if (id !== undefined) {
        this.deletions.push({ kind: "buffer", id });
        session.buffers.delete(object);
      }
    });
  }
  private target(obj: WebGLFramebuffer | null) {
    const id = this.session.targets.get(obj);
    if (id === undefined) this.problem = "New render target";
    return id ?? 0;
  }
  private draw(
    mode: number,
    count: number,
    first: number,
    indexType: number,
    instances: number,
  ) {
    if (!this.recording) return;
    const gl = this.gl;
    const pid = this.session.programs.get(this.currentProgram);
    if (pid === undefined) {
      this.problem = "New shader variant";
      return;
    }
    let geometry = this.vaos.get(this.vao);
    if (!geometry) {
      const attributes: any[] = [];
      for (const a of this.session.manifest.programs[pid].attributes) {
        const columns =
          a.type === gl.FLOAT_MAT4 ? 4 : a.type === gl.FLOAT_MAT3 ? 3 : 1;
        for (let column = 0; column < columns; column++) {
          const loc = a.location + column;
          const enabled = gl.getVertexAttrib(
            loc,
            gl.VERTEX_ATTRIB_ARRAY_ENABLED,
          );
          const buffer = enabled
            ? this.session.buffers.get(
                gl.getVertexAttrib(loc, gl.VERTEX_ATTRIB_ARRAY_BUFFER_BINDING),
              )
            : undefined;
          if (enabled && buffer === undefined)
            this.problem = "Uncaptured geometry buffer";
          attributes.push({
            name: a.name,
            column,
            location: loc,
            enabled,
            ...(enabled
              ? {
                  buffer,
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
      const obj = gl.getParameter(gl.ELEMENT_ARRAY_BUFFER_BINDING);
      const index = obj ? this.session.buffers.get(obj) : null;
      if (index === undefined) this.problem = "Uncaptured index buffer";
      geometry = { attributes, index: index ?? null };
      this.vaos.set(this.vao, geometry);
    }
    const uniforms = { ...this.uniforms.get(this.currentProgram) },
      samplers: Record<string, any> = {};
    for (const u of this.session.manifest.programs[pid].uniforms)
      if ([gl.SAMPLER_2D, gl.SAMPLER_2D_SHADOW].includes(u.type)) {
        const value = uniforms[u.name];
        samplers[u.name] = [];
        for (const unit of typeof value === "number"
          ? [value]
          : (value ?? [])) {
          const obj = this.boundTextures.get(unit)!;
          const texture = this.session.textures.get(obj);
          let params = this.params.get(obj);
          if (texture !== undefined && !params) {
            const active = this.activeUnit;
            gl.activeTexture(gl.TEXTURE0 + unit);
            const extension = gl.getExtension("EXT_texture_filter_anisotropic");
            params = {
              texture,
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
              anisotropy: extension
                ? gl.getTexParameter(
                    gl.TEXTURE_2D,
                    extension.TEXTURE_MAX_ANISOTROPY_EXT,
                  )
                : 1,
            };
            this.params.set(obj, params);
            gl.activeTexture(gl.TEXTURE0 + active);
          }
          if (texture === undefined || !params) {
            this.problem = `Uncaptured sampled texture: ${u.name}, unit ${unit}, id ${texture}, program ${pid}, params ${!!params}`;
            continue;
          }
          samplers[u.name].push({ ...params, texture });
        }
      }
    if (this.firstFrame)
      for (const u of this.session.manifest.programs[pid].uniforms) {
        const actual =
          u.size > 1
            ? Array.from({ length: u.size }, (_, i) => {
                const v = gl.getUniform(
                  this.currentProgram,
                  gl.getUniformLocation(
                    this.currentProgram,
                    u.name.replace("[0]", `[${i}]`),
                  )!,
                );
                return ArrayBuffer.isView(v) ? Array.from(v as any) : [v];
              }).flat()
            : plain(
                gl.getUniform(
                  this.currentProgram,
                  gl.getUniformLocation(this.currentProgram, u.name)!,
                ),
              );
        if (JSON.stringify(actual) !== JSON.stringify(uniforms[u.name]))
          this.uniformMismatches.push({
            program: pid,
            name: u.name,
            actual,
            recorded: uniforms[u.name],
          });
      }
    this.commands.push({
      op: "draw",
      target: this.target(this.drawTarget),
      program: pid,
      mode,
      count,
      first,
      indexType,
      instances,
      ...geometry,
      uniforms,
      samplers,
      state: { ...this.state },
    });
  }
  frame(render: () => void) {
    this.commands = [];
    this.problem = this.changedResource;
    this.recording = true;
    try {
      render();
    } finally {
      this.recording = false;
    }
    if (this.problem) throw new Error(this.problem);
    this.firstFrame = false;
    const uniformMismatches = this.uniformMismatches;
    this.uniformMismatches = [];
    const textureUploads = this.textureUploads;
    this.textureUploads = [];
    const uploads = this.uploads,
      allocations = this.allocations,
      deletions = this.deletions;
    this.uploads = [];
    this.allocations = [];
    this.deletions = [];
    return {
      commands: this.commands,
      uploads,
      textureUploads,
      allocations,
      deletions,
      uniformMismatches,
    };
  }
  dispose() {
    for (const [name, original] of this.originals)
      (this.gl as any)[name] = original;
    this.originals.clear();
  }
}
