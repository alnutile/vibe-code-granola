import { useEffect, useRef } from "react";
import { formatMs } from "../lib/events";
import type { Segment } from "../lib/types";

interface Props {
  segments: Segment[];
  isRecording: boolean;
}

const SPEAKER: Record<Segment["source"], string> = {
  mic: "You",
  system: "Them",
  mixed: "Room",
};

export default function TranscriptPane({ segments, isRecording }: Props) {
  const endRef = useRef<HTMLDivElement>(null);

  // Follow the conversation while recording, but stay put afterwards so reading
  // back through a finished transcript isn't yanked around.
  useEffect(() => {
    if (isRecording) endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [segments.length, isRecording]);

  if (segments.length === 0) {
    return (
      <div className="pane pane-empty">
        <p>No transcript yet.</p>
        <p className="muted small">
          {isRecording
            ? "Listening. Lines appear once there's enough speech to transcribe — usually every 15–30 seconds."
            : "Press Record to start capturing this meeting."}
        </p>
      </div>
    );
  }

  return (
    <div className="pane transcript">
      {segments.map((s) => (
        <div key={s.id} className={`line line-${s.source}`}>
          <span className="line-time">{formatMs(s.startMs)}</span>
          <span className="line-speaker">{SPEAKER[s.source]}</span>
          <span className="line-text">{s.text}</span>
        </div>
      ))}
      <div ref={endRef} />
    </div>
  );
}
