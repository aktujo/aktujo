// SPDX-License-Identifier: AGPL-3.0-or-later
// The client of the aktujo API, typed from api/openapi.json (`pnpm api` writes api-types.ts).
import createClient from "openapi-fetch";
import type { components, paths } from "./api-types";

export const api = createClient<paths>({ baseUrl: "/" });

export type Schemas = components["schemas"];
export type Space = Schemas["SpaceView"];
export type Summary = Schemas["DocumentSummary"];
export type DocumentView = Schemas["DocumentView"];
export type Kind = Schemas["KindView"];
export type Value = Schemas["ValueView"];

/** Sends a file's bytes as they are; the API takes no forms. */
export async function upload(path: string, file: File): Promise<{ ok: boolean; body: unknown }> {
  const response = await fetch(`${path}?name=${encodeURIComponent(file.name)}`, {
    method: "POST",
    headers: { "Content-Type": file.type || "application/octet-stream" },
    body: file,
  });
  return { ok: response.ok, body: await response.json().catch(() => null) };
}

/** The address of a version's file. */
export function fileUrl(space: string, document: string, number: number, download = false): string {
  const path = `/api/v1/spaces/${encodeURIComponent(space)}/documents/${encodeURIComponent(document)}`;
  return `${path}/versions/${number}/file${download ? "?download=true" : ""}`;
}

/** A value as it is read. */
export function shown(value: Value, yes: string, no: string): string {
  switch (value.type) {
    case "money":
      return `${value.value.amount} ${value.value.currency}`;
    case "flag":
      return value.value ? yes : no;
    default:
      return value.value;
  }
}
