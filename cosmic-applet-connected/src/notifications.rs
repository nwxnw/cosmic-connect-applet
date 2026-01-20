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

    // Use file locking to ensure atomic read-modify-write
    let fd = file.as_raw_fd();

    // Acquire exclusive lock (blocking)
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
