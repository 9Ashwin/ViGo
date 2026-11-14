import type { HelperStatus } from '../shared/protocol';

/** Starts once per permission grant; an allowed-but-refused helper waits for a real change or explicit retry. */
export class AccessibilityGate {
  private attempted = false;
  private wasTrusted = false;

  retry(): void { this.attempted = false; }

  observe(trusted: boolean, helper: HelperStatus): {
    status: HelperStatus; stale: boolean; action: 'start' | 'stop' | null;
  } {
    if (!trusted) {
      const stop = this.wasTrusted && (helper === 'running' || helper === 'starting');
      this.wasTrusted = false;
      this.attempted = false;
      return { status: 'noAccess', stale: false, action: stop ? 'stop' : null };
    }
    this.wasTrusted = true;
    if (!this.attempted) {
      this.attempted = true;
      return { status: 'starting', stale: false, action: 'start' };
    }
    return { status: helper, stale: helper === 'noAccess', action: null };
  }
}
