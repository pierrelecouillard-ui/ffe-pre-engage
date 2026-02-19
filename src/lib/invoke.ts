type InvokeFn = <T = unknown>(cmd: string, args?: Record<string, unknown>) => Promise<T>;

export const invokeSafe: InvokeFn = async (cmd, args) => {
  // Tauri v2 expose généralement window.__TAURI__ en runtime Tauri
  const tauri = (window as any).__TAURI__;

  if (tauri?.core?.invoke) {
    return tauri.core.invoke(cmd, args);
  }

  // Mode Web (GitHub Pages) : remplace par une API HTTP
  // Exemple : ton serveur doit exposer /api/<cmd>
  const res = await fetch(`/api/${cmd}`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(args ?? {})
  });

  if (!res.ok) throw new Error(`Web API error ${res.status}`);
  return res.json() as Promise<any>;
};
