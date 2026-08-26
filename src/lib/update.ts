import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";

export type UpdateProgress = {
  downloaded: number;
  total: number | null;
};

export async function checkForUpdate(): Promise<Update | null> {
  return check({ timeout: 30_000 });
}

export async function installUpdate(
  update: Update,
  onProgress: (progress: UpdateProgress) => void,
): Promise<void> {
  let downloaded = 0;
  let total: number | null = null;
  await update.downloadAndInstall(
    (event: DownloadEvent) => {
      if (event.event === "Started") {
        total = event.data.contentLength ?? null;
        downloaded = 0;
      } else if (event.event === "Progress") {
        downloaded += event.data.chunkLength;
      } else {
        downloaded = total ?? downloaded;
      }
      onProgress({ downloaded, total });
    },
    { timeout: 10 * 60 * 1000 },
  );
  await relaunch();
}
