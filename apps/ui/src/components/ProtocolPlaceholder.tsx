/**
 * Placeholder pane shown when the active tab's request is a protocol
 * Argos can persist but doesn't yet have an editor for. Each protocol
 * gets a one-liner so the user knows what they're looking at and when
 * editing lands.
 */

import { Construction, MessageCircle, Network } from 'lucide-solid';

import type { ProtocolTag } from '../types/workspace';

const LABELS: Record<Exclude<ProtocolTag, 'rest'>, { title: string; body: string }> = {
  graphql: {
    title: 'GraphQL request',
    body: 'Editor lands in E5 chunk 2. The file is saved on disk and round-trips through git — switch back to REST or import a Postman collection in the meantime.',
  },
  websocket: {
    title: 'WebSocket connection',
    body: 'WebSocket UI lands in E5 chunk 3. Connection params and message templates are saved on disk; sending is not wired up yet.',
  },
};

export default function ProtocolPlaceholder(props: { protocol: ProtocolTag }) {
  if (props.protocol === 'rest') return null;
  const info = LABELS[props.protocol];
  const Icon = props.protocol === 'graphql' ? Network : MessageCircle;
  return (
    <div class="flex h-full w-full items-center justify-center p-8">
      <div class="flex max-w-md flex-col items-center gap-3 rounded-2xl border border-border bg-bg-card px-8 py-6 text-center">
        <Icon size={28} class="text-fg-secondary" />
        <h2 class="text-[15px] font-semibold">{info.title}</h2>
        <p class="text-[12px] leading-relaxed text-fg-secondary">{info.body}</p>
        <div class="mt-1 inline-flex items-center gap-1.5 rounded-full bg-bg-secondary px-3 py-1 text-[10px] font-medium uppercase tracking-wider text-fg-secondary">
          <Construction size={11} />
          editor coming soon
        </div>
      </div>
    </div>
  );
}
