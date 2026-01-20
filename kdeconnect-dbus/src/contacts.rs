//! Contact loading and lookup from KDE Connect synced vCard files.
//!
//! KDE Connect syncs contacts as vCard files to ~/.local/share/kpeoplevcard/kdeconnect-{device-id}/

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// A contact with name and phone numbers.
#[derive(Debug, Clone)]
pub struct Contact {
    /// Display name (from FN field).
    pub name: String,
    /// List of phone numbers associated with this contact.
    pub phone_numbers: Vec<String>,
}

/// Contact lookup cache mapping normalized phone numbers to contact names.
#[derive(Debug, Clone, Default)]
pub struct ContactLookup {
    /// Map from normalized phone number to contact name.
    phone_to_name: HashMap<String, String>,
    /// Full list of contacts for name-based searching.
    contacts: Vec<Contact>,
}

impl ContactLookup {
    /// Create a new empty contact lookup.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load contacts from the kpeoplevcard directory for a specific device.
    pub fn load_for_device(device_id: &str) -> Self {
        let mut lookup = Self::new();

        // Get the kpeoplevcard directory
        let vcard_dir = match dirs::data_local_dir() {
            Some(dir) => dir
                .join("kpeoplevcard")
                .join(format!("kdeconnect-{}", device_id)),
            None => {
                tracing::warn!("Could not find local data directory for contacts");
                return lookup;
            }
        };

        if !vcard_dir.exists() {
            tracing::debug!("vCard directory does not exist: {:?}", vcard_dir);
            return lookup;
        }

        // Read all .vcf files
        let entries = match fs::read_dir(&vcard_dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("Failed to read vCard directory: {}", e);
                return lookup;
            }
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "vcf").unwrap_or(false) {
                if let Some(contact) = parse_vcard(&path) {
                    for phone in &contact.phone_numbers {
                        let normalized = normalize_phone_number(phone);
                        if !normalized.is_empty() {
                            lookup
                                .phone_to_name
                                .insert(normalized, contact.name.clone());
                        }
                    }
                    lookup.contacts.push(contact);
                }
            }
        }

        // Sort contacts alphabetically by name for consistent display
        lookup
            .contacts
            .sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        tracing::info!(
            "Loaded {} contacts with {} phone mappings",
            lookup.contacts.len(),
            lookup.phone_to_name.len()
        );
        lookup
    }

    /// Look up a contact name by phone number.
    /// Returns None if no contact is found.
    pub fn get_name(&self, phone_number: &str) -> Option<&str> {
        let normalized = normalize_phone_number(phone_number);
        self.phone_to_name.get(&normalized).map(|s| s.as_str())
    }

    /// Look up a contact name by phone number, returning the phone number if not found.
    /// Also falls back to phone number if the stored name is empty.
    pub fn get_name_or_number(&self, phone_number: &str) -> String {
        self.get_name(phone_number)
            .filter(|s| !s.trim().is_empty())
            .map(|s| s.to_string())
            .unwrap_or_else(|| phone_number.to_string())
    }

    /// Returns the number of contacts loaded.
    pub fn len(&self) -> usize {
        self.phone_to_name.len()
    }

    /// Returns true if no contacts are loaded.
    pub fn is_empty(&self) -> bool {
        self.phone_to_name.is_empty()
    }

    /// Search contacts by name (case-insensitive prefix/substring match).
    /// Returns up to `limit` matching contacts.
    pub fn search_by_name(&self, query: &str, limit: usize) -> Vec<&Contact> {
        if query.is_empty() {
            return Vec::new();
        }

        let query_lower = query.to_lowercase();

        self.contacts
            .iter()
            .filter(|c| c.name.to_lowercase().contains(&query_lower))
            .take(limit)
            .collect()
    }

    /// Get all contacts (for browsing).
    pub fn all_contacts(&self) -> &[Contact] {
        &self.contacts
    }
}

/// Normalize a phone number by removing non-digit characters.
/// Also handles country code variations (e.g., +1 vs 1 vs no prefix).
fn normalize_phone_number(phone: &str) -> String {
    // Extract only digits
    let digits: String = phone.chars().filter(|c| c.is_ascii_digit()).collect();

    // If it's a US number (10 or 11 digits starting with 1), normalize to 10 digits
    if digits.len() == 11 && digits.starts_with('1') {
        digits[1..].to_string()
    } else {
        digits
    }
}

/// Parse a vCard file and extract the contact information.
fn parse_vcard(path: &PathBuf) -> Option<Contact> {
    let content = fs::read_to_string(path).ok()?;

    let mut name = String::new();
    let mut phone_numbers = Vec::new();

    for line in content.lines() {
        // Handle FN (Full Name) field
        if let Some(fn_value) = line.strip_prefix("FN:") {
            name = fn_value.trim().to_string();
        }
        // Handle TEL (telephone) fields - various formats
        else if line.starts_with("TEL") {
            // TEL;CELL:1234567890
            // TEL;TYPE=CELL:1234567890
            // TEL:1234567890
            if let Some(idx) = line.find(':') {
                let number = line[idx + 1..].trim().to_string();
                if !number.is_empty() && !number.contains('=') {
                    // Skip encoded numbers for now
                    phone_numbers.push(number);
                }
            }
        }
    }

    if name.is_empty() || phone_numbers.is_empty() {
        return None;
    }

    Some(Contact {
        name,
        phone_numbers,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_phone_number() {
        assert_eq!(normalize_phone_number("(555) 123-4567"), "5551234567");
        assert_eq!(normalize_phone_number("+1-555-123-4567"), "5551234567");
        assert_eq!(normalize_phone_number("15551234567"), "5551234567");
        assert_eq!(normalize_phone_number("555.123.4567"), "5551234567");
    }
}
