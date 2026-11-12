mod devices;
#[cfg(windows)]
mod windows_devices;

use devices::{groups, Inventory};
use serde_json::{json, Value};
use std::io::{self, BufRead, Write};

fn snapshot(inventory: &Inventory) -> Value {
    let groups: Vec<_> = groups(&inventory.devices)
        .into_iter()
        .map(|group| {
            let members: Vec<_> = group.members.into_iter().map(|device| {
                json!({
                    "instance_id": device.instance_id,
                    "container_id": device.container_id,
                    "name": device.title(),
                    "manufacturer": device.manufacturer,
                    "class": device.class,
                    "enumerator": device.enumerator,
                    "parent": device.parent,
                    "hardware_ids": device.hardware_ids,
                    "usb_ids": device.usb_ids(),
                })
            }).collect();
            json!({"key": group.key, "members": members})
        })
        .collect();
    json!({
        "groups": groups,
        "recordCount": inventory.devices.len(),
        "hasSnapshot": inventory.last_good.is_some(),
        "error": inventory.error,
        "changes": {
            "added": inventory.changes.added,
            "updated": inventory.changes.updated,
            "removed": inventory.changes.removed,
        },
    })
}

fn main() -> io::Result<()> {
    let mut inventory = Inventory::default();
    let mut output = io::BufWriter::new(io::stdout().lock());
    for line in io::stdin().lock().lines() {
        if line? != "scan" {
            eprintln!("Unsupported request; expected scan");
            return Err(io::Error::new(io::ErrorKind::InvalidInput, "Invalid request"));
        }
        #[cfg(windows)]
        let scan = windows_devices::enumerate();
        #[cfg(not(windows))]
        let scan = Err("Device enumeration currently supports Windows only.".into());
        inventory.apply(scan);
        serde_json::to_writer(&mut output, &snapshot(&inventory))?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_retains_snapshot_after_failure() {
        let mut inventory = Inventory::default();
        inventory.apply(Ok(vec![devices::Device {
            instance_id: "USB\\example".into(),
            name: "Device \"name\"\n".into(),
            ..Default::default()
        }]));
        inventory.apply(Err("Scan failed".into()));
        let encoded = serde_json::to_string(&snapshot(&inventory)).unwrap();
        let result: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(result["recordCount"], 1);
        assert_eq!(result["hasSnapshot"], true);
        assert_eq!(result["error"], "Scan failed");
        assert_eq!(result["groups"][0]["members"][0]["name"], "Device \"name\"\n");
    }
}
