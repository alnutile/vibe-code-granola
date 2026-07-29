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
      <div className="doc-empty">
        <span className="tag tag-neutral">No transcript yet</span>
        <p>
          {isRecording
            ? "Listening. Lines appear once there's enough speech to transcribe — usually every 15 to 30 seconds."
            : "Start recording and the transcript builds here as you talk."}
        </p>
      </div>
    );
  }

  return (
    <div className="doc-section">
      {segments.map((s) => (
        <div className={`t-line ${s.source === "system" ? "is-them" : ""}`} key={s.id}>
          <span className="t-time">{formatMs(s.startMs)}</span>
          <div className="t-body">
            <span className="t-who">{SPEAKER[s.source]}</span>
            <span className="t-text">{s.text}</span>
          </div>
        </div>
      ))}
      <div ref={endRef} />
    </div>
  );
}
