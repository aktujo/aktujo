// SPDX-License-Identifier: AGPL-3.0-or-later
// The client of the aktujo API, typed from api/openapi.json (`pnpm api` writes api-types.ts).
import createClient from "openapi-fetch";
import type { paths } from "./api-types";

export const api = createClient<paths>({ baseUrl: "/" });
