import { isNative, nativeInvoke } from "./native";

let pending = Promise.resolve();

/** Keep rapid changes ordered without blocking the content's theme transition. */
export function syncWindowTheme(theme: "day" | "night") {
  if (!isNative) return;
  pending = pending
    .then(() => nativeInvoke<void>("set_window_theme", { theme }))
    .catch(error => console.warn("Unable to sync native window theme", error));
}
