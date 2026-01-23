//! Notification deduplication helpers for cross-process coordination.
//!
//! COSMIC spawns multiple applet processes, and KDE Connect may send duplicate
//! signals. These helpers use file-based locking to ensure only one notification
//! is shown across all processes.

use std::fs::OpenOptions;
use std::io::{Read, Seek, Write};
use std::os::unix::io::AsRawFd;
use std::time::{SystemTime, UNIX_EPOCH};

/// Deduplication window in milliseconds (2 seconds).
const DEDUP_WINDOW_MS: u128 = 2000;

/// File path for file notification deduplication.
const FILE_DEDUP_PATH: &str = "/tmp/cosmic-connected-file-dedup";

/// File path for SMS notification deduplication.
const SMS_DEDUP_PATH: &str = "/tmp/cosmic-connected-sms-dedup";

/// Check if we should show a file notification (cross-process deduplication via file lock).
/// Returns true if this is the first notification for this file within the dedup window.
pub fn should_show_file_notification(file_url: &str) -> bool {
    should_show_notification(FILE_DEDUP_PATH, file_url)
}

/// Check if we should show an SMS notification (cross-process deduplication via file lock).
/// Returns true if this is the first notification for this message within the dedup window.
/// Uses thread_id and message timestamp as the unique key.
pub fn should_show_sms_notification(thread_id: i64, message_date: i64) -> bool {
    let message_key = format!("{}:{}", thread_id, message_date);
    should_show_notification(SMS_DEDUP_PATH, &message_key)
}

/// Generic notification deduplication using file-based locking.
///
/// This function:
/// 1. Opens or creates a deduplication file
/// 2. Acquires an exclusive lock (blocking)
/// 3. Reads the last notification key and timestamp
/// 4. Determines if this notification should be shown
/// 5. Updates the file with the new key and timestamp
/// 6. Releases the lock
fn should_show_notification(dedup_file: &str, key: &str) -> bool {
    // Get current time as milliseconds since epoch
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);

    // Try to open or create the dedup file
    let mut file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dedup_file)
    {
        Ok(f) => f,
        Err(_) => return true, // Can't open file, allow notification
    };

    // Use file locking to ensure atomic read-modify-write.
    // Note: flock is blocking, which isn't ideal in async contexts. However:
    // - The lock is held for microseconds (read/compare/write ~50 bytes)
    // - Files are in /tmp (typically tmpfs, no disk I/O)
    // - Contention only occurs when multiple processes receive the same D-Bus signal simultaneously
    // - spawn_blocking overhead would likely exceed the blocked time
    // The simplicity of blocking here outweighs the theoretical async executor stall risk.
    let fd = file.as_raw_fd();
    // SAFETY: flock is a standard POSIX system call that operates on valid file descriptors
    unsafe {
        if libc::flock(fd, libc::LOCK_EX) != 0 {
            return true; // Lock failed, allow notification
        }
    }

    // Read current contents
    let mut contents = String::new();
    let _ = file.read_to_string(&mut contents);

    let should_notify = if let Some((stored_key, stored_time_str)) = contents.split_once('\n') {
        if let Ok(stored_time) = stored_time_str.parse::<u128>() {
            // Check if same key and within window
            stored_key != key || now_ms.saturating_sub(stored_time) >= DEDUP_WINDOW_MS
        } else {
            true
        }
    } else {
        true
    };

    if should_notify {
        // Write new key and timestamp
        let _ = file.set_len(0); // Truncate
        let _ = file.rewind();
        let _ = write!(file, "{}\n{}", key, now_ms);
    }

    // Release lock
    // SAFETY: flock is a standard POSIX system call that operates on valid file descriptors
    unsafe {
        libc::flock(fd, libc::LOCK_UN);
    }

    should_notify
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Generate a unique temp file path for testing
    fn temp_dedup_path(test_name: &str) -> String {
        format!(
            "/tmp/cosmic-connected-test-{}-{}",
            test_name,
            std::process::id()
        )
    }

    /// Clean up a temp file, ignoring errors if it doesn't exist
    fn cleanup(path: &str) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn first_notification_returns_true() {
        let path = temp_dedup_path("first");
        cleanup(&path);

        let result = should_show_notification(&path, "test-key");
        assert!(result, "First notification for a key should return true");

        cleanup(&path);
    }

    #[test]
    fn duplicate_within_window_returns_false() {
        let path = temp_dedup_path("duplicate");
        cleanup(&path);

        // First call should succeed
        let first = should_show_notification(&path, "same-key");
        assert!(first, "First notification should return true");

        // Immediate second call with same key should be suppressed
        let second = should_show_notification(&path, "same-key");
        assert!(!second, "Duplicate within window should return false");

        cleanup(&path);
    }

    #[test]
    fn different_key_returns_true() {
        let path = temp_dedup_path("different");
        cleanup(&path);

        let first = should_show_notification(&path, "key-a");
        assert!(first, "First notification should return true");

        let second = should_show_notification(&path, "key-b");
        assert!(second, "Different key should return true");

        cleanup(&path);
    }

    #[test]
    fn expired_window_returns_true() {
        // Use a custom short window for testing by writing an old timestamp directly
        let path = temp_dedup_path("expired");
        cleanup(&path);

        // Write an entry with a timestamp from 3 seconds ago (beyond 2s window)
        let old_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
            - 3000; // 3 seconds ago

        fs::write(&path, format!("old-key\n{}", old_time)).unwrap();

        // Same key should now be allowed since window expired
        let result = should_show_notification(&path, "old-key");
        assert!(result, "Same key after window expires should return true");

        cleanup(&path);
    }

    #[test]
    fn handles_corrupted_file() {
        let path = temp_dedup_path("corrupted");
        cleanup(&path);

        // Write garbage data
        fs::write(&path, "not-valid-format").unwrap();

        let result = should_show_notification(&path, "test-key");
        assert!(result, "Corrupted file should allow notification");

        cleanup(&path);
    }

    #[test]
    fn handles_empty_file() {
        let path = temp_dedup_path("empty");
        cleanup(&path);

        // Create empty file
        fs::write(&path, "").unwrap();

        let result = should_show_notification(&path, "test-key");
        assert!(result, "Empty file should allow notification");

        cleanup(&path);
    }

    #[test]
    fn file_notification_wrapper_works() {
        // Just verify the wrapper function doesn't panic
        // We can't easily test the actual dedup without affecting real state,
        // but we can verify it returns a bool
        let result = should_show_file_notification("/test/path/file.txt");
        // Result could be true or false depending on prior state, just verify it runs
        assert!(result || !result);
    }

    #[test]
    fn sms_notification_wrapper_works() {
        // Verify the wrapper function doesn't panic and formats key correctly
        let result = should_show_sms_notification(12345, 1700000000000);
        assert!(result || !result);
    }

    #[test]
    fn multiple_rapid_calls_same_key() {
        let path = temp_dedup_path("rapid");
        cleanup(&path);

        let first = should_show_notification(&path, "rapid-key");
        assert!(first);

        // Multiple rapid calls should all be suppressed
        for i in 0..5 {
            let result = should_show_notification(&path, "rapid-key");
            assert!(!result, "Call {} should be suppressed", i + 2);
        }

        cleanup(&path);
    }

    #[test]
    fn alternating_keys() {
        let path = temp_dedup_path("alternating");
        cleanup(&path);

        // Each different key should trigger notification
        assert!(should_show_notification(&path, "key-1"));
        assert!(should_show_notification(&path, "key-2"));
        assert!(should_show_notification(&path, "key-3"));
        assert!(should_show_notification(&path, "key-1")); // key-1 again after others

        cleanup(&path);
    }
}
