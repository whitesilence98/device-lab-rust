//! OS metadata only. Never opens a device endpoint or sends a hardware command.
use crate::devices::{decode_strings, Device};
use std::{
    mem::size_of,
    ptr::{null, null_mut},
};
use windows_sys::Win32::{
    Devices::{DeviceAndDriverInstallation::*, Properties::*},
    Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER, ERROR_NO_MORE_ITEMS},
};

const MAX_PROPERTY_BYTES: usize = 1024 * 1024;

struct DeviceSet(HDEVINFO);
impl Drop for DeviceSet {
    fn drop(&mut self) {
        // SAFETY: this wrapper owns exactly one valid SetupAPI information-set handle.
        unsafe {
            SetupDiDestroyDeviceInfoList(self.0);
        }
    }
}

fn error(operation: &str, code: u32) -> String {
    format!(
        "{operation}: {}",
        std::io::Error::from_raw_os_error(code as i32)
    )
}

fn instance_id(set: &DeviceSet, device: &SP_DEVINFO_DATA) -> Result<String, String> {
    let mut needed = 0;
    // SAFETY: sizing call; valid set/device, null output with zero length, writable size.
    let ok = unsafe { SetupDiGetDeviceInstanceIdW(set.0, device, null_mut(), 0, &mut needed) };
    // SAFETY: called immediately after the Win32 operation on the same thread.
    let code = unsafe { GetLastError() };
    if ok == 0 && code != ERROR_INSUFFICIENT_BUFFER {
        return Err(error("Device identity", code));
    }
    for _ in 0..3 {
        if needed == 0 || needed as usize > MAX_PROPERTY_BYTES / 2 {
            return Err("Invalid device identity size".into());
        }
        let mut words = vec![0u16; needed as usize];
        // SAFETY: words owns at least needed UTF-16 units; all pointers remain valid.
        let ok = unsafe {
            SetupDiGetDeviceInstanceIdW(
                set.0,
                device,
                words.as_mut_ptr(),
                words.len() as u32,
                &mut needed,
            )
        };
        if ok != 0 {
            let end = words
                .iter()
                .position(|&w| w == 0)
                .ok_or("Unterminated device identity")?;
            if end == 0 {
                return Err("Empty device identity".into());
            }
            return String::from_utf16(&words[..end])
                .map_err(|_| "Invalid UTF-16 device identity".into());
        }
        // SAFETY: immediately following the failed call, before other Win32 calls.
        let code = unsafe { GetLastError() };
        if code != ERROR_INSUFFICIENT_BUFFER {
            return Err(error("Device identity", code));
        }
    }
    Err("Device identity changed repeatedly during enumeration; refresh again".into())
}

fn registry_property(
    set: &DeviceSet,
    device: &SP_DEVINFO_DATA,
    property: u32,
) -> Option<Vec<String>> {
    let mut needed = 0;
    let mut kind = 0;
    // SAFETY: valid metadata set/device, null sizing buffer, initialized writable outputs.
    unsafe {
        SetupDiGetDeviceRegistryPropertyW(
            set.0,
            device,
            property,
            &mut kind,
            null_mut(),
            0,
            &mut needed,
        );
    }
    // SAFETY: immediately reads the preceding sizing call's error.
    if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return None;
    }
    for _ in 0..3 {
        if needed == 0 || needed as usize > MAX_PROPERTY_BYTES {
            return None;
        }
        let mut bytes = vec![0u8; needed as usize];
        // SAFETY: buffer capacity matches supplied byte count; set/device are valid.
        let ok = unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set.0,
                device,
                property,
                &mut kind,
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                &mut needed,
            )
        };
        if ok != 0 {
            if needed as usize > bytes.len() || ![1, 7].contains(&kind) {
                return None;
            } // REG_SZ / REG_MULTI_SZ
            return decode_strings(&bytes[..needed as usize], kind == 7);
        }
        // SAFETY: reads the immediately preceding failed API operation's error.
        if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
    }
    None
}

fn property(set: &DeviceSet, device: &SP_DEVINFO_DATA, key: &DEVPROPKEY) -> Option<(u32, Vec<u8>)> {
    let mut needed = 0;
    let mut kind = 0;
    // SAFETY: metadata-only sizing call with valid key/device and writable outputs.
    unsafe {
        SetupDiGetDevicePropertyW(set.0, device, key, &mut kind, null_mut(), 0, &mut needed, 0);
    }
    // SAFETY: reads last error immediately following the sizing call.
    if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return None;
    }
    for _ in 0..3 {
        if needed == 0 || needed as usize > MAX_PROPERTY_BYTES {
            return None;
        }
        let mut bytes = vec![0u8; needed as usize];
        // SAFETY: buffer has the supplied capacity; all metadata pointers remain valid.
        let ok = unsafe {
            SetupDiGetDevicePropertyW(
                set.0,
                device,
                key,
                &mut kind,
                bytes.as_mut_ptr(),
                bytes.len() as u32,
                &mut needed,
                0,
            )
        };
        if ok != 0 {
            if needed as usize > bytes.len() {
                return None;
            }
            bytes.truncate(needed as usize);
            return Some((kind, bytes));
        }
        // SAFETY: immediately reads the failing API call's error on this thread.
        if unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
            return None;
        }
    }
    None
}

fn container(set: &DeviceSet, device: &SP_DEVINFO_DATA) -> Option<String> {
    let (kind, b) = property(set, device, &DEVPKEY_Device_ContainerId)?;
    if kind != DEVPROP_TYPE_GUID || b.len() != 16 || b.iter().all(|v| *v == 0) {
        return None;
    }
    Some(format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        u32::from_le_bytes(b[0..4].try_into().ok()?),
        u16::from_le_bytes(b[4..6].try_into().ok()?),
        u16::from_le_bytes(b[6..8].try_into().ok()?),
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    ))
}

pub fn enumerate() -> Result<Vec<Device>, String> {
    // SAFETY: null filters select every present class; this creates a metadata set only.
    let handle = unsafe {
        SetupDiGetClassDevsW(null(), null(), null_mut(), DIGCF_ALLCLASSES | DIGCF_PRESENT)
    };
    if handle == -1 {
        // SAFETY: immediately reads the failed SetupDiGetClassDevsW error.
        return Err(error("Enumerating Windows devices", unsafe {
            GetLastError()
        }));
    }
    let set = DeviceSet(handle);
    let mut devices = Vec::new();
    for index in 0u32.. {
        // SAFETY: SP_DEVINFO_DATA is a C structure of integers and GUID; zero is valid initialization.
        let mut info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
        info.cbSize = size_of::<SP_DEVINFO_DATA>() as u32;
        // SAFETY: valid information-set handle and correctly sized writable structure.
        if unsafe { SetupDiEnumDeviceInfo(set.0, index, &mut info) } == 0 {
            // SAFETY: immediately follows the failing enumeration operation.
            let code = unsafe { GetLastError() };
            if code == ERROR_NO_MORE_ITEMS {
                return Ok(devices);
            }
            return Err(error("Reading Windows device list", code));
        }
        let text = |key| {
            registry_property(&set, &info, key)
                .and_then(|values| values.into_iter().next())
                .unwrap_or_default()
        };
        let friendly = text(SPDRP_FRIENDLYNAME);
        let parent = property(&set, &info, &DEVPKEY_Device_Parent)
            .filter(|(kind, _)| *kind == DEVPROP_TYPE_STRING)
            .and_then(|(_, bytes)| decode_strings(&bytes, false))
            .and_then(|values| values.into_iter().next())
            .unwrap_or_default();
        devices.push(Device {
            // Identity failure invalidates the snapshot: do not create phantom removals.
            instance_id: instance_id(&set, &info)?,
            container_id: container(&set, &info),
            name: if friendly.is_empty() {
                text(SPDRP_DEVICEDESC)
            } else {
                friendly
            },
            manufacturer: text(SPDRP_MFG),
            class: text(SPDRP_CLASS),
            enumerator: text(SPDRP_ENUMERATOR_NAME),
            parent,
            hardware_ids: registry_property(&set, &info, SPDRP_HARDWAREID).unwrap_or_default(),
        });
    }
    Err("Device index exceeded supported range".into())
}
