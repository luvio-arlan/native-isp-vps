import React from "react";
import { ArrowUpCircle, Download, Loader2 } from "lucide-react";
import type { GitHubReleaseInfo } from "../types/telemetry";
import type { UpdateStage } from "../hooks/useUpdater";

interface UpdateBannerProps {
  stage: UpdateStage;
  releaseInfo: GitHubReleaseInfo | null;
  hasNativeUpdater: boolean;
  progressPct: number;
  onInstall: () => void;
}

export const UpdateBanner: React.FC<UpdateBannerProps> = ({
  stage,
  releaseInfo,
  hasNativeUpdater,
  progressPct,
  onInstall,
}) => {
  if (stage !== "available" && stage !== "downloading" && stage !== "installing") {
    return null;
  }

  return (
    <button
      onClick={onInstall}
      disabled={stage === "downloading" || stage === "installing"}
      className="titlebar-nodrag inline-flex items-center gap-1.5 rounded-full border border-emerald-500/35 bg-emerald-500/15 px-3 py-1 text-[11px] font-medium text-emerald-200 transition hover:bg-emerald-500/25 disabled:cursor-wait"
    >
      {stage === "downloading" || stage === "installing" ? (
        <>
          <Loader2 className="h-3.5 w-3.5 animate-spin text-emerald-300" />
          <span className="font-mono-num">
            {stage === "installing" ? "正在重启安装..." : `下载更新 ${progressPct}%`}
          </span>
        </>
      ) : (
        <>
          {hasNativeUpdater ? (
            <Download className="h-3.5 w-3.5 text-emerald-300" />
          ) : (
            <ArrowUpCircle className="h-3.5 w-3.5 text-emerald-300" />
          )}
          <span>
            v{releaseInfo?.latestVersion ?? "新版本"} 可用 · {hasNativeUpdater ? "一键更新" : "查看 Release"}
          </span>
        </>
      )}
    </button>
  );
};
