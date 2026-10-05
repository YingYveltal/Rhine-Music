import type { MusicLibrary, MusicTrack } from "./music-types";
import type { MusicPlayerState } from "./music-player";
type Invoke = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;
const tauri = (
  window as unknown as {
    __TAURI__?: {
      core: { invoke: Invoke; convertFileSrc(path: string): string };
    };
  }
).__TAURI__;
export const isNative = !!tauri;
if (isNative) {
  document.addEventListener("click", (event) => {
    const link = (event.target as Element | null)?.closest<HTMLAnchorElement>('a[target="_blank"]');
    if (!link || event.defaultPrevented) return;
    event.preventDefault();
    void nativeInvoke("open_link", {href:link.href}).catch(error => console.error("Unable to open link", error));
  });
  window.addEventListener("error", (event) => {
    void nativeInvoke("save_benchmark", {
      report: {
        label: "runtime-error",
        message: event.message,
        stack: event.error?.stack,
      },
    }).catch(() => {});
  });
  window.addEventListener("unhandledrejection", (event) => {
    void nativeInvoke("save_benchmark", {
      report: {
        label: "runtime-rejection",
        message: String(event.reason),
        stack: event.reason?.stack,
      },
    }).catch(() => {});
  });
}
export function nativeInvoke<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  return tauri!.core.invoke<T>(command, args).catch((error) => {
    throw new Error(String(error));
  });
}
export async function nativeRequest<T>(
  route: string,
  body?: unknown,
): Promise<T> {
  const result = await nativeInvoke<T>("music_request", {
    route,
    body: body ?? null,
  });
  const library = result as MusicLibrary;
  if (library.albums)
    for (const album of library.albums) {
      const path = (album as unknown as { nativeCoverPath?: string })
        .nativeCoverPath;
      if (path) album.coverUrl = tauri!.core.convertFileSrc(path);
    }
  return result;
}
type Options = {
  volume?: number;
  songFadeEnabled?: boolean;
  bgmVolume?: number;
  bgmEnabled?: boolean;
};
type NativeState = {
  track: MusicTrack | null;
  playing: boolean;
  elapsed: number;
  error: string | null;
  transport: MusicPlayerState["transport"];
  bgmPlaying: boolean;
  bgmError: string | null;
};
export class NativeMusicPlayer {
  private value: MusicPlayerState;
  private listeners = new Set<(state: MusicPlayerState) => void>();
  private disposed = false;
  private timer?: ReturnType<typeof setTimeout>;
  private generation = 0;
  private commands = Promise.resolve();
  constructor(options: Options = {}) {
    this.value = {
      transport: "idle",
      currentTrack: null,
      queue: [],
      currentIndex: -1,
      playing: false,
      loading: false,
      duration: 0,
      currentTime: 0,
      volume: options.volume ?? 0.65,
      songFadeEnabled: options.songFadeEnabled ?? true,
      bgmVolume: options.bgmVolume ?? 0.18,
      bgmEnabled: options.bgmEnabled ?? true,
      bgmPlaying: false,
      error: null,
      bgmError: null,
      backend: "rust",
    };
    this.settings();
    void this.poll();
    document.addEventListener("pointerdown", this.unlock, {
      once: true,
      capture: true,
    });
    document.addEventListener("keydown", this.unlock, {
      once: true,
      capture: true,
    });
  }
  private unlock = () => {
    void this.send("unlock");
  };
  get state(): MusicPlayerState {
    return { ...this.value, queue: [...this.value.queue] };
  }
  subscribe(listener: (state: MusicPlayerState) => void) {
    this.listeners.add(listener);
    listener(this.state);
    return () => this.listeners.delete(listener);
  }
  private emit() {
    for (const listener of this.listeners) listener(this.state);
  }
  private send(operation: string, args: Record<string, unknown> = {}) {
    this.commands = this.commands
      .catch(() => {})
      .then(() =>
        nativeInvoke<void>("player_command", {
          operation,
          id: null,
          ids: null,
          value: null,
          ...args,
        }),
      )
      .catch((error) => {
        this.value.error = String(error);
        this.value.transport = "error";
        this.value.loading = false;
        this.emit();
      });
    return this.commands;
  }
  private async poll() {
    if (this.disposed) return;
    const generation = this.generation;
    try {
      const state = await nativeInvoke<NativeState>("player_state");
      if (generation === this.generation && !this.disposed) {
        const currentTrack = state.track
          ? { ...this.value.queue.find((t) => t.id === state.track!.id), ...state.track }
          : this.value.currentTrack;
        Object.assign(this.value, {
          currentTrack,
          currentIndex: this.value.queue.findIndex(
            (t) => t.id === currentTrack?.id,
          ),
          playing: state.playing,
          currentTime: state.elapsed,
          duration: currentTrack?.duration ?? 0,
          transport: state.transport || "idle",
          loading: state.transport === "loading",
          error: state.error,
          bgmPlaying: state.bgmPlaying,
          bgmError: state.bgmError,
        });
        this.emit();
      }
    } catch (error) {
      this.value.error = String(error);
      this.emit();
    }
    if (!this.disposed)
      this.timer = setTimeout(
        () => void this.poll(),
        document.hidden ? 600 : 150,
      );
  }
  setQueue(tracks: readonly MusicTrack[]) {
    this.value.queue = [...new Map(tracks.map((t) => [t.id, t])).values()];
    this.value.currentIndex = this.value.queue.findIndex(
      (t) => t.id === this.value.currentTrack?.id,
    );
    if (this.value.currentTrack && this.value.currentIndex < 0) {
      this.stop();
      this.value.currentTrack = null;
    }
    this.emit();
  }
  async play(id: string, tracks: readonly MusicTrack[] = this.value.queue) {
    this.generation++;
    this.value.queue = [...new Map(tracks.map((t) => [t.id, t])).values()];
    const index = this.value.queue.findIndex((t) => t.id === id);
    if (index < 0) return;
    this.value.currentTrack = this.value.queue[index];
    this.value.currentIndex = index;
    this.value.loading = true;
    this.value.transport = "loading";
    this.emit();
    await this.send("play", { id, ids: this.value.queue.map((t) => t.id) });
  }
  async toggle() {
    this.generation++;
    if (
      this.value.transport === "idle" ||
      this.value.transport === "error" ||
      this.value.transport === "paused"
    ) {
      const track = this.value.currentTrack ?? this.value.queue[0];
      if (track) await this.play(track.id);
    } else await this.send("toggle");
  }
  stop() {
    this.generation++;
    this.value.transport = "idle";
    this.value.playing = false;
    this.value.loading = false;
    this.value.currentTime = 0;
    this.emit();
    void this.send("stop");
  }
  async next() {
    const track = this.value.queue[this.value.currentIndex + 1];
    if (track) await this.play(track.id);
    else this.stop();
  }
  async previous() {
    if (this.value.currentTime > 3) this.seek(0);
    else {
      const track = this.value.queue[Math.max(0, this.value.currentIndex - 1)];
      if (track) await this.play(track.id);
    }
  }
  seek(position: number) {
    if (Number.isFinite(position))
      void this.send("seek", { value: Math.max(0, position) });
  }
  setVolume(value: number) {
    if (Number.isFinite(value))
      this.value.volume = Math.max(0, Math.min(1, value));
    this.settings();
  }
  setBgmVolume(value: number) {
    if (Number.isFinite(value))
      this.value.bgmVolume = Math.max(0, Math.min(1, value));
    this.settings();
  }
  setBgmEnabled(enabled: boolean) {
    this.value.bgmEnabled = enabled;
    this.settings();
  }
  setSongFadeEnabled(enabled: boolean) {
    this.value.songFadeEnabled = enabled;
    this.settings();
  }
  private settings() {
    void this.send("settings", {
      value: {
        volume: this.value.volume,
        bgmVolume: this.value.bgmVolume,
        bgmEnabled: this.value.bgmEnabled,
        songFadeEnabled: this.value.songFadeEnabled,
      },
    });
    this.emit();
  }
  dispose() {
    this.disposed = true;
    clearTimeout(this.timer);
    this.listeners.clear();
    document.removeEventListener("pointerdown", this.unlock, true);
    document.removeEventListener("keydown", this.unlock, true);
  }
}
