// In-memory cache of integration fetches, keyed by brief path + selector.
// Fetches are lazy on brief open and on manual refresh, and the last result is
// kept so switching briefs back doesn't refetch. Fetched data is never
// persisted.
//
// Opt-in auto-sync (settings.autoSyncFeeds): the scheduler in
// autosync.svelte.ts calls `poll` in the background, and every successful
// fetch is compared with the feed's "seen" fingerprints to mark items "new"
// (fresh). Only hashes are held or stored (localStorage `waid-feed-seen`); with
// the setting off none of this bookkeeping runs and the key doesn't exist.
//
// Every fetch — background poll, panel open, manual refresh — is debited to
// its budget domain in a small spend ledger the scheduler drains each tick, so
// foreground use and auto-sync share one rate-limit budget per credential.
// Credential ids are session-salted hashes from the backend, kept in memory
// only (never in localStorage).

import type { BriefIntegration, IntegrationFetch, IntegrationItem } from "$lib/types";
import { fetchIntegration, digestIntegrations, IntegrationFetchError } from "$lib/tauri";
import { settings } from "$lib/stores/settings.svelte";
import { projects } from "$lib/stores/projects.svelte";
import {
  budgetClass,
  domainKey,
  fingerprint as fingerprintOf,
  hash,
  neverFresh,
  PANEL_MAX_AGE_MS,
  SEEN_CAP,
} from "$lib/feedSync";

interface Entry {
  loading: boolean;
  error: string | null;
  /** Last successful fetch; kept across reloads so the panel stays populated. */
  data: IntegrationFetch | null;
}

/** An LLM digest of a brief's items — display-only, kept in memory per brief. */
interface DigestEntry {
  loading: boolean;
  error: string | null;
  text: string | null;
}

/** A feed's identity, as the scheduler and `pruneSeen` pass it. */
export interface FeedRef {
  path: string;
  connection: string;
  kind: string;
  query?: string | null;
}

export type PollResult =
  | { ok: true; data: IntegrationFetch }
  | { ok: false; error: string; /** The rejection, for rateLimitHoldMs. */ cause: unknown };

/** Spend entries older than this are dropped if nobody drains them. */
const SPEND_TTL_MS = 120_000;

const SEEN_KEY = "waid-feed-seen";

class IntegrationStore {
  // Keyed so a brief can point at several connections/kinds independently.
  private cache = $state<Record<string, Entry>>({});
  // LLM digests, keyed by brief path.
  private digests = $state<Record<string, DigestEntry>>({});

  // --- "New" tracking (auto-sync only) ---
  /** feed key hash → seen fingerprint hashes (insertion-ordered). Not reactive. */
  private seen: Map<string, Set<string>> | null = null;
  /** feed key → fingerprints currently fresh. Reactive; never persisted. */
  private fresh = $state<Record<string, string[]>>({});
  /** feed key → the brief path + kind it belongs to (for counts and baselines). */
  private meta = new Map<string, { path: string; kind: string }>();
  private inFlight = new Set<string>();

  // --- Budget bookkeeping (not reactive; read by the scheduler) ---
  /** domain → requests made since the scheduler last drained it. */
  private spend = new Map<string, { at: number; cost: number }[]>();
  /** "path\nconnection" → the credential id its last fetch reported. */
  private credentialIds = new Map<string, string>();

  /** The credential id last seen for a brief's connection, or null before its
   *  first fetch (the scheduler then uses the provider-wide domain). */
  credentialIdFor(path: string, connection: string): string | null {
    return this.credentialIds.get(`${path}\n${connection}`) ?? null;
  }

  /** Drain the ledger: requests per domain since the last call, skipping
   *  entries past SPEND_TTL_MS (e.g. left over from before auto-sync was
   *  turned off, which would otherwise debit a fresh bucket). */
  takeSpend(): Map<string, number> {
    const now = Date.now();
    const out = new Map<string, number>();
    for (const [domain, entries] of this.spend) {
      const live = entries.filter((e) => now - e.at < SPEND_TTL_MS);
      if (live.length) out.set(domain, live.reduce((n, e) => n + e.cost, 0));
    }
    this.spend.clear();
    return out;
  }

  /** Debit a fetch to its domain. Only kept while auto-sync is on. */
  private recordSpend(domain: string | null, cost: number): void {
    if (!domain || !cost || !settings.autoSyncFeeds) return;
    const now = Date.now();
    const entries = (this.spend.get(domain) ?? []).filter((e) => now - e.at < SPEND_TTL_MS);
    entries.push({ at: now, cost });
    this.spend.set(domain, entries);
  }

  /** The budget domain a brief's feed belongs to, or null if not polled. */
  private domainFor(path: string, connection: string, kind: string): string | null {
    const provider = projects.briefs
      .find((b) => b.path === path)
      ?.connections.find((c) => c.id === connection)?.provider;
    if (!provider || provider === "neuroskill") return null;
    return domainKey(budgetClass(provider, kind), this.credentialIdFor(path, connection));
  }

  /** Shared by `fetch` and `poll`: learn the credential id, then debit.
   *  `failure` is the rejection when the fetch failed. */
  private account(
    path: string,
    connection: string,
    kind: string,
    data: IntegrationFetch | null,
    failure?: unknown,
  ): void {
    if (data?.credentialId) this.credentialIds.set(`${path}\n${connection}`, data.credentialId);
    // A failed fetch still spent at least one request — more if it got partway
    // through a multi-request fetch (Gmail's per-message reads).
    const cost = data ? (data.cost ?? 1) : failure instanceof IntegrationFetchError ? (failure.cost ?? 1) : 1;
    this.recordSpend(this.domainFor(path, connection, kind), cost);
  }

  private keyFor(path: string, connection: string, kind: string, query?: string | null): string {
    return `${path}\0${connection}\0${kind}\0${query ?? ""}`;
  }

  /** Current cache entry for a selector, or null if never fetched. Reading this
   *  in a template subscribes to updates as the fetch resolves. Keyed by query
   *  too, so sibling feeds (e.g. two Notion databases) cache independently. */
  get(path: string, connection: string, kind: string, query?: string | null): Entry | null {
    return this.cache[this.keyFor(path, connection, kind, query)] ?? null;
  }

  /** Fetch a selector's items. Returns the cached result without a network call
   *  unless `force` is set (manual refresh). A first load accepts the backend's
   *  cached copy up to PANEL_MAX_AGE_MS old (another brief on the same
   *  credential may have just fetched it); a forced refresh always asks the
   *  provider. On error the previous data is kept so the panel degrades to
   *  "stale + error" rather than going blank. */
  async fetch(
    path: string,
    connection: string,
    kind: string,
    query?: string | null,
    limit?: number | null,
    force = false,
  ): Promise<IntegrationFetch | null> {
    const key = this.keyFor(path, connection, kind, query);
    const existing = this.cache[key];
    if (existing?.data && !force) return existing.data;

    this.meta.set(key, { path, kind });
    this.inFlight.add(key);
    this.cache = {
      ...this.cache,
      [key]: { loading: true, error: null, data: existing?.data ?? null },
    };
    try {
      const data = await fetchIntegration(path, connection, kind, query, limit, force ? 0 : PANEL_MAX_AGE_MS);
      this.account(path, connection, kind, data);
      this.cache = { ...this.cache, [key]: { loading: false, error: null, data } };
      this.applyResult(key, kind, data);
      return data;
    } catch (e) {
      this.account(path, connection, kind, null, e);
      this.cache = {
        ...this.cache,
        [key]: { loading: false, error: String(e), data: existing?.data ?? null },
      };
      throw e;
    } finally {
      this.inFlight.delete(key);
    }
  }

  /** Background fetch: never flips `loading`, never throws, never toasts.
   *  Skips (returns null) if a fetch for the same key is already in flight.
   *  Records the error on the entry with the previous data kept, so the
   *  panel's "Couldn't refresh: …" line shows. `maxAgeMs` lets the backend
   *  answer from its feed cache (dedup across briefs sharing a credential). */
  async poll(
    path: string,
    connection: string,
    kind: string,
    query?: string | null,
    limit?: number | null,
    maxAgeMs = 0,
  ): Promise<PollResult | null> {
    const key = this.keyFor(path, connection, kind, query);
    if (this.inFlight.has(key) || this.cache[key]?.loading) return null;
    this.meta.set(key, { path, kind });
    this.inFlight.add(key);
    try {
      const data = await fetchIntegration(path, connection, kind, query, limit, maxAgeMs);
      this.account(path, connection, kind, data);
      this.cache = { ...this.cache, [key]: { loading: this.cache[key]?.loading ?? false, error: null, data } };
      this.applyResult(key, kind, data);
      return { ok: true, data };
    } catch (e) {
      this.account(path, connection, kind, null, e);
      const prev = this.cache[key];
      this.cache = {
        ...this.cache,
        [key]: { loading: prev?.loading ?? false, error: String(e), data: prev?.data ?? null },
      };
      return { ok: false, error: String(e), cause: e };
    } finally {
      this.inFlight.delete(key);
    }
  }

  /** Shared by `fetch` and `poll`: first result for a feed is the silent
   *  baseline; afterwards, fresh = current fingerprints not yet seen. `seen`
   *  only grows on baseline and take, so an item stays fresh until taken. */
  private applyResult(key: string, kind: string, data: IntegrationFetch): void {
    if (!settings.autoSyncFeeds) return;
    const seen = this.loadSeen();
    const fk = hash(key);
    const prints = data.items.map((it) => fingerprintOf(kind, it));
    const set = seen.get(fk);
    if (!set) {
      seen.set(fk, capped(new Set(prints)));
      this.saveSeen();
      this.setFresh(key, []);
      return;
    }
    const freshPrints = data.items.flatMap((it, i) => (!neverFresh(it) && !set.has(prints[i]) ? [prints[i]] : []));
    this.setFresh(key, [...new Set(freshPrints)]);
  }

  private setFresh(key: string, prints: string[]): void {
    const cur = this.fresh[key];
    if (!prints.length && !cur?.length) return;
    if (cur && cur.length === prints.length && cur.every((p, i) => p === prints[i])) return;
    if (prints.length) this.fresh = { ...this.fresh, [key]: prints };
    else {
      const { [key]: _drop, ...rest } = this.fresh;
      this.fresh = rest;
    }
  }

  /** New items across a brief's feeds. 0 when the setting is off. */
  freshCount(path: string): number {
    if (!settings.autoSyncFeeds) return 0;
    let n = 0;
    for (const [key, prints] of Object.entries(this.fresh)) {
      if (this.meta.get(key)?.path === path) n += prints.length;
    }
    return n;
  }

  /** New items in one feed. 0 when the setting is off. */
  freshCountFor(path: string, connection: string, kind: string, query?: string | null): number {
    if (!settings.autoSyncFeeds) return 0;
    return this.fresh[this.keyFor(path, connection, kind, query)]?.length ?? 0;
  }

  /** New items across every brief. 0 when the setting is off. */
  get totalFresh(): number {
    if (!settings.autoSyncFeeds) return 0;
    return Object.values(this.fresh).reduce((n, p) => n + p.length, 0);
  }

  /** Mark every feed of this brief as seen and fold the fingerprints that were
   *  fresh into `prev`, per feed, so the caller can highlight those rows. Kept
   *  per feed so an item shared by two overlapping selectors is only new in the
   *  one it was fresh in. Returns `prev` itself when nothing was fresh. */
  takeFresh(path: string, prev: Highlights): Highlights {
    const keys = Object.keys(this.fresh).filter((k) => this.meta.get(k)?.path === path);
    if (!keys.length) return prev;
    const next = new Map(prev);
    const seen = this.loadSeen();
    const rest = { ...this.fresh };
    for (const key of keys) {
      const fk = hash(key);
      const set = seen.get(fk) ?? new Set<string>();
      const lit = new Set(next.get(key));
      for (const p of rest[key]) {
        set.add(p);
        lit.add(p);
      }
      seen.set(fk, capped(set));
      next.set(key, lit);
      delete rest[key];
    }
    this.fresh = rest;
    this.saveSeen();
    return next;
  }

  /** Whether `item` was taken as fresh in this selector's own feed. False when
   *  the setting is off, so a mounted panel drops its highlights with it. */
  isHighlighted(h: Highlights, path: string, ig: BriefIntegration, item: IntegrationItem): boolean {
    if (!settings.autoSyncFeeds || !h.size) return false;
    return h.get(this.keyFor(path, ig.connection, ig.kind, ig.query))?.has(fingerprintOf(ig.kind, item)) ?? false;
  }

  /** Wipe seen + fresh + the localStorage key. If the setting is on afterwards,
   *  baseline every cache entry that has data. */
  resetSeen(): void {
    this.seen = new Map();
    this.fresh = {};
    try {
      localStorage.removeItem(SEEN_KEY);
    } catch {
      // Storage unavailable — in-memory only.
    }
    if (!settings.autoSyncFeeds) return;
    for (const [key, entry] of Object.entries(this.cache)) {
      const m = this.meta.get(key);
      if (entry.data && m) this.seen.set(hash(key), capped(new Set(entry.data.items.map((it) => fingerprintOf(m.kind, it)))));
    }
    if (this.seen.size) this.saveSeen();
  }

  /** Drop seen/fresh for feeds that no longer exist on any brief. */
  pruneSeen(live: FeedRef[]): void {
    if (!settings.autoSyncFeeds) return;
    const liveKeys = new Set(live.map((f) => this.keyFor(f.path, f.connection, f.kind, f.query)));
    const liveHashes = new Set([...liveKeys].map(hash));
    const seen = this.loadSeen();
    let changed = false;
    for (const fk of [...seen.keys()]) {
      if (!liveHashes.has(fk)) {
        seen.delete(fk);
        changed = true;
      }
    }
    if (changed) this.saveSeen();
    const stale = Object.keys(this.fresh).filter((k) => !liveKeys.has(k));
    if (stale.length) {
      const rest = { ...this.fresh };
      for (const k of stale) delete rest[k];
      this.fresh = rest;
    }
  }

  private loadSeen(): Map<string, Set<string>> {
    if (this.seen) return this.seen;
    this.seen = new Map();
    try {
      const raw = JSON.parse(localStorage.getItem(SEEN_KEY) ?? "null");
      if (raw?.v === 1 && raw.feeds && typeof raw.feeds === "object") {
        for (const [fk, prints] of Object.entries(raw.feeds)) {
          if (Array.isArray(prints)) this.seen.set(fk, capped(new Set(prints.map(String))));
        }
      }
    } catch {
      // Corrupt or unavailable — start empty, in memory.
    }
    return this.seen;
  }

  private saveSeen(): void {
    if (!this.seen || !settings.autoSyncFeeds) return;
    const feeds: Record<string, string[]> = {};
    for (const [fk, set] of this.seen) feeds[fk] = [...set];
    try {
      localStorage.setItem(SEEN_KEY, JSON.stringify({ v: 1, feeds }));
    } catch {
      // Storage unavailable — in-memory only.
    }
  }

  /** Current digest entry for a brief, or null if never generated. */
  digestFor(path: string): DigestEntry | null {
    return this.digests[path] ?? null;
  }

  /** (Re)generate the LLM digest for a brief. Keeps prior text on error. */
  async runDigest(path: string): Promise<string> {
    const prev = this.digests[path];
    this.digests = { ...this.digests, [path]: { loading: true, error: null, text: prev?.text ?? null } };
    try {
      const text = await digestIntegrations(path);
      this.digests = { ...this.digests, [path]: { loading: false, error: null, text } };
      return text;
    } catch (e) {
      this.digests = {
        ...this.digests,
        [path]: { loading: false, error: String(e), text: prev?.text ?? null },
      };
      throw e;
    }
  }

  /** Clear a brief's digest (e.g. after snapshotting it). */
  clearDigest(path: string): void {
    const { [path]: _drop, ...rest } = this.digests;
    this.digests = rest;
  }
}

/** Fingerprints taken by `takeFresh`, per feed key, for row highlighting. */
export type Highlights = ReadonlyMap<string, ReadonlySet<string>>;

/** Keep the newest SEEN_CAP fingerprints (Sets iterate in insertion order). */
function capped(set: Set<string>): Set<string> {
  if (set.size <= SEEN_CAP) return set;
  return new Set([...set].slice(set.size - SEEN_CAP));
}

export const integrations = new IntegrationStore();
