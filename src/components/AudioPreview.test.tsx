import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { api } from "../api";
import { AudioPreview } from "./AudioPreview";

vi.mock("../api", () => ({
  api: {
    audioDuration: vi.fn().mockResolvedValue(100),
    audioAnalysis: vi.fn().mockResolvedValue({ durationSeconds: 100, peaks: [0.2, 0.8] }),
    audioStatus: vi.fn().mockResolvedValue({ path: null, playing: false, positionSeconds: 0, durationSeconds: 0 }),
    seekAudio: vi.fn().mockImplementation((path: string, positionSeconds: number) => Promise.resolve({
      path,
      playing: false,
      positionSeconds,
      durationSeconds: 100,
    })),
    toggleAudio: vi.fn().mockResolvedValue({ path: "/tmp/tone.wav", playing: true, positionSeconds: 0, durationSeconds: 100 }),
    stopAudio: vi.fn().mockResolvedValue(undefined),
  },
}));

describe("AudioPreview", () => {
  it("exposes the waveform as a keyboard-operable slider", async () => {
    vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue(null);
    render(<AudioPreview path="/tmp/tone.wav" />);
    const slider = await screen.findByRole("slider", { name: "Audio position" });
    await waitFor(() => expect(slider).toHaveAttribute("aria-valuemax", "100"));

    fireEvent.keyDown(slider, { key: "ArrowRight" });
    await waitFor(() => expect(api.seekAudio).toHaveBeenCalledWith("/tmp/tone.wav", 1));

    fireEvent.keyDown(slider, { key: "End" });
    await waitFor(() => expect(api.seekAudio).toHaveBeenCalledWith("/tmp/tone.wav", 100));
  });
});
