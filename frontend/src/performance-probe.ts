import type { ArchiveScene } from "./scene";
import { isNative, nativeInvoke } from "./native";
type Sample = {
  interval: number;
  cpu: number;
  calls: number;
  triangles: number;
};
type Input = {
  action: string;
  scheduledMs: number;
  timerDelayMs: number;
  handlerMs: number;
  issuedAt: number;
  nextSubmissionMs?: number;
  changedPoseMs?: number;
  pose: number[];
};
let active:
  | {
      label: string;
      start: number;
      until: number;
      previous: number;
      samples: Sample[];
      metadata: unknown;
      viewport: number[];
      dpr: number;
      canvas: number[];
      interruptions: string[];
      interrupted: boolean;
      inputs: Input[];
      maxPending: number;
      finalState?: unknown;
    }
  | undefined;
const percentile = (items: number[], q: number) => {
  const sorted = [...items].sort((a, b) => a - b);
  return (
    sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * q))] ?? 0
  );
};
export function beginMeasurement(
  label: string,
  seconds: number,
  metadata: unknown,
  scene?: ArchiveScene,
) {
  if (active) return false;
  const now = performance.now();
  active = {
    label,
    start: now,
    until: now + seconds * 1000,
    previous: 0,
    samples: [],
    metadata,
    viewport: [innerWidth, innerHeight],
    dpr: devicePixelRatio,
    canvas: scene ? [scene.renderer.domElement.width, scene.renderer.domElement.height] : [],
    interruptions: [],
    interrupted: false,
    inputs: [],
    maxPending: 0,
  };
  document.documentElement.dataset.measurement = label;
  return true;
}
export function recordInteraction(
  action: string,
  scheduledAt: number,
  run: () => void,
  scene: ArchiveScene,
) {
  if (!active) return;
  const issuedAt = performance.now(),
    pose = scene.interactionPose();
  run();
  active.inputs.push({
    action,
    scheduledMs: scheduledAt - active.start,
    timerDelayMs: Math.max(0, issuedAt - scheduledAt),
    handlerMs: performance.now() - issuedAt,
    issuedAt,
    pose,
  });
}
export function sampleFrame(
  now: number,
  cpu: number,
  scene: ArchiveScene,
  interactionState?: { pending: boolean; albumId?: string; phase: string },
) {
  if (!active) return;
  if (document.hidden) {
    active.interrupted = true;
    if(!active.interruptions.includes("hidden")) active.interruptions.push("hidden");
    active.previous = 0;
    return;
  }
  if (
    innerWidth !== active.viewport[0] ||
    innerHeight !== active.viewport[1] ||
    devicePixelRatio !== active.dpr
  ) {
    active.interrupted = true;
    const change=`display: ${innerWidth}x${innerHeight}@${devicePixelRatio}, started ${active.viewport.join("x")}@${active.dpr}`;
    if(!active.interruptions.includes(change)) active.interruptions.push(change);
  }
  if(active.canvas.length && (scene.renderer.domElement.width!==active.canvas[0] || scene.renderer.domElement.height!==active.canvas[1])) {
    active.interrupted=true;
    if(!active.interruptions.includes("render-buffer-resized"))active.interruptions.push("render-buffer-resized");
  }
  if (active.previous)
    active.samples.push({
      interval: now - active.previous,
      cpu,
      calls: scene.renderer.info.render.calls,
      triangles: scene.renderer.info.render.triangles,
    });
  active.previous = now;
  if (active.inputs.length) {
    const submitted = performance.now(),
      pose = scene.interactionPose();
    for (const input of active.inputs) {
      input.nextSubmissionMs ??= submitted - input.issuedAt;
      if (
        input.changedPoseMs === undefined &&
        pose.some((value, i) => Math.abs(value - input.pose[i]) > 1e-6)
      )
        input.changedPoseMs = submitted - input.issuedAt;
    }
    active.maxPending = Math.max(
      active.maxPending,
      interactionState?.pending ? 1 : 0,
    );
    active.finalState = interactionState;
  }
  if (now < active.until) return;
  const m = active;
  active = undefined;
  const intervals = m.samples.map((s) => s.interval),
    cpus = m.samples.map((s) => s.cpu);
  const total = intervals.reduce((a, b) => a + b, 0);
  const summary = (values: number[]) => ({
    p50: percentile(values, 0.5),
    p95: percentile(values, 0.95),
    p99: percentile(values, 0.99),
    max: Math.max(0, ...values),
  });
  const coverage = total / (m.until - m.start);
  const valid =
    !m.interrupted &&
    m.samples.length > 0 &&
    coverage >= 0.9 &&
    now - m.until < 2000;
  const report = {
    valid,
    interruptions: m.interruptions,
    startingDevicePixelRatio: m.dpr,
    startingCanvas: m.canvas,
    coverage,
    invalidReason: valid
      ? undefined
      : "Window hidden, display changed, or frame callbacks suspended; do not compare this run.",
    optimization:
      "v4: v3 rendering plus optional fused bokeh/output and transmission depth prepass; shared cover cache and elapsed-time motion in both modes",
    label: m.label,
    measuredAt: new Date().toISOString(),
    runtime: isNative ? "Tauri / macOS WKWebView" : "browser",
    userAgent: navigator.userAgent,
    viewport: m.viewport,
    devicePixelRatio: m.dpr,
    canvas: [scene.renderer.domElement.width, scene.renderer.domElement.height],
    metadata: m.metadata,
    samples: m.samples.length,
    elapsedMs: now - m.start,
    fps: (1000 * m.samples.length) / total,
    frameMs: summary(intervals),
    framesOver50Ms: intervals.filter((x) => x > 50).length,
    cpuSubmissionMs: summary(cpus),
    drawCalls: percentile(
      m.samples.map((s) => s.calls),
      0.5,
    ),
    triangles: percentile(
      m.samples.map((s) => s.triangles),
      0.5,
    ),
    gpuMs: null,
    sceneStats: scene.getStats(),
    postFusionEnabled: scene.postFusionEnabled,
    transmissionDepthEnabled: scene.transmissionDepthEnabled,
    benchmarkDevicePixelRatio: scene.benchmarkDevicePixelRatio ?? null,
    interactions: m.inputs.length
      ? {
          count: m.inputs.length,
          timerDelayMs: summary(m.inputs.map((x) => x.timerDelayMs)),
          handlerMs: summary(m.inputs.map((x) => x.handlerMs)),
          nextSubmissionMs: summary(
            m.inputs.flatMap((x) =>
              x.nextSubmissionMs === undefined ? [] : [x.nextSubmissionMs],
            ),
          ),
          maxPendingSelection: m.maxPending,
          finalState: m.finalState,
          raw: m.inputs,
        }
      : undefined,
    note: "CPU submission is not GPU execution. Interaction replay invokes the same application handlers; timer delay and input-to-render-submission are measured, not OS-to-photon latency. A changing pose during existing motion does not prove a causal response to the new input.",
    raw: m.samples,
  };
  document.documentElement.dataset.measurement = JSON.stringify(report);
  console.info("RHINE_PERFORMANCE", report);
  if (isNative) void nativeInvoke("save_benchmark", { report });
}
