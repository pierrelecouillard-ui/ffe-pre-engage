import { invoke } from "@tauri-apps/api/core";

export type Target = {
  id: number;
  label: string;
  url: string;
  cheval?: string | null;
  cavalier?: string | null;
  interval_normal_sec: number;
  interval_hot_sec: number;
  hot_from?: string | null;
  hot_to?: string | null;
  last_status: string;
  last_checked_at?: number | null;
  last_error?: string | null;
};

export type Epreuve = { label: string; url: string };

export async function listEpreuves(contestId: string): Promise<Epreuve[]> {
  return await invoke("list_epreuves", { contestId });
}

export async function listTargets(): Promise<Target[]> {
  return await invoke("list_targets");
}

export async function addTarget(payload: {
  label: string;
  url: string;
  cheval?: string | null;
  cavalier?: string | null;
  interval_normal_sec?: number;
  interval_hot_sec?: number;
  hot_from?: string | null;
  hot_to?: string | null;
}): Promise<void> {
  await invoke("add_target", { payload });
}

export async function deleteTarget(id: number): Promise<void> {
  await invoke("delete_target", { id });
}

export async function startWatcher(): Promise<void> {
  await invoke("start_watcher");
}

export async function stopWatcher(): Promise<void> {
  await invoke("stop_watcher");
}

export async function openUrl(url: string): Promise<void> {
  await invoke("open_url", { url });
}
