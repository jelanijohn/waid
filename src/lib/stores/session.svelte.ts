// NeuroSkill labeled work sessions, app-wide (lifted from ProjectDetail so the
// widget can show "· in session" and end it too). A brief with a NeuroSkill
// connection can mark a labeled session, so its EEG epochs attribute to that
// project. Start fires at the moment of intent (the first launch/open action,
// or the explicit button); end fires from the explicit button and, as safety
// nets, on navigating away (ProjectDetail's onDestroy) — the backend's
// session cap is the final backstop. At most one session is open at a time.

import type { Brief } from "$lib/types";
import { projects } from "$lib/stores/projects.svelte";
import { toasts } from "$lib/stores/toasts.svelte";

function hasNeuro(brief: Brief): boolean {
  return brief.connections.some((c) => c.provider === "neuroskill");
}

class SessionStore {
  /** Path of the brief with an open labeled session, or null. At most one. */
  activePath = $state<string | null>(null);
  // Warn at most once per "daemon down" stretch so repeatedly launching into
  // work doesn't spam toasts; cleared the moment a label fires successfully.
  private warned = false;

  isActive(path: string): boolean {
    return this.activePath === path;
  }

  /** No-op when the brief has no neuroskill connection or is already active.
   *  If another brief's session is open, end it silently first. */
  async start(brief: Brief): Promise<void> {
    if (!hasNeuro(brief) || this.isActive(brief.path)) return;
    if (this.activePath) this.end({ silent: true });
    this.activePath = brief.path;
    const ok = await projects.markSession(brief.path, "start");
    if (ok) {
      this.warned = false;
    } else {
      // The label never reached NeuroSkill — don't leave a false "recording"
      // state, and tell the user so tracking isn't silently lost.
      if (this.activePath === brief.path) this.activePath = null;
      if (!this.warned) {
        this.warned = true;
        toasts.error(
          "Couldn't reach NeuroSkill — session not started, so EEG won't attribute to this project. Is the NeuroSkill app running?",
        );
      }
    }
  }

  /** Clear activePath and fire the end label. `silent` suppresses the
   *  "session end wasn't recorded" toast (navigating away shouldn't pop one;
   *  the 4h cap backstops it). */
  end(opts: { silent?: boolean } = {}): void {
    const path = this.activePath;
    if (!path) return;
    this.activePath = null;
    projects.markSession(path, "end").then((ok) => {
      if (!ok && !opts.silent)
        toasts.error(
          "Couldn't reach NeuroSkill — the session end wasn't recorded (it auto-closes after 4h).",
        );
    });
  }
}

export const session = new SessionStore();
