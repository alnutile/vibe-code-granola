import { useEffect, useRef, useState } from "react";
import Markdown from "react-markdown";
import * as api from "../lib/api";
import { subscribe } from "../lib/events";
import type { ChatMessage, Suggestion } from "../lib/types";

interface Props {
  meetingId: string;
  suggestions: Suggestion[];
  isRecording: boolean;
  hasTranscript: boolean;
  onError: (msg: string) => void;
}

export default function AssistantPane(props: Props) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState<string | null>(null);
  const [sending, setSending] = useState(false);
  const [asking, setAsking] = useState(false);
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    void api
      .chatHistory(props.meetingId)
      .then(setMessages)
      .catch(() => {});
  }, [props.meetingId]);

  // Tokens arrive over an event stream; accumulate them into a provisional
  // bubble, then swap it for the persisted message when the stream ends.
  useEffect(
    () =>
      subscribe({
        onChatDelta: (e) => {
          if (e.meetingId !== props.meetingId) return;
          setStreaming((prev) => (prev ?? "") + e.delta);
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
  }, [messages.length, streaming]);

  const send = async () => {
    const text = input.trim();
    if (!text || sending) return;

    // Show the question immediately rather than waiting for the round trip.
    setMessages((prev) => [
      ...prev,
      {
        id: `pending-${Date.now()}`,
        meetingId: props.meetingId,
        role: "user",
        content: text,
        createdAt: new Date().toISOString(),
      },
    ]);
    setInput("");
    setSending(true);
    setStreaming("");

    try {
      await api.sendChat(props.meetingId, text);
    } catch (e) {
      props.onError(api.errorMessage(e));
      setStreaming(null);
    } finally {
      setSending(false);
    }
  };

  const askForSuggestion = async () => {
    setAsking(true);
    try {
      const s = await api.suggestNow(props.meetingId);
      if (s === null) props.onError("Nothing worth suggesting right now.");
    } catch (e) {
      props.onError(api.errorMessage(e));
    } finally {
      setAsking(false);
    }
  };

  return (
    <section className="assistant">
      <div className="assistant-section suggestions">
        <div className="notes-head">
          <h3>Suggestions</h3>
          <button
            className="btn btn-small"
            onClick={askForSuggestion}
            disabled={asking || !props.hasTranscript}
            title={props.hasTranscript ? undefined : "Nothing has been transcribed yet."}
          >
            {asking ? "Thinking…" : "Suggest now"}
          </button>
        </div>

        {props.suggestions.length === 0 ? (
          <p className="muted small">
            {props.isRecording
              ? "Ideas against your meeting prompt will appear here as the conversation goes on."
              : "Set a meeting prompt above, then record — suggestions arrive while you talk."}
          </p>
        ) : (
          <ul className="suggestion-list">
            {props.suggestions
              .slice()
              .reverse()
              .map((s) => (
                <li key={s.id}>{s.content}</li>
              ))}
          </ul>
        )}
      </div>

      <div className="assistant-section chat grow">
        <div className="notes-head">
          <h3>Chat with this meeting</h3>
          {messages.length > 0 && (
            <button
              className="btn btn-small"
              onClick={() => {
                void api.clearChat(props.meetingId).then(() => setMessages([]));
              }}
            >
              Clear
            </button>
          )}
        </div>

        <div className="chat-log">
          {messages.length === 0 && !streaming && (
            <p className="muted small">
              Ask anything about what was said — "what did I commit to?", "summarize the objections",
              "what did they say about pricing?"
            </p>
          )}
          {messages.map((m) => (
            <div key={m.id} className={`bubble bubble-${m.role}`}>
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
            <div className="bubble bubble-assistant">
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

        <div className="chat-input">
          <textarea
            rows={2}
            placeholder="Ask about this meeting…"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              // Enter sends; Shift+Enter for a newline.
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                void send();
              }
            }}
          />
          <button className="btn btn-primary" onClick={send} disabled={sending || !input.trim()}>
            Send
          </button>
        </div>
      </div>
    </section>
  );
}
