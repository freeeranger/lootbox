import { api } from "./api";
import type { AudioStatus } from "./types";

interface AudioPlaybackSnapshot {
  path: string | null;
  playing: boolean;
}

let status: AudioStatus = {
  path: null,
  playing: false,
  positionSeconds: 0,
  durationSeconds: 0,
};
let playback: AudioPlaybackSnapshot = { path: null, playing: false };
let poll: number | null = null;
const statusListeners = new Set<() => void>();
const playbackListeners = new Set<() => void>();

function stopPolling() {
  if (poll !== null) window.clearInterval(poll);
  poll = null;
}

function applyStatus(next: AudioStatus) {
  const playbackChanged = playback.path !== next.path || playback.playing !== next.playing;
  status = next;
  statusListeners.forEach((listener) => listener());
  if (playbackChanged) {
    playback = { path: next.path, playing: next.playing };
    playbackListeners.forEach((listener) => listener());
  }
  if (!next.playing) stopPolling();
}

function startPolling() {
  if (poll !== null) return;
  poll = window.setInterval(() => {
    void api.audioStatus().then(applyStatus).catch(stopPolling);
  }, 150);
}

export function subscribeAudioStatus(listener: () => void) {
  statusListeners.add(listener);
  return () => statusListeners.delete(listener);
}

export function getAudioStatusSnapshot() {
  return status;
}

export function subscribeAudioPlayback(listener: () => void) {
  playbackListeners.add(listener);
  return () => playbackListeners.delete(listener);
}

export function getAudioPlaybackSnapshot() {
  return playback;
}

export async function syncAudioStatus() {
  applyStatus(await api.audioStatus());
}

export async function toggleAudioPlayback(path: string) {
  const next = await api.toggleAudio(path);
  applyStatus(next);
  if (next.playing) startPolling();
  return next;
}

export async function seekAudioPlayback(path: string, positionSeconds: number) {
  const next = await api.seekAudio(path, positionSeconds);
  applyStatus(next);
  if (next.playing) startPolling();
  return next;
}

export async function stopAudioPlayback(path: string) {
  await api.stopAudio(path);
  if (status.path === path) {
    applyStatus({
      path: null,
      playing: false,
      positionSeconds: 0,
      durationSeconds: 0,
    });
  }
}
