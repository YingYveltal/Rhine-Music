// Keep the user's unsaved textarea intact; only append new picker results.
export function appendMusicFolders(current: string, selected: string[] | null) {
  const key = (path: string) => path.trim().replace(/\/+$/, "") || "/";
  const seen = new Set(current.split(/\r?\n/).filter(line => line.trim()).map(key));
  const added: string[] = [];
  for (const path of selected || []) {
    if (!path || seen.has(key(path))) continue;
    seen.add(key(path));
    added.push(path);
  }
  return {
    value: added.length ? current + (current && !current.endsWith("\n") ? "\n" : "") + added.join("\n") : current,
    added: added.length,
  };
}
