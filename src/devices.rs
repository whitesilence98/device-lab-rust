use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Device {
    pub instance_id: String,
    pub container_id: Option<String>,
    pub name: String,
    pub manufacturer: String,
    pub class: String,
    pub enumerator: String,
    pub parent: String,
    pub hardware_ids: Vec<String>,
}

impl Device {
    pub fn key(&self) -> String {
        self.instance_id.to_ascii_lowercase()
    }

    pub fn title(&self) -> &str {
        if self.name.is_empty() {
            "Unknown device"
        } else {
            &self.name
        }
    }

    pub fn matches(&self, query: &str) -> bool {
        let query = query.to_lowercase();
        [
            &self.name,
            &self.manufacturer,
            &self.class,
            &self.instance_id,
            &self.enumerator,
        ]
        .into_iter()
        .chain(self.hardware_ids.iter())
        .any(|value| value.to_lowercase().contains(&query))
    }

    pub fn usb_ids(&self) -> Option<(u16, u16)> {
        self.hardware_ids.iter().find_map(|id| {
            let id = id.to_ascii_uppercase();
            // Only parse USB/HID VID/PID identifiers, not unrelated PCI IDs.
            if !(id.starts_with("USB\\") || id.starts_with("HID\\")) {
                return None;
            }
            let mut vid = None;
            let mut pid = None;
            for part in id.split(['\\', '&']) {
                if let Some(value) = part.strip_prefix("VID_").filter(|v| v.len() == 4) {
                    vid = u16::from_str_radix(value, 16).ok();
                }
                if let Some(value) = part.strip_prefix("PID_").filter(|v| v.len() == 4) {
                    pid = u16::from_str_radix(value, 16).ok();
                }
            }
            vid.zip(pid)
        })
    }
}

pub struct Group<'a> {
    pub key: String,
    pub members: Vec<&'a Device>,
}

pub fn groups(devices: &[Device]) -> Vec<Group<'_>> {
    let mut grouped: BTreeMap<String, Vec<&Device>> = BTreeMap::new();
    for device in devices {
        let key = device
            .container_id
            .as_ref()
            .filter(|id| !id.is_empty() && id.chars().any(|c| c.is_ascii_hexdigit() && c != '0'))
            .map(|id| format!("container:{}", id.to_ascii_lowercase()))
            .unwrap_or_else(|| format!("instance:{}", device.key()));
        grouped.entry(key).or_default().push(device);
    }
    let mut result: Vec<_> = grouped
        .into_iter()
        .map(|(key, mut members)| {
            members.sort_by_key(|d| (d.title().to_lowercase(), d.key()));
            Group { key, members }
        })
        .collect();
    result.sort_by_key(|group| (group.members[0].title().to_lowercase(), group.key.clone()));
    result
}

#[derive(Default, Debug, PartialEq, Eq)]
pub struct Changes {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

#[derive(Default)]
pub struct Inventory {
    pub devices: Vec<Device>,
    pub last_good: Option<Instant>,
    pub error: Option<String>,
    pub changes: Changes,
}

impl Inventory {
    pub fn apply(&mut self, result: Result<Vec<Device>, String>) {
        match result {
            Ok(mut next) => {
                let old: BTreeMap<_, _> = self.devices.iter().map(|d| (d.key(), d)).collect();
                let keys: BTreeSet<_> = next.iter().map(Device::key).collect();
                self.changes = Changes {
                    added: next.iter().filter(|d| !old.contains_key(&d.key())).count(),
                    updated: next
                        .iter()
                        .filter(|d| old.get(&d.key()).is_some_and(|previous| *previous != *d))
                        .count(),
                    removed: old.keys().filter(|key| !keys.contains(*key)).count(),
                };
                next.sort_by_key(Device::key);
                next.dedup_by(|a, b| a.key() == b.key());
                self.devices = next;
                self.last_good = Some(Instant::now());
                self.error = None;
            }
            Err(error) => self.error = Some(error), // Retain the last successful snapshot.
        }
    }
}

// Strict property decoding: malformed metadata becomes unavailable, not a panic.
pub fn decode_strings(bytes: &[u8], multiple: bool) -> Option<Vec<String>> {
    if bytes.len() % 2 != 0 || bytes.is_empty() {
        return None;
    }
    let words: Vec<_> = bytes
        .chunks_exact(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    if words.last() != Some(&0) {
        return None;
    }
    let mut values = Vec::new();
    for part in words.split(|word| *word == 0) {
        if part.is_empty() {
            break;
        }
        values.push(String::from_utf16(part).ok()?);
        if !multiple {
            break;
        }
    }
    Some(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, container: Option<&str>) -> Device {
        Device {
            instance_id: id.into(),
            container_id: container.map(str::to_owned),
            ..Default::default()
        }
    }

    #[test]
    fn composite_grouping_preserves_children_and_identical_products() {
        let mut devices = vec![
            device("USB\\A", Some("abc")),
            device("HID\\A", Some("ABC")),
            device("USB\\B", None),
        ];
        for device in &mut devices {
            device.hardware_ids = vec!["USB\\VID_1234&PID_5678".into()];
        }
        let grouped = groups(&devices);
        assert_eq!(grouped.len(), 2);
        assert_eq!(grouped.iter().map(|g| g.members.len()).sum::<usize>(), 3);
        assert_eq!(devices[0].usb_ids(), Some((0x1234, 0x5678)));
        assert_ne!(devices[0].key(), devices[2].key());
    }

    #[test]
    fn missing_and_zero_containers_do_not_merge() {
        assert_eq!(
            groups(&[
                device("a", None),
                device("b", Some("00000000-0000-0000-0000-000000000000"))
            ])
            .len(),
            2
        );
    }

    #[test]
    fn reconcile_and_error_retention() {
        let mut inventory = Inventory::default();
        inventory.apply(Ok(vec![device("A", None), device("B", None)]));
        let mut updated = device("a", None);
        updated.name = "Updated".into();
        inventory.apply(Ok(vec![updated, device("C", None)]));
        assert_eq!(
            inventory.changes,
            Changes {
                added: 1,
                updated: 1,
                removed: 1
            }
        );
        inventory.apply(Err("Denied".into()));
        assert_eq!(inventory.devices.len(), 2);
        assert!(inventory.last_good.is_some());
        assert!(inventory.error.is_some());
        inventory.apply(Ok(vec![]));
        assert!(inventory.devices.is_empty());
        assert!(inventory.error.is_none());
        assert_eq!(inventory.changes.removed, 2);
    }

    #[test]
    fn search_and_malformed_ids() {
        let mut d = device("USB\\test", None);
        d.name = "Learning Mouse".into();
        d.hardware_ids = vec!["USB\\VID_bad!&PID_5678".into()];
        assert!(d.matches("MOUSE"));
        assert!(d.matches("usb\\test"));
        assert!(!d.matches("headset"));
        assert_eq!(d.usb_ids(), None);
    }

    #[test]
    fn property_decoding_rejects_bad_buffers() {
        assert_eq!(
            decode_strings(&[65, 0, 0, 0], false),
            Some(vec!["A".into()])
        );
        assert_eq!(
            decode_strings(&[65, 0, 0, 0, 66, 0, 0, 0, 0, 0], true),
            Some(vec!["A".into(), "B".into()])
        );
        assert_eq!(decode_strings(&[65], false), None);
        assert_eq!(decode_strings(&[65, 0], false), None);
        assert_eq!(decode_strings(&[0, 216, 0, 0], false), None);
    }
}
