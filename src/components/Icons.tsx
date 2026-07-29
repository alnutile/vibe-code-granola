// Icons lifted from designs/Amble.html — same paths, same 2.75 stroke weight.
// Inline rather than an icon package: there are six of them, and a dependency
// would drift from the design the first time it updated.

type Props = { size?: number };

const stroke = {
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2.75,
  strokeLinecap: "round" as const,
  strokeLinejoin: "round" as const,
};

export const PlusIcon = ({ size = 16 }: Props) => (
  <svg width={size} height={size} viewBox="0 0 24 24" {...stroke} aria-hidden>
    <path d="M12 5v14M5 12h14" />
  </svg>
);

export const ChatIcon = ({ size = 16 }: Props) => (
  <svg width={size} height={size} viewBox="0 0 24 24" {...stroke} aria-hidden>
    <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
  </svg>
);

export const MicIcon = ({ size = 16 }: Props) => (
  <svg width={size} height={size} viewBox="0 0 24 24" {...stroke} aria-hidden>
    <path d="M12 2a3 3 0 0 1 3 3v6a3 3 0 0 1-6 0V5a3 3 0 0 1 3-3z" />
    <path d="M19 10v1a7 7 0 0 1-14 0v-1M12 18v4" />
  </svg>
);

export const StopIcon = ({ size = 14 }: Props) => (
  <svg width={size} height={size} viewBox="0 0 24 24" fill="currentColor" aria-hidden>
    <rect x="6" y="6" width="12" height="12" rx="3" />
  </svg>
);

export const CloseIcon = ({ size = 18 }: Props) => (
  <svg width={size} height={size} viewBox="0 0 24 24" {...stroke} aria-hidden>
    <path d="M18 6 6 18M6 6l12 12" />
  </svg>
);

export const SendIcon = ({ size = 17 }: Props) => (
  <svg width={size} height={size} viewBox="0 0 24 24" {...stroke} aria-hidden>
    <path d="M22 2 11 13M22 2l-7 20-4-9-9-4z" />
  </svg>
);

export const TrashIcon = ({ size = 15 }: Props) => (
  <svg width={size} height={size} viewBox="0 0 24 24" {...stroke} aria-hidden>
    <path d="M3 6h18M8 6V4h8v2M19 6l-1 14H6L5 6" />
  </svg>
);
