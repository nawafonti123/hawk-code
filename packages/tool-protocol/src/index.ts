import { z } from "zod";

export const protocolEnvelopeSchema = z.object({
  protocolVersion: z.literal(1),
  requestId: z.string().uuid(),
});

export const workspacePayloadSchema = z.object({
  workspacePath: z.string().min(1).max(32_768),
});

export type WorkspacePayload = z.infer<typeof workspacePayloadSchema>;
