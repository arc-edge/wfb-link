## Approach

Both commands already validate and build descriptor options before opening USB. This change wraps the existing transport open result in `RuntimeRadioSession`, selects bulk-OUT through the session, and calls `submit_80211_frame` for each live transmission.

Control-register side effects for TX LED and TX status remain diagnostic-side and borrow `session.transport`, matching the bridge commands. Report counters continue to be derived from the existing diagnostic submit, LED, and TX-status reports so external JSON remains stable.
