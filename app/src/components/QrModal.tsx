import React, { useEffect, useState } from "react";
import QRCode from "qrcode";
import { Check, Copy, Smartphone, X } from "lucide-react";

interface QrModalProps {
  title: string;
  url: string | null;
  onClose: () => void;
}

export const QrModal: React.FC<QrModalProps> = ({ title, url, onClose }) => {
  const [dataUrl, setDataUrl] = useState<string>("");
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    if (!url) return;
    QRCode.toDataURL(url, {
      width: 220,
      margin: 2,
      color: { dark: "#0b0d14", light: "#ffffff" },
    }).then(setDataUrl);
  }, [url]);

  if (!url) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/70 p-4 backdrop-blur-md">
      <div className="glass-card w-full max-w-xs rounded-2xl border border-white/12 p-5 text-center shadow-2xl">
        <div className="mb-3 flex items-center justify-between">
          <div className="flex items-center gap-1.5 text-[13px] font-semibold text-zinc-100">
            <Smartphone className="h-4 w-4 text-emerald-400" />
            {title}
          </div>
          <button onClick={onClose} className="rounded-lg p-1 text-zinc-400 hover:bg-white/8 hover:text-zinc-100">
            <X className="h-4 w-4" />
          </button>
        </div>

        <div className="mx-auto flex h-[228px] w-[228px] items-center justify-center rounded-xl bg-white p-2 shadow-inner">
          {dataUrl && <img src={dataUrl} alt="Subscription QR" className="h-full w-full rounded-lg" />}
        </div>

        <button
          onClick={async () => {
            await navigator.clipboard.writeText(url);
            setCopied(true);
            setTimeout(() => setCopied(false), 1800);
          }}
          className="mt-3 inline-flex w-full items-center justify-center gap-1.5 rounded-xl border border-white/10 bg-white/6 py-2 text-[12px] font-medium text-zinc-100 transition hover:bg-white/12"
        >
          {copied ? <Check className="h-3.5 w-3.5 text-emerald-400" /> : <Copy className="h-3.5 w-3.5" />}
          {copied ? "订阅链接已复制" : "复制订阅链接"}
        </button>
      </div>
    </div>
  );
};
