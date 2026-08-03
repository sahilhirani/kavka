/**
 * The error library from docs/DESIGN.md §7, as a pure function.
 *
 * Three layers, always: plain title → cause and fix → the raw librdkafka
 * string verbatim behind `Show details`. This module owns the first two; the
 * third is never this module's business, because the raw string is shown
 * unmodified by whoever renders the banner.
 *
 * The generalizable rule: WHEN THE BROKER TELLS US THE ANSWER, PUT THE ANSWER
 * IN THE MESSAGE. Every branch below either names the address that failed or
 * names the setting to change — never both a shrug and a stack trace.
 *
 * PURE AND TOTAL. No React, no imports, no I/O, no `Date.now()`. Every branch
 * is decided by the input string alone, so the whole table is checkable by
 * calling it with a captured broker string and comparing the result. Keep it
 * that way: the moment this reaches for component state it stops being
 * testable and starts being a component.
 */

export interface ClassifiedError {
  /** Line 1 — what happened, in the user's vocabulary. */
  title: string;
  /** Line 2 — the next click. Empty only if there genuinely isn't one. */
  detail: string;
  /**
   * True when we recognised the cause. False means `title` is the raw string
   * and the caller is showing a broker message it could not translate — worth
   * knowing for telemetry, and worth not pretending otherwise in the UI.
   */
  known: boolean;
}

/**
 * Pull the first `host:port` out of a broker string. librdkafka writes
 * addresses as `broker-1.internal:9092/bootstrap` or `localhost:9092/1`, so
 * the trailing `/id` is stripped. Returns null rather than guessing.
 */
export function extractAddress(raw: string): string | null {
  // Bracketed IPv6 first: [::1]:9092
  const m6 = raw.match(/(\[[0-9A-Fa-f:.]+\]:\d{2,5})/);
  if (m6) return m6[1];
  const m = raw.match(/\b([A-Za-z0-9][A-Za-z0-9._-]*:\d{2,5})\b/);
  return m ? m[1] : null;
}

/** The SASL mechanism a broker named as acceptable, if it named one. */
function offeredMechanism(raw: string): string | null {
  const m = raw.match(/\b(SCRAM-SHA-(?:256|512)|GSSAPI|OAUTHBEARER|PLAIN)\b/g);
  if (!m) return null;
  // The last mechanism named is the broker's, not ours: librdkafka writes
  // "...mechanism PLAIN is not enabled, supported: SCRAM-SHA-512".
  return m[m.length - 1];
}

const has = (raw: string, ...needles: string[]) => {
  const s = raw.toLowerCase();
  return needles.some((n) => s.includes(n.toLowerCase()));
};

/**
 * Map a raw error string to the plain-language pair from the §7 table.
 * Order matters: the specific causes are tested before the generic ones,
 * because librdkafka nests them ("Failed to get metadata: Local: Broker
 * transport failure" is a transport failure, not a metadata timeout).
 */
export function classifyError(raw: string): ClassifiedError {
  const text = (raw ?? "").trim();
  const at = extractAddress(text);
  const where = at ?? "that broker";

  // ── Kavka's own IPC, before anything librdkafka says ──────────────────
  if (has(text, "unknown profile", "no such profile", "profile not found")) {
    return {
      title: "That connection isn't on this machine any more",
      detail:
        "It may have been deleted in another window. Pick another connection from the sidebar, or add it again.",
      known: true,
    };
  }
  if (has(text, "read-only", "read only connection")) {
    return {
      title: "Read-only connection — nothing was sent",
      detail:
        "This connection is marked read-only, so Kavka didn't write anything. Turn read-only off in the connection's settings if you meant to.",
      known: true,
    };
  }

  // ── Sign-in ───────────────────────────────────────────────────────────
  // Mechanism mismatch is checked first: it also matches "authentication
  // failed", and the broker has already told us which mechanism it wants.
  if (
    has(
      text,
      "not enabled",
      "unsupported sasl mechanism",
      "unsupported_sasl_mechanism",
      "mechanism handshake failed",
      "does not support",
    ) &&
    has(text, "sasl", "scram", "mechanism", "plain")
  ) {
    const offered = offeredMechanism(text);
    return {
      title: "The broker doesn't accept this sign-in mechanism",
      detail: offered
        ? `It offered ${offered}. Switch the mechanism and connect again.`
        : "Switch the SCRAM mechanism (or the sign-in method) and connect again.",
      known: true,
    };
  }
  if (
    has(
      text,
      "authentication failed",
      "sasl authentication",
      "saslauthentication",
      "invalid username or password",
      "authentication_failed",
      "err_sasl_authentication",
    )
  ) {
    return {
      title: "The broker rejected these credentials",
      detail:
        "Check the username, then re-enter the password — Kavka can't tell whether the stored one is still valid.",
      known: true,
    };
  }
  if (
    has(text, "topic_authorization", "cluster_authorization", "authorization failed", "not authorized")
  ) {
    return {
      title: "Connected, but this account can't list topics",
      detail:
        "It needs Describe on the cluster. Ask whoever issued the credentials for that permission.",
      known: true,
    };
  }

  // ── TLS ───────────────────────────────────────────────────────────────
  if (
    has(
      text,
      "certificate verify failed",
      "unable to get local issuer",
      "self signed certificate",
      "self-signed certificate",
      "unable to verify the first certificate",
      "certificate is not trusted",
    )
  ) {
    return {
      title: "The broker's certificate isn't trusted",
      detail: `${where} presented a certificate this machine's trust store doesn't recognise. Add the CA certificate as a PEM file — Kavka doesn't need a keystore.`,
      known: true,
    };
  }
  // Broker speaks plaintext, we spoke TLS. OpenSSL says so very distinctly.
  if (
    has(
      text,
      "wrong version number",
      "packet length too long",
      "unknown protocol",
      "record layer failure",
    )
  ) {
    return {
      title: "This broker isn't using TLS",
      detail: "Turn off “Encrypt the connection (TLS)” and connect again.",
      known: true,
    };
  }
  // librdkafka conflates two causes in this one string: the port answered
  // but spoke something other than Kafka (wrong port), or it is a TLS
  // listener. Honest title names both; port is the likelier mistake.
  if (has(text, "disconnected while requesting apiversion")) {
    return {
      title: `${where} answered, but not like a Kafka broker`,
      detail:
        "Either that port isn't Kafka — it usually runs on 9092, or 9093/9094 with TLS — or the broker wants an encrypted connection. Check the port first, then try turning on “Encrypt the connection (TLS)”.",
      known: true,
    };
  }
  // Broker speaks TLS, we spoke plaintext. This is librdkafka's own hint.
  if (
    has(
      text,
      "incorrect security.protocol",
      "connecting to a ssl listener",
      "ssl handshake failed",
    )
  ) {
    return {
      title: "This broker expects an encrypted connection",
      detail: "Turn on “Encrypt the connection (TLS)” and connect again.",
      known: true,
    };
  }

  // ── Reaching the host at all ──────────────────────────────────────────
  if (
    has(
      text,
      "failed to resolve",
      "name or service not known",
      "nodename nor servname",
      "no address associated",
      "getaddrinfo",
      "temporary failure in name resolution",
      "host not found",
    )
  ) {
    return {
      title: `Can't reach ${where}`,
      detail:
        "The hostname didn't resolve. Check the spelling, or whether you need to be on the VPN.",
      known: true,
    };
  }
  if (has(text, "connection refused", "econnrefused")) {
    return {
      title: `${where} refused the connection`,
      detail:
        "Nothing is listening there. If you're running Kafka in Docker, check the port is published to the host.",
      known: true,
    };
  }
  if (
    has(
      text,
      "connection timed out",
      "etimedout",
      "connect timed out",
      "connection setup timed out",
      "no route to host",
    )
  ) {
    return {
      title: `${where} didn't answer`,
      detail:
        "The address is routable but nothing answered on that port. Check the port number, or whether the broker is running.",
      known: true,
    };
  }
  if (has(text, "broker transport failure", "all broker connections are down")) {
    return {
      title: `Can't reach ${where}`,
      detail:
        "The connection didn't get far enough to speak Kafka. Check the address and port, then whether a VPN or firewall is in the way.",
      known: true,
    };
  }

  // ── Connected, but the cluster didn't finish the job ───────────────────
  // Timeout wording is required; "metadata" alone proves nothing about why.
  if (has(text, "timed out", "timeout")) {
    return {
      title: "Connected, but the cluster didn't answer in time",
      detail:
        "The broker accepted the connection but didn't return metadata. It may be overloaded, or a firewall may be blocking the address the broker advertises — which can differ from the one you typed.",
      known: true,
    };
  }

  // ── Fallback: keep the broker's own words as the title ─────────────────
  // No apology, no "Something went wrong". The raw string is still the most
  // informative thing we have, so it is shown rather than buried, and the
  // full text stays available under Show details.
  return {
    title: text.length > 0 ? firstLine(text) : "The connection attempt failed",
    detail:
      "Kavka doesn't recognise this one. The broker's full reply is under Show details — it usually names the host or the setting at fault.",
    known: false,
  };
}

function firstLine(text: string): string {
  const line = text.split("\n")[0].trim();
  return line.length > 160 ? `${line.slice(0, 157)}…` : line;
}
