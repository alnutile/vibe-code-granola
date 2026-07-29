import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import * as api from "../lib/api";
import { subscribe } from "../lib/events";
import type { ChatMessage, Suggestion } from "../lib/types";
import { CloseIcon, SendIcon } from "./Icons";

interface Props {
  meetingId: string;
  suggestions: Suggestion[];
  isRecording: boolean;
  hasTranscript: boolean;
  onClose: () => void;
  onError: (msg: string) => void;
}

const STARTERS = ["What was decided?", "What do I owe anyone?", "Anything left open?"];

const GREETING =
  "I've read this note and its transcript. Ask me anything, or pick a starter below.";

/**
 * The right-hand aside: live suggestions on top, chat underneath.
 *
 * The design has no home for in-meeting suggestions, but they belong with the
 * assistant rather than in the document — so they sit above the chat log, where
 * they're visible while recording and scroll away afterwards.
 */
export default function ChatAside(props: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void api
      .chatHistory(props.meetingId)
      .then(setMessages)
      .catch(() => {});
  }, [props.meetingId]);

  // Tokens arrive as events; accumulate into a provisional bubble, then swap it
  // for the persisted message when the stream ends.
  useEffect(
    () =>
      subscribe({
        onChatDelta: (e) => {
          if (e.meetingId === props.meetingId) setStreaming((p) => (p ?? "") + e.delta);
        },
        onChatDone: (e) => {
          if (e.meetingId !== props.meetingId) return;
          setStreaming(null);
          void api.chatHistory(props.meetingId).then(setMessages).catch(() => {});
        },
      }),
    [props.meetingId],
  );

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages.length, streaming, props.suggestions.length]);

  const send = async (text?: string) => {
    const q = (text ?? input).trim();
    if (!q || sending) return;

    setMessages((p) => [
      ...p,
      {
        id: `pending-${Date.now()}`,
        meetingId: props.meetingId,
        role: "user",
        content: q,
        createdAt: new Date().toISOString(),
      },
    ]);
    setInput("");
    setSending(true);
    setStreaming("");

    try {
      await api.sendChat(props.meetingId, q);
    } catch (e) {
      props.onError(api.errorMessage(e));
      setStreaming(null);
    } finally {
      setSending(false);
    }
  };

  // Newest first: during a meeting the latest nudge is the only one still actionable.
  const suggestions = props.suggestions.slice().reverse();

  return (
    <aside className="chat-aside">
      <div className="chat-head">
        <h3>Ask this note</h3>
        <button className="btn btn-ghost btn-icon" onClick={props.onClose} aria-label="Close chat">
          <CloseIcon />
        </button>
      </div>

      {suggestions.length > 0 && (
        <div className="suggestions">
          {suggestions.map((s) => (
            <div className="suggestion" key={s.id}>
              <span className="dot" />
              <span>{s.content}</span>
            </div>
          ))}
        </div>
      )}

      <div className="chat-log">
        <div className="bubble bubble-bot">
          {props.hasTranscript
            ? GREETING
            : "Nothing has been transcribed yet — record the meeting and I'll have something to work from."}
        </div>

        {messages.map((m) => (
          <div key={m.id} className={`bubble bubble-${m.role === "user" ? "me" : "bot"}`}>
            {m.role === "assistant" ? (
              <div className="markdown">
                <Markdown>{m.content}</Markdown>
              </div>
            ) : (
              m.content
            )}
          </div>
        ))}

        {streaming !== null && (
          <div className="bubble bubble-bot">
            {streaming ? (
              <div className="markdown">
                <Markdown>{streaming}</Markdown>
              </div>
            ) : (
              <span className="muted small">Thinking…</span>
            )}
          </div>
        )}
        <div ref={endRef} />
      </div>

      {messages.length === 0 && props.hasTranscript && (
        <div className="chat-starters">
          {STARTERS.map((s) => (
            <button className="tag tag-outline" key={s} onClick={() => void send(s)}>
              {s}
            </button>
          ))}
        </div>
      )}

      <div className="chat-compose">
        <input
          className="input"
          placeholder="Ask across this note…"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.shiftKey) {
              e.preventDefault();
              void send();
            }
          }}
        />
        <button
          className="btn btn-primary btn-icon"
          onClick={() => void send()}
          disabled={sending || !input.trim()}
          aria-label="Send"
        >
          <SendIcon />
        </button>
      </div>
    </aside>
  );
}
