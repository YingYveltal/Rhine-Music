import { mergeUpdates } from "./metal-data";
import { nativeInvoke } from "./native";
import { GLFrameCapture } from "./gl-frame-capture";
import { MetalLiveRecorder } from "./metal-live";
import { invalidateMeasurement, recordNativeCompletion } from "./performance-probe";
import type { WebGLRenderer } from "three";

export class MetalController {
  private recorder?: MetalLiveRecorder;
  private pending?: { frame: unknown; issuedAt: number };
  private inFlight = false;
  private generation = 0;
  private fpsSince = 0;
  private fpsFrames = 0;
  preparing = false;
  stats = {
    active: false,
    completedFrames: 0,
    completedFps: 0,
    gpuMs: 0,
    cpuMs: 0,
    roundTripMs: 0,
    replacedFrames: 0,
    error: "",
  };
  constructor(
    private renderer: WebGLRenderer,
    private capture: GLFrameCapture,
  ) {
    const style = document.createElement("style");
    style.textContent = `html[data-native-metal="active"],html[data-native-metal="active"] body,html[data-native-metal="active"] #viewport,html[data-native-metal="active"] #stage{background:transparent!important}html[data-native-metal="active"] .three-scene canvas{visibility:hidden!important}`;
    document.head.append(style);
  }
  async toggle(render: () => void, metadata: Record<string, unknown>) {
    if (this.preparing) return;
    if (this.recorder) {
      await this.stop();
      return;
    }
    this.preparing = true;
    this.stats.error = "";
    try {
      await nativeInvoke("metal_capabilities");
      document.documentElement.dataset.nativeMetal = "preparing";
      const id = await this.capture.capture(render, metadata);
      await nativeInvoke("metal_prepare", { id });
      this.recorder = new MetalLiveRecorder(
        this.renderer.getContext() as WebGL2RenderingContext,
        this.capture.session!,
      );
      this.generation++;
      this.stats.active = true;
      this.stats.completedFrames = 0;
      this.stats.replacedFrames = 0;
      this.stats.completedFps = 0;
      this.fpsSince = performance.now();
      this.fpsFrames = 0;
      document.documentElement.dataset.nativeMetal = "ready";
    } catch (error) {
      this.stats.error = String(error);
      void nativeInvoke("save_benchmark", {
        report: { label: "metal-prepare-error", message: String(error) },
      });
      await this.stop();
      throw error;
    } finally {
      this.preparing = false;
    }
  }
  render(render: () => void) {
    if (!this.recorder) {
      render();
      return;
    }
    try {
      const frame = this.recorder.frame(render);
      if (this.pending) {
        this.stats.replacedFrames++;
        // A discarded pose may contain a buffer delta not repeated by the next
        // pose. Coalesce ranges in last-write order instead of losing updates.
        frame.uploads = mergeUpdates(
          (this.pending.frame as any).uploads,
          frame.uploads,
          (u: any) => `${u.buffer}:${u.offset}:${u.data.length}`,
        );
        frame.allocations = mergeUpdates(
          (this.pending.frame as any).allocations,
          frame.allocations,
          (u: any) => `${u.kind}:${u.id}`,
        );
        frame.deletions = mergeUpdates(
          (this.pending.frame as any).deletions,
          frame.deletions,
          (u: any) => `${u.kind}:${u.id}`,
        );
        frame.textureUploads = mergeUpdates(
          (this.pending.frame as any).textureUploads,
          frame.textureUploads,
          (u: any) => `${u.texture}:${u.x}:${u.y}:${u.width}:${u.height}`,
        );
      }
      this.pending = { frame, issuedAt: performance.now() };
      void this.submit();
    } catch (error) {
      this.fallback(error);
      render();
    }
  }
  private async submit() {
    if (this.inFlight || !this.pending || !this.recorder) return;
    const request = this.pending;
    this.pending = undefined;
    this.inFlight = true;
    const generation = this.generation;
    try {
      const response = await nativeInvoke<{
        gpuMs: number;
        cpuMs: number;
        frames: number;
        paused?: boolean;
        profiled?: boolean;
      }>("metal_frame", { frame: request.frame });
      if (generation !== this.generation) return;
      if (response.profiled) invalidateMeasurement("Diagnostic GPU profiling interrupted this run");
      if (!response.paused) {
        document.documentElement.dataset.nativeMetal = "active";
        if (this.stats.completedFrames === 0)
          void nativeInvoke("save_benchmark", {
            report: {
              label: "metal-first-frame",
              captureId: this.capture.session?.id,
              ...response,
            },
          });
        const completed = performance.now();
        Object.assign(this.stats, {
          completedFrames: this.stats.completedFrames + 1,
          gpuMs: response.gpuMs,
          cpuMs: response.cpuMs,
          roundTripMs: completed - request.issuedAt,
        });
        this.fpsFrames++;
        if (completed - this.fpsSince >= 1000) {
          this.stats.completedFps =
            (1000 * this.fpsFrames) / (completed - this.fpsSince);
          this.fpsFrames = 0;
          this.fpsSince = completed;
        }
        recordNativeCompletion(
          request.issuedAt,
          completed,
          response.gpuMs,
          response.cpuMs,
        );
        document.documentElement.dataset.nativeMetal = "active";
      }
    } catch (error) {
      if (generation === this.generation) this.fallback(error);
    } finally {
      this.inFlight = false;
      if (this.pending && generation === this.generation) void this.submit();
    }
  }
  private fallback(error: unknown) {
    this.stats.error = String(error);
    console.warn("RHINE_METAL_FALLBACK", error);
    void nativeInvoke("save_benchmark", {
      report: { label: "metal-fallback", message: String(error) },
    });
    void this.stop();
  }
  async stop() {
    this.generation++;
    this.pending = undefined;
    this.recorder?.dispose();
    this.recorder = undefined;
    this.stats.active = false;
    delete document.documentElement.dataset.nativeMetal;
    await nativeInvoke("metal_stop").catch(() => {});
  }
}
