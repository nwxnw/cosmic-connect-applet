# SMS Implementation Notes

Current SMS behavior in Connected, plus known limitations and possible follow-up work.

## Current Status

Implemented:

- Conversation-list bootstrap is cache-first and primarily signal-driven.
- Bootstrap re-reads `activeConversations()` during initial sync, settles after a quiet window, and retries once on suspiciously small cold-start results.
- Warm starts preserve cached timeout semantics instead of fabricating activity.
- Conversation threads use a long-lived subscription with deferred initial scroll, pagination, and optimistic reply reconciliation.
- Device selection can prefetch cached conversation heads before the SMS view opens.

Possible next steps:

- Reduce the differences between conversation-list and thread-loading synchronization models.
- Keep reviewing thread correctness around deduplication, pagination heuristics, notification interaction, scroll preservation, and reply/cache-priming assumptions.
- Keep bootstrap logging focused on cache size, signal activity, retry use, and final settled counts.

## Architecture Overview

### Conversation List Loading

Conversation-list loading is cache-first and long-lived:

1. Opening the SMS view starts `conversation_list_subscription`.
2. The subscription installs D-Bus match rules before firing bootstrap requests.
3. Bootstrap requests fire immediately, before anything is emitted, so the phone round trip is in flight as early as possible.
4. Cached conversations from `activeConversations()` are emitted as one batch; later data arrives as `conversationCreated` / `conversationUpdated` signals and merges into the list.
5. A quiet window or bootstrap deadline dismisses the sync indicator.
6. The subscription remains alive while the SMS view is open.

Important details:

- Warm starts use cached rows immediately and a shorter bootstrap window.
- Cold starts use a longer wait and one bounded retry.
- The daemon cache is read exactly once, before bootstrap requests fire. Everything after that arrives as signals, and there is no follow-up poll — a missed signal is not backfilled by a later cache read.

### Message Thread Loading

Thread loading uses a long-lived subscription with distinct startup phases:

1. Opening a thread starts `conversation_message_subscription`.
2. Match rules are installed before requests are fired.
3. Two `requestConversation()` calls are made:
   - SMS plugin request for daemon cache priming
   - Conversations request for per-message UI signals
4. The local persistent-store phase ends at `conversationLoaded`.
5. A phone-response window stays open after local-store completion to catch delayed phone data.
6. The subscription then continues listening for new incoming messages and sent-message echoes until the thread closes.

Important details:

- The list is rendered oldest-first, so an unscrolled scrollable lands on the oldest message. Auto-scroll-to-bottom is dispatched on `ConversationStoreLoaded`, on `ConversationLoadComplete`, on each `ConversationMessageReceived` while `!initial_load_complete`, and on confirmed sent-message echoes. The per-message dispatch covers cached-store hits where the daemon's worker satisfies the request entirely from `m_conversations` and never emits `conversationLoaded` (see `requestconversationworker.cpp`'s `numHandled >= howMany` branch), which would otherwise leave the user pinned at the top of a long thread.
- `conversationLoaded` reflects local-store count, not authoritative phone total.
- `initial_load_complete` gates scroll-based loading of older messages, and bounds the per-message auto-scroll so a new incoming SMS arriving while the user is reading older content doesn't yank them down.
- The daemon writes phone-supplied messages to the local store asynchronously, so the first-open Conversations worker may finish before that data lands. The daemon's `addMessages()` only emits `conversationUpdated` for the latest message in a thread, so historical backfill from the phone arrives silently — observable only via a second `conversationLoaded(count)` emission with a higher count than we've received. Recovery is bounded to one re-issued `requestConversation` per thread open, with two triggers:
  - **Primary** (Option 1): a duplicate `conversationLoaded` arrives with `store_count > received_message_count` while we're still under-filled (`received < messages_per_page`). The retry fires immediately. Catches both the original "received only 1 of N" truncation and the off-by-one "received N-1 of N" case where the daemon's worker emits one fewer per-message signal than its store reports. The page-size guard avoids firing on natural scroll-pagination boundaries.
  - **Fallback**: `phone_deadline` expires with `received <= 1` — used if the daemon doesn't re-emit `conversationLoaded` (e.g. the phone added no new UIDs, or a signal-ordering race). Narrow gate kept here on purpose: if no duplicate fired, retry against an unchanged store would just re-deliver what we already have.
- The retry re-reads the Conversations interface as `requestConversation(threadId, 0, received + page)`. **The offset is deliberately pinned to 0 and only the range end moves.** The daemon serves newest-first, so widening the range returns the same messages, and `crbegin() + 0` can never index past the end of its cache. A non-zero `start` against a cache holding fewer messages **segfaults kdeconnectd 23.08.5** (no bounds check between the D-Bus wire and the iterator in `requestconversationworker.cpp`) and silently returns nothing on newer daemons - so an out-of-range offset is an applet-side correctness bug against every daemon version, and the daemon does not restart itself. Clamping `start` against the last known store count is **not** an alternative: every count the applet holds is stale in exactly the crash scenario (a restarted daemon with a cold cache) and it cannot detect that, so the clamp is circular. The resulting per-message signals merge via `known_message_ids` dedup.

### Reaction-Thread Merging

iOS reactions over SMS arrive on slightly different address-sets and AOSP buckets them into a separate `threadId`. The phone re-merges visually; KDE Connect / Connected report them separately. Connected wraps each user-perceived conversation in a `LogicalConversation` (`sms/logical.rs`) that may collapse multiple underlying SMS threadIds.

Merging precondition — the grouping key in `merge_into_logical` (`sms/logical.rs`) is `(canonical address-set, conversation-level sub_id)`:

- both `sub_id`s are non-`-1` and equal, AND
- canonical address-sets are equal (digit-only normalize, leading-`1` stripped, deduplicated as a set).

Each `LogicalConversation` carries:

- `primary_thread_id` — the most-recently-active sibling within the merged set; used as the reply target.
- `merged_thread_ids` — all underlying threadIds composing this logical conversation. Always contains `primary_thread_id`. Single-element for non-merged.

Opening a merged conversation fans out the message subscription: one `conversation_message_subscription` per underlying threadId, each firing its own `requestConversation` and emitting `ConversationMessageReceived` for its own thread. Signal handlers accept any thread in the open `current_merged_thread_ids` set rather than only the primary.

Multi-subscription completion semantics:

- `ConversationLoadComplete` is idempotent. Math (sort, `messages_has_more`, `last_seen_sms`) always runs; loading-state clear and scroll-to-bottom snap fire only on the first arrival. Late completions silently refresh stats so a slow-completing subscription can't yank the user back to bottom.
- `thread_has_more: HashMap<i64, bool>` tracks exhaustion per thread as `loaded_t < total_count`, counting only messages carrying that `thread_id` — so per-thread counts are directly comparable and merged-set size no longer forces a heuristic. The heuristic survives only for `total_count == 0`, which the daemon uses to mean *unknown*, not *empty*; there it falls back to `loaded_t >= MESSAGES_PER_PAGE`. `messages_has_more` is the OR across `thread_has_more` over `current_merged_thread_ids`.

### Reply target rule

When the user sends a reply into a conversation, Connected picks the threadId to pass to `replyToConversation` based on the merge state of the open conversation:

- **Symmetric merge** (canonical address-sets equal across the merged group — the case M7's primary-equality heuristic produces): redirect to `primary_thread_id`. This matches AOSP's outgoing-reply canonicalization, so the echo lands on the threadId Connected passed and the optimistic-send reconciliation can complete cleanly. As a side effect, the redirect bypasses AOSP's per-bucket processing that would otherwise produce **recipient-side duplicate delivery** — the recipient receives one copy instead of two.
- **Asymmetric / subset clause** (untested across captured pairs; reintroduced if/when the subset clause returns to the merge heuristic): preserve the displayed thread's threadId. Conservative until field data confirms the redirect is address-safe under subset shapes. The branch is dormant under M7's primary-equality heuristic — every merged set is symmetric by construction — so production paths today take the symmetric arm exclusively.
- **Non-merged or unknown thread**: pass the displayed threadId through unchanged.

The duplicate-delivery side effect is empirically locked. Pre-merge behavior on a known reaction-bucket pair (Pair 4 captured 2026-05-02) reproduced two-copy delivery to the recipient. Under the redirect, the same pair delivers one copy. The redirect is therefore a corrective fix for a recipient-visible bug, not just a display-merge convenience.

The reply-target rule applies only when merging is on. With merging off (see "Per-entry markers and SMS-view toggle" below), Connected sends to whichever underlying thread the user opened. Replying into the non-canonical sibling thread of a reaction-bucket pair will reproduce the AOSP-canonicalization symptoms — the echo arrives on the canonical primary instead of the displayed thread, the optimistic-send "Sending…" indicator can stay pinned, and the recipient may receive duplicate copies. This is documented behavior gated behind the user opt-out, not a regression.

### Per-entry markers and SMS-view toggle

The SMS conversation list shows a small marker on rows that participate in a reaction-bucket group. Two glyphs:

- **Merge marker** (visible when the merge toggle is on): rows whose `LogicalConversation.merged_thread_ids.len() > 1` show a converging-Y glyph next to the message preview. Indicates "this conversation merges multiple phone-side threads."
- **Split marker** (visible when the merge toggle is off): rows whose underlying thread has at least one reaction-bucket sibling in the conversation list show a parallel-arrows glyph next to the message preview. Indicates "this conversation has a sibling thread on the phone; turning the merge toggle on would combine them."

A header toggle in the SMS view (between the conversation-list title and the new-message button) switches between merged and split states. The toggle uses the same iconography as the per-entry markers — converging-Y when merging is on, parallel-arrows when off — and dispatches the same `Message::ToggleSetting(SettingKey::MergeReactionThreads)` as the M13 settings option, so the two surfaces share state automatically. Toggling either one updates the other, and the toggle state persists across applet restarts.

The merge-off path uses the same grouping key as the merge-on path, so any pair the merge logic *would* combine also appears as split-marker entries when the user has merging off. When the v0.6.0+ subset clause returns to the heuristic, both surfaces pick it up automatically without further coordination.

### Older Message Loading

Older messages are loaded automatically when the user scrolls near the top of the thread.

- Scroll position and content height are captured before the fetch.
- Older messages are prepended when they arrive.
- Scroll offset is adjusted so the user stays anchored near the same visible messages.

For merged conversations the fetch fans out: one request per threadId in `current_merged_thread_ids` that still has more to give (`thread_has_more`), each carrying its own already-loaded count. The page is held open until every targeted thread has answered (`OlderPageLoad.pending`), so the prepend and the scroll-offset adjustment happen once against the whole batch rather than per thread. `messages_has_more` is the OR across `thread_has_more`, so the thread stops paginating only when every merged thread is exhausted.

Each request is `requestConversation(threadId, 0, loaded_count + page)` — **the offset is pinned to 0 here for the same reason as the truncation retry above**: a non-zero `start` segfaults kdeconnectd 23.08.5.

## Sending Behavior

### Replies

Replies use `replyToConversation(threadId, message, attachments)` on the Conversations D-Bus interface. This preserves thread context, including group conversations, but depends on the daemon's in-memory `m_conversations` cache being primed first.

On success:

- the conversation preview updates immediately with the latest body and timestamp
- an optimistic sent bubble is inserted into the open thread
- the long-lived message subscription reconciles that optimistic entry when the phone echoes back the real sent message

For merged conversations, optimistic-send reconciliation matches by `OPTIMISTIC_MESSAGE_UID` + body + 5-minute window with no thread-id filter, so an echo arriving on a sibling thread within `merged_thread_ids` still upgrades the optimistic bubble in place. Combined with the symmetric-merge reply-target redirect (see "Reply target rule" above), this is what closes the present-tense "stuck spinner" UX behavior that pre-merge code paths produced when AOSP canonicalized the outgoing message into a non-displayed sibling thread.

### New Messages

New-message compose uses `sendWithoutConversation(addresses, message, attachments)` with explicit recipients. On success, the compose flow returns to the conversation list and keeps the conversation-list subscription active so the phone can sync back the resulting thread.

### Cache Priming

Thread loading fires two requests because they serve different purposes:

- the SMS plugin request populates the daemon's in-memory `m_conversations` cache
- the Conversations request emits the per-message signals used by the UI

Reply sending depends on the first; thread rendering depends on the second.

## Loading and Caching

Loading state is tracked with `SmsLoadingState`:

- `Idle`
- `LoadingConversations(Connecting|Requesting)`
- `LoadingMessages(Connecting|Requesting)`
- `LoadingMoreMessages`

Caching behavior:

- Re-opening SMS for the same device reuses in-memory conversation data and refreshes in the background.
- Switching devices clears device-specific SMS state as needed.
- Contacts are loaded per device from KDE Connect's synced vCard directory and reused for same-device reopens. Loading once at SMS-view open and preserving across re-opens avoids a race where async contact loading completes after the conversation list has already rendered with phone numbers.

### Contact Name Resolution

`ContactLookup` parses vCards from `~/.local/share/kpeoplevcard/kdeconnect-{device-id}/`.

- `get_name_or_number(&address)` — resolves a single address. Used for per-message sender labels in thread view.
- `get_group_display_name(&addresses, limit)` — resolves multiple addresses into a comma-separated contact list (e.g. "Alice, Bob, Charlie, ..."). Used in the conversation list, thread header, and SMS notifications.

## MMS Attachments

The KDE Connect daemon sets its Qt application name to `"kdeconnect.daemon"` in `kdeconnectd.cpp`. Qt's `QStandardPaths::CacheLocation` resolves to `~/.cache/<applicationName>/`, so MMS attachments are cached at `~/.cache/kdeconnect.daemon/<device-name>/<uniqueIdentifier>` (e.g. `PART_1762553269778`). Files have no extension — the MIME type comes from the message's attachment metadata. The Flatpak manifest must include `--filesystem=xdg-cache/kdeconnect.daemon:ro` for the applet to read cached attachments.

## Known Constraints

- `conversationLoaded` reports the local persistent-store count, not the phone's authoritative total.
- Reply sending still depends on daemon cache priming before `replyToConversation` can work reliably.
- Notification correctness depends on careful `last_seen_sms` handling when opening threads and merging incoming data.

## Reference

### Key Symbols

Messages (see `app.rs`):

- `ConversationReceived` — cached or newly discovered conversation summary
- `ConversationSyncStarted` / `ConversationSyncComplete` — spinner lifecycle for the list
- `ConversationMessageReceived` — individual message during thread load or live updates
- `ConversationStoreLoaded` — local persistent-store phase finished (triggers initial scroll)
- `ConversationLoadComplete` — phone-response window elapsed (sets `initial_load_complete`)

- Timeout constants (see `constants.rs`):

- `CONVERSATION_LIST_PHONE_WAIT_MS` — cold-start bootstrap ceiling
- `CONVERSATION_TIMEOUT_CACHED_SECS` — warm-start bootstrap window
- `CONVERSATION_LIST_QUIET_MS` — quiet-window settle after bootstrap activity
- `CONVERSATION_LIST_RETRY_THRESHOLD` / `CONVERSATION_LIST_RETRY_WAIT_MS` — cold-start retry gate and window
- `PHONE_RESPONSE_TIMEOUT_MS` — thread phone-response window after `conversationLoaded`
- `CONVERSATION_RETRY_WAIT_MS` — settle window for the one-shot Conversations-interface re-read fired when first-open truncation is suspected
- `MESSAGE_SUBSCRIPTION_TIMEOUT_SECS` — Phase 1 local-store safety-net timeout
- `NEW_MESSAGE_SYNC_INDICATOR_SECS` — post-send window the conversation-list sync indicator stays up while waiting for the phone to report the new conversation

D-Bus surface:

- Device base path: `/modules/kdeconnect/devices/{id}`
- Conversations interface: `org.kde.kdeconnect.device.conversations` (signals: `conversationCreated`, `conversationUpdated`, `conversationLoaded`)
- SMS plugin path: `/modules/kdeconnect/devices/{id}/sms` (`org.kde.kdeconnect.device.sms`) — used for cache priming via `requestConversation` / `requestAllConversations`

### Message Types

- Message types: `1 = inbox`, `2 = sent`, `3 = draft`, `4 = outbox`, `5 = failed`, `6 = queued`
- Message fields relied on by the app: body, addresses, date, type, read, thread ID, UID, sub ID, attachments

## Related Docs

- `docs/KNOWN_ISSUES.md`
- `docs/NOTIFICATIONS.md`
- `docs/DBUS.md`
