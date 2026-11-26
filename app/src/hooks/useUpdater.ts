import { useCallback, useEffect, useState } from "react";
import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";
import { openUrl } from "@tauri-apps/plugin-opener";
import { api, isTauriRuntime } from "../lib/tauri";
import type { GitHubReleaseInfo } from "../types/telemetry";

export type UpdateStage =
  | "idle"
  | "checking"
  | "up-to-date"
  | "available"
  | "downloading"
  | "installing"
  | "error";

export function useUpdater(autoCheck = true) {
  const [stage, setStage] = useState<UpdateStage>("idle");
  const [releaseInfo, setReleaseInfo] = useState<GitHubReleaseInfo | null>(null);
  const [tauriUpdate, setTauriUpdate] = useState<Update | null>(null);
  const [progressPct, setProgressPct] = useState<number>(0);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const checkForUpdates = useCallback(async (silent = false) => {
    if (!isTauriRuntime()) return;
    if (!silent) setStage("checking");
    setErrorMsg(null);
    try {
      const gh = await api.checkGitHubRelease();
      setReleaseInfo(gh);

      // Try native signed Tauri updater first
      try {
        const upd = await check();
        if (upd) {
          setTauriUpdate(upd);
          setStage("available");
          return;
        }
      } catch {
        // Unsigned local dev build or no latest.json yet — gracefully fall back to GitHub release info
      }

      if (gh.hasUpdate) {
        setStage("available");
      } else if (!silent) {
        setStage("up-to-date");
      } else {
        setStage("idle");
      }
    } catch (e) {
      if (!silent) {
        setStage("error");
        setErrorMsg(e instanceof Error ? e.message : String(e));
      }
    }
  }, []);

  useEffect(() => {
    if (!autoCheck || !isTauriRuntime()) return;
    const timer = setTimeout(() => {
      checkForUpdates(true);
    }, 4000);
    return () => clearTimeout(timer);
  }, [autoCheck, checkForUpdates]);

  const installUpdate = useCallback(async () => {
    if (tauriUpdate) {
      setStage("downloading");
      setProgressPct(0);
      let downloaded = 0;
      let total = 0;
      try {
        await tauriUpdate.downloadAndInstall((event) => {
          if (event.event === "Started") {
            total = event.data.contentLength ?? 0;
          } else if (event.event === "Progress") {
            downloaded += event.data.chunkLength;
            if (total > 0) {
              setProgressPct(Math.min(99, Math.round((downloaded / total) * 100)));
            }
          } else if (event.event === "Finished") {
            setProgressPct(100);
            setStage("installing");
          }
        });
        await relaunch();
      } catch (e) {
        setStage("error");
        setErrorMsg(e instanceof Error ? e.message : String(e));
      }
      return;
    }
    if (releaseInfo?.releaseUrl) {
      await openUrl(releaseInfo.releaseUrl);
    }
  }, [tauriUpdate, releaseInfo]);

  return {
    stage,
    releaseInfo,
    hasNativeUpdater: Boolean(tauriUpdate),
    progressPct,
    errorMsg,
    checkForUpdates,
    installUpdate,
  };
}
