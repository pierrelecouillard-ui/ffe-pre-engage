type Json = Record<string, unknown>;

const API_BASE = "https://ffe-pre-engage.onrender.com";

export async function invoke<T = unknown>(cmd: string, args?: Json): Promise<T> {
  const tauri = (window as any).__TAURI__;

  // MODE DESKTOP TAURI
  if (tauri?.core?.invoke) {
    return tauri.core.invoke(cmd, args);
  }

  // MODE WEB (iPhone / GitHub Pages)
  const res = await fetch(`${API_BASE}/api/${cmd}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args ?? {})
  });

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`API ${res.status}: ${text}`);
  }

  return res.json() as Promise<T>;
}
export default invoke;
