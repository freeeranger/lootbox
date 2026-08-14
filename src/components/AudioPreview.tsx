import { useEffect, useRef, useState, useSyncExternalStore } from "react";
import { LoaderCircle, Pause, Play } from "lucide-react";
import { api } from "../api";
import type { AudioAnalysis } from "../types";
import { Button } from "@/components/ui/button";
import {
  getAudioStatusSnapshot,
  seekAudioPlayback,
  stopAudioPlayback,
  subscribeAudioStatus,
  syncAudioStatus,
  toggleAudioPlayback,
} from "../audioPlayback";

interface Props {
  path: string;
}

const analyses = new Map<string, Promise<AudioAnalysis>>();

function getAnalysis(path: string) {
  let analysis = analyses.get(path);
  if (!analysis) {
    analysis = api.audioAnalysis(path);
    analyses.set(path, analysis);
    void analysis.catch(() => analyses.delete(path));
  }
  return analysis;
}

function formatTime(seconds: number) {
  if (!Number.isFinite(seconds) || seconds <= 0) return "0:00";
  const rounded = Math.floor(seconds);
  const minutes = Math.floor(rounded / 60);
  const remainder = rounded % 60;
  return `${minutes}:${remainder.toString().padStart(2, "0")}`;
}

export function AudioPreview({ path }: Props) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [peaks, setPeaks] = useState<number[]>([]);
  const [duration, setDuration] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const audioStatus = useSyncExternalStore(
    subscribeAudioStatus,
    getAudioStatusSnapshot,
  );
  const isCurrent = audioStatus.path === path;
  const playing = isCurrent && audioStatus.playing;
  const position = isCurrent ? audioStatus.positionSeconds : 0;

  useEffect(() => {
    let active = true;
    setPeaks([]);
    setDuration(0);
    setError(null);
    void syncAudioStatus();
    void api
      .audioDuration(path)
      .then((seconds) => {
        if (active) setDuration(seconds);
      })
      .catch((caught) => {
        if (active) setError(String(caught));
      });
    void getAnalysis(path)
      .then((analysis) => {
        if (!active) return;
        setDuration(analysis.durationSeconds);
        setPeaks(analysis.peaks);
      })
      .catch(() => {
        // Duration and playback remain available if waveform analysis fails.
      });

    return () => {
      active = false;
      void stopAudioPlayback(path);
    };
  }, [path]);

  useEffect(() => {
    if (isCurrent && audioStatus.durationSeconds > 0) {
      setDuration(audioStatus.durationSeconds);
    }
  }, [audioStatus.durationSeconds, isCurrent]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const draw = () => {
      const bounds = canvas.getBoundingClientRect();
      const ratio = Math.min(window.devicePixelRatio, 2);
      canvas.width = Math.max(1, Math.floor(bounds.width * ratio));
      canvas.height = Math.max(1, Math.floor(bounds.height * ratio));
      const context = canvas.getContext("2d");
      if (!context) return;
      context.scale(ratio, ratio);
      context.clearRect(0, 0, bounds.width, bounds.height);
      const middle = bounds.height / 2;
      const progress = duration > 0 ? position / duration : 0;
      if (peaks.length === 0) {
        context.fillStyle = "#30343b";
        context.fillRect(0, middle, bounds.width, 1);
        return;
      }
      const barWidth = bounds.width / peaks.length;
      peaks.forEach((peak, index) => {
        const height = Math.max(1, peak * (bounds.height - 8));
        context.fillStyle = index / peaks.length <= progress ? "#c99a45" : "#555b64";
        context.fillRect(index * barWidth, middle - height / 2, Math.max(1, barWidth - 1), height);
      });
    };
    const observer = new ResizeObserver(draw);
    observer.observe(canvas);
    draw();
    return () => observer.disconnect();
  }, [duration, peaks, position]);

  async function toggle() {
    setBusy(true);
    setError(null);
    try {
      await toggleAudioPlayback(path);
    } catch (caught) {
      setError(String(caught));
    } finally {
      setBusy(false);
    }
  }

  async function seek(event: React.MouseEvent<HTMLCanvasElement>) {
    if (duration <= 0) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const nextPosition =
      Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width)) * duration;
    setError(null);
    try {
      await seekAudioPlayback(path, nextPosition);
    } catch (caught) {
      setError(String(caught));
    }
  }

  return (
    <div
      className="mx-3 flex h-[150px] flex-col justify-center gap-3 rounded-md border bg-muted/10 px-4 outline-none focus-visible:ring-1 focus-visible:ring-ring"
      tabIndex={0}
      onKeyDown={(event) => {
        if (event.key !== " ") return;
        event.preventDefault();
        void toggle();
      }}
    >
      <canvas
        ref={canvasRef}
        aria-label="Seek audio"
        className="h-14 w-full cursor-pointer"
        onClick={(event) => void seek(event)}
      />
      <div className="flex items-center gap-2">
        <Button
          type="button"
          variant="secondary"
          size="icon-sm"
          className="rounded-full"
          onClick={() => void toggle()}
          disabled={busy}
          aria-label={playing ? "Pause" : "Play"}
        >
          {busy ? <LoaderCircle className="animate-spin" /> : playing ? <Pause /> : <Play />}
        </Button>
        <span className="font-mono text-[11px] tabular-nums text-muted-foreground">
          {formatTime(position)} / {formatTime(duration)}
        </span>
        <span className="ml-auto font-mono text-[11px] uppercase text-muted-foreground">
          {path.split(".").pop()}
        </span>
      </div>
      {error && <p className="truncate text-[11px] text-destructive" title={error}>{error}</p>}
    </div>
  );
}
