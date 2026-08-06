import { useSyncExternalStore } from "react";
import { alertsList, type AlertEvent, type AlertRule } from "./api";
import type { MessageKey, Params } from "./i18n";
import type { ToastAction } from "./Toast";

/**
 * THE WATCHED THING — how an alert gets you to what it is about.
 *
 * A firing toast that only says "checkout falling behind" leaves the reader
 * doing the navigation: which screen, which group, which topic. The field
 * report was exactly that, and the fix is a second button beside Dismiss that
 * opens the thing the rule is watching.
 *
 * WHY A MODULE AND NOT A PROP, the same argument `stage.ts` makes. The alert
 * subscription lives in `ClusterView` — mounted for exactly as long as the
 * cluster is connected, which is exactly as long as the core is watching — but
 * the toast can be read from any of the ten screens, and the destination
 * (which rail item, which group) is owned half by the shell and half by
 * `ClusterView`'s placement record. Threading a navigator down into `Toast`
 * would put a copy of both in every component between them. So the SHELL
 * registers one function here and everything else asks this module.
 *
 * A DEAD BUTTON IS WORSE THAN NO BUTTON. `alertToastAction` returns
 * `undefined` while nothing is registered, and `useAlertNav` returns `null`,
 * so every caller renders the affordance only when it will actually work. That
 * also means this module is safe in tests, in the palette and anywhere the
 * shell has not mounted.
 *
 * WHERE EACH RULE POINTS
 *
 *   lag_threshold      → the group's own detail, where its per-partition lag
 *                        and its members are. That is the thing the rule is
 *                        literally watching.
 *   everything else    → the Alerts screen. `under_replicated`,
 *                        `offline_partitions` and `throughput_floor` are
 *                        cluster-wide readings with no single subject, and
 *                        sending someone to a group detail that has nothing to
 *                        do with the firing would be a wrong claim about what
 *                        tripped it.
 */

export type AlertTarget =
  /** One consumer group's detail on the Groups screen. */
  | { screen: "groups"; group: string }
  /** The Alerts screen itself — the rule, its evidence and the log. */
  | { screen: "alerts" };

export type AlertNavigate = (target: AlertTarget) => void;

// ---------------------------------------------------------------------------
// The registry
// ---------------------------------------------------------------------------

let navigate: AlertNavigate | null = null;
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of listeners) listener();
}

/**
 * The shell says how to get somewhere, once per connected cluster.
 *
 * Returns its own teardown, so the caller is `useEffect(() =>
 * registerAlertNav(id, go), […])` and a disconnect cannot leave a navigator
 * pointing at a cluster that is no longer on screen. The guard on the way out
 * (`navigate === go`) matters: React runs the NEXT effect before the previous
 * cleanup in some orders, and a blind `navigate = null` would then delete the
 * registration that had just replaced this one.
 *
 * It also PRIMES the rule cache. The labels below name the group a rule
 * watches, and a rule Kavka has never read is a rule it cannot name — priming
 * on connect means the very first firing of the session already knows.
 */
export function registerAlertNav(
  profileId: string,
  go: AlertNavigate,
): () => void {
  navigate = go;
  emit();
  void primeAlertRules(profileId);
  return () => {
    if (navigate !== go) return;
    navigate = null;
    emit();
  };
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

function snapshot(): AlertNavigate | null {
  return navigate;
}

/**
 * The navigator, or `null` while there is none — for components that render a
 * "take me there" control and must not render a dead one. Subscribed rather
 * than read once, because registration happens in an effect and a screen that
 * mounted first would otherwise never see it.
 */
export function useAlertNav(): AlertNavigate | null {
  return useSyncExternalStore(subscribe, snapshot, snapshot);
}

/** Go, if anyone is listening. Silent otherwise — see the header. */
export function goToAlertTarget(target: AlertTarget): void {
  navigate?.(target);
}

// ---------------------------------------------------------------------------
// What each rule is watching
// ---------------------------------------------------------------------------

/**
 * Rules by id, per profile.
 *
 * An `AlertEvent` carries `rule_id` and `rule_name` and nothing else — it
 * cannot say which group a lag rule watches, and the core is not going to
 * grow a field for the sake of a button label. So the rule list, which two
 * places already read, is remembered here.
 */
const known = new Map<string, Map<string, AlertRule>>();

/** Called by whoever has just read the rules — the Alerts screen, and the prime. */
export function rememberAlertRules(
  profileId: string,
  rules: readonly AlertRule[],
): void {
  known.set(profileId, new Map(rules.map((rule) => [rule.id, rule])));
}

/**
 * Read the rules once so the first firing already knows its subject. Failure
 * is silent and costs only the precision of a label: an unknown rule still
 * gets a working button, pointed at the Alerts screen.
 */
function primeAlertRules(profileId: string): Promise<void> {
  return alertsList(profileId)
    .then((rules) => rememberAlertRules(profileId, rules))
    .catch(() => undefined);
}

function knownRule(profileId: string, ruleId: string): AlertRule | null {
  return known.get(profileId)?.get(ruleId) ?? null;
}

/** Where a rule points. `null` — a rule Kavka has not read — points at Alerts. */
export function alertTarget(rule: AlertRule | null): AlertTarget {
  if (rule !== null && rule.kind === "lag_threshold") {
    const group = rule.group_id.trim();
    if (group.length > 0) return { screen: "groups", group };
  }
  return { screen: "alerts" };
}

export function alertTargetForEvent(
  profileId: string,
  ruleId: string,
): AlertTarget {
  return alertTarget(knownRule(profileId, ruleId));
}

// ---------------------------------------------------------------------------
// The toast's second button
// ---------------------------------------------------------------------------

/**
 * "View group demo-checkout" / "View the alert", beside Dismiss.
 *
 * THE LABEL AND THE DESTINATION ARE DECIDED TOGETHER, from one lookup. A
 * label that named a group while the handler resolved a destination later
 * could disagree with itself — the one failure a navigation button must not
 * have — so an unknown rule gets both the generic wording and the Alerts
 * screen, which is where its rule, its evidence and the log all are.
 *
 * `t` is passed in rather than read here: this is a plain module, and the
 * label has to come from the same catalog as the toast it lands in.
 */
export function alertToastAction(
  profileId: string,
  event: AlertEvent,
  t: (key: MessageKey, params?: Params) => string,
): ToastAction | undefined {
  if (navigate === null) return undefined;
  const target = alertTargetForEvent(profileId, event.rule_id);
  return {
    label:
      target.screen === "groups"
        ? t("alerts.toast.viewGroup", { group: target.group })
        : t("alerts.toast.viewAlerts"),
    // The one button on a firing toast that does something other than make it
    // go away. Dismiss stays quiet beside it.
    primary: true,
    run: () => goToAlertTarget(target),
  };
}
