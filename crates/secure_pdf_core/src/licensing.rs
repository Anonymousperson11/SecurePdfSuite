//! Device hardware fingerprinting (`MachineHash`, `MachineCode`) and network adapter inspection.

use std::net::Ipv4Addr;
use std::ptr::null_mut;
use std::time::Duration;

use windows_core::{BSTR, Interface, PCWSTR, PWSTR};

use crate::bindings;
use crate::crypto::{CODE_ALPHABET, sha256};
use crate::error::{CoreError, CoreResult};
use crate::win32_util::{
    ComApartmentGuard, HKeyGuard, to_wide_null, wide_ptr_to_string, wide_slice_to_string,
};

/// Snapshot of local machine identity and network configuration for watermark forensics & quota HMAC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceSnapshot {
    /// Grouped 12-character readable machine code (`XXXX-XXXX-XXXX`).
    pub machine_code: String,
    /// 32-byte SHA-256 hardware fingerprint used as HMAC key in `LocalQuotaStore`.
    pub machine_hash: [u8; 32],
    /// Local host computer name.
    pub computer_name: String,
    /// Selected primary IPv4 address (`127.0.0.1` fallback if no active non-virtual NIC exists).
    pub primary_ipv4: String,
    /// All active non-loopback/non-link-local unicast IPv4 addresses on this machine.
    pub ipv4_list: Vec<String>,
    /// OS `MachineGuid` from HKLM cryptography registry.
    pub machine_guid: String,
    /// BIOS `UUID` queried via WMI `Win32_ComputerSystemProduct`.
    pub bios_uuid: String,
    /// System volume serial number formatted as 8 uppercase hex digits.
    pub volume_serial: String,
}

/// Collects hardware fingerprint components and active IPv4 context from Windows.
///
/// The fingerprint is byte-for-byte compatible with `SecurePdfSuite` v1.x
/// (`DeviceAndNetworkContext.ComputeMachineHash`): it is the HMAC key of every local quota record,
/// so any change to field sources, fallbacks or ordering invalidates existing records.
///
/// # Errors
///
/// Returns [`CoreError::Crypto`] if SHA-256 hashing fails. Hardware identifiers that cannot be
/// read contribute an empty string, exactly like v1.x.
pub fn collect_device_snapshot() -> CoreResult<DeviceSnapshot> {
    let machine_guid = read_machine_guid().unwrap_or_default();
    let bios_uuid = read_bios_uuid_with_retries().unwrap_or_default();
    let volume_serial = read_system_volume_serial().unwrap_or_default();
    let computer_name = netbios_computer_name();

    let machine_hash =
        compute_machine_hash(&machine_guid, &bios_uuid, &volume_serial, &computer_name)?;
    let machine_code = format_machine_code(&machine_hash);
    let (primary_ipv4, ipv4_list) = enumerate_ipv4_addresses();

    Ok(DeviceSnapshot {
        machine_code,
        machine_hash,
        computer_name,
        primary_ipv4,
        ipv4_list,
        machine_guid,
        bios_uuid,
        volume_serial,
    })
}

/// Builds the exact UTF-8 string hashed into the machine fingerprint (v1.x layout).
#[must_use]
pub fn machine_fingerprint_source(
    machine_guid: &str,
    bios_uuid: &str,
    volume_serial: &str,
    computer_name: &str,
) -> String {
    format!("MG={machine_guid}|BIOS={bios_uuid}|VOL={volume_serial}|OS={computer_name}")
}

/// Pure v1.x-compatible machine fingerprint: `SHA-256(UTF-8("MG=..|BIOS=..|VOL=..|OS=.."))`.
///
/// # Errors
///
/// Propagates [`CoreError::Crypto`] if SHA-256 fails.
pub fn compute_machine_hash(
    machine_guid: &str,
    bios_uuid: &str,
    volume_serial: &str,
    computer_name: &str,
) -> CoreResult<[u8; 32]> {
    sha256(
        machine_fingerprint_source(machine_guid, bios_uuid, volume_serial, computer_name)
            .as_bytes(),
    )
}

/// `NetBIOS` name of this computer (identical to .NET `Environment.MachineName`); empty on failure.
#[must_use]
pub fn netbios_computer_name() -> String {
    query_computer_name(bindings::ComputerNameNetBIOS).unwrap_or_default()
}

/// Returns the formatted machine code and raw hash for the current host.
///
/// # Errors
///
/// Propagates [`CoreError::Crypto`] if SHA-256 fails.
pub fn current_machine_identity() -> CoreResult<(String, [u8; 32])> {
    let snap = collect_device_snapshot()?;
    Ok((snap.machine_code, snap.machine_hash))
}

/// Derives the grouped 12-character `XXXX-XXXX-XXXX` code from the first 12 bytes of `machine_hash`.
#[must_use]
pub fn format_machine_code(machine_hash: &[u8; 32]) -> String {
    let mut out = String::with_capacity(14);
    for (idx, &byte) in machine_hash[..12].iter().enumerate() {
        if idx > 0 && idx % 4 == 0 {
            out.push('-');
        }
        out.push(char::from(CODE_ALPHABET[usize::from(byte & 0x1F)]));
    }
    out
}

/// Returns the primary IPv4 address string (`"127.0.0.1"` fallback) and sorted non-virtual candidates.
#[must_use]
pub fn primary_ipv4() -> String {
    enumerate_ipv4_addresses().0
}

/// Enumerates all non-loopback, non-link-local unicast IPv4 addresses alongside the chosen primary.
#[must_use]
pub fn enumerate_ipv4_addresses() -> (String, Vec<String>) {
    let flags = bindings::GAA_FLAG_INCLUDE_GATEWAYS
        | bindings::GAA_FLAG_SKIP_ANYCAST
        | bindings::GAA_FLAG_SKIP_MULTICAST;
    let mut size = 16_384_u32;
    let mut buf: Vec<u8> = Vec::new();

    for _attempt in 0..4 {
        buf.resize(usize::try_from(size).unwrap_or(16_384), 0);
        // SAFETY: `buf` is sized to `size` bytes and properly aligned for reading via pointer cast;
        // allocate aligned via u64 chunks or verify alignment cleanly below.
        let mut aligned = vec![0_u64; buf.len().div_ceil(8)];
        let ptr = aligned
            .as_mut_ptr()
            .cast::<bindings::IP_ADAPTER_ADDRESSES_LH>();
        let family_v4 = u32::try_from(bindings::AF_INET).unwrap_or(2);
        // SAFETY: `GetAdaptersAddresses` writes up to `size` bytes into `aligned`.
        let rc = unsafe {
            bindings::GetAdaptersAddresses(
                family_v4,
                u32::try_from(flags).unwrap_or(0),
                None,
                Some(ptr),
                &raw mut size,
            )
        };
        if rc == 0 {
            return parse_adapter_addresses(ptr);
        }
        if rc != 111 {
            // 111 == ERROR_BUFFER_OVERFLOW
            break;
        }
    }

    ("127.0.0.1".to_owned(), Vec::new())
}

fn parse_adapter_addresses(
    first: *const bindings::IP_ADAPTER_ADDRESSES_LH,
) -> (String, Vec<String>) {
    let mut candidates: Vec<(u32, String)> = Vec::new();
    let mut all_ips: Vec<String> = Vec::new();
    let mut cur = first;

    while !cur.is_null() {
        // SAFETY: `cur` walks a null-terminated Win32 linked list returned by `GetAdaptersAddresses`.
        let adapter = unsafe { &*cur };
        let loopback_if =
            bindings::IFTYPE(u32::try_from(bindings::IF_TYPE_SOFTWARE_LOOPBACK).unwrap_or(24));
        let tunnel_if = bindings::IFTYPE(u32::try_from(bindings::IF_TYPE_TUNNEL).unwrap_or(131));
        if adapter.OperStatus == bindings::IfOperStatusUp
            && adapter.IfType != loopback_if
            && adapter.IfType != tunnel_if
        {
            // SAFETY: `Description` / `FriendlyName` are valid NUL-terminated UTF-16 pointers when non-null.
            let desc = unsafe { wide_ptr_to_string(adapter.Description) }.to_lowercase();
            // SAFETY: Same lifetime guarantee within `aligned` adapter buffer.
            let friendly = unsafe { wide_ptr_to_string(adapter.FriendlyName) }.to_lowercase();
            let has_gateway = !adapter.FirstGatewayAddress.is_null();
            let is_virtual = is_virtual_adapter_name(&desc) || is_virtual_adapter_name(&friendly);
            let score = u32::from(!has_gateway) + 5_u32 * u32::from(is_virtual);

            let mut uc = adapter.FirstUnicastAddress;
            while !uc.is_null() {
                // SAFETY: `uc` walks unicast linked list inside adapter buffer.
                let unicast = unsafe { &*uc };
                if let Some(ip) =
                    extract_ipv4(unicast.Address.lpSockaddr, unicast.Address.iSockaddrLength)
                    && !ip.is_loopback()
                    && !ip.is_link_local()
                {
                    let ip_str = ip.to_string();
                    if !all_ips.contains(&ip_str) {
                        all_ips.push(ip_str.clone());
                    }
                    candidates.push((score, ip_str));
                }
                uc = unicast.Next;
            }
        }
        cur = adapter.Next;
    }

    let primary = candidates
        .iter()
        .min_by_key(|(score, _)| *score)
        .map_or_else(|| "127.0.0.1".to_owned(), |(_, ip)| ip.clone());
    (primary, all_ips)
}

fn extract_ipv4(sa: *const bindings::SOCKADDR, len: i32) -> Option<Ipv4Addr> {
    let min_len = i32::try_from(size_of::<bindings::SOCKADDR_IN>()).ok()?;
    if sa.is_null() || len < min_len {
        return None;
    }
    // SAFETY: Caller verified non-null `sa` with buffer length >= `size_of::<SOCKADDR_IN>()`.
    let family = unsafe { (*sa).sa_family };
    if family != bindings::ADDRESS_FAMILY(u16::try_from(bindings::AF_INET).ok()?) {
        return None;
    }
    // SAFETY: `sa_family == AF_INET` and `len >= size_of::<SOCKADDR_IN>()` guarantee readable `SOCKADDR_IN`.
    let sin = unsafe { sa.cast::<bindings::SOCKADDR_IN>().read_unaligned() };
    // SAFETY: Reading byte-aligned union view `S_un_b` from initialized `IN_ADDR`.
    let b = unsafe { sin.sin_addr.S_un.S_un_b };
    Some(Ipv4Addr::new(b.s_b1, b.s_b2, b.s_b3, b.s_b4))
}

fn is_virtual_adapter_name(lower: &str) -> bool {
    [
        "virtual",
        "wsl",
        "hyper-v",
        "vmware",
        "virtualbox",
        "vethernet",
        "tap",
        "tun",
        "wireguard",
        "tailscale",
        "zerotier",
        "nordlynx",
        "loopback",
    ]
    .iter()
    .any(|kw| lower.contains(kw))
}

fn read_machine_guid() -> Option<String> {
    let subkey = to_wide_null(r"SOFTWARE\Microsoft\Cryptography");
    let value_name = to_wide_null("MachineGuid");
    let mut raw_hkey = bindings::HKEY(null_mut());
    let access =
        u32::try_from(bindings::KEY_READ | bindings::KEY_WOW64_64KEY).unwrap_or(0x0002_0119);
    // SAFETY: Opens read-only HKLM Cryptography subkey into `raw_hkey`.
    let rc = unsafe {
        bindings::RegOpenKeyExW(
            bindings::HKEY_LOCAL_MACHINE,
            PCWSTR(subkey.as_ptr()),
            None,
            bindings::ACCESS_MASK(access),
            &raw mut raw_hkey,
        )
    };
    if rc.0 != 0 {
        return None;
    }
    let hkey = HKeyGuard(raw_hkey);

    let mut val_type = 0_u32;
    let mut byte_len = 0_u32;
    // SAFETY: First query gets required buffer byte length for `MachineGuid`.
    let rc_size = unsafe {
        bindings::RegQueryValueExW(
            hkey.0,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&raw mut val_type),
            None,
            &raw mut byte_len,
        )
    };
    if rc_size.0 != 0 || byte_len < 2 || val_type != bindings::REG_SZ {
        return None;
    }

    let mut data = vec![0_u8; usize::try_from(byte_len).ok()?];
    // SAFETY: Second query reads `MachineGuid` string bytes into `data`.
    let rc_val = unsafe {
        bindings::RegQueryValueExW(
            hkey.0,
            PCWSTR(value_name.as_ptr()),
            None,
            Some(&raw mut val_type),
            Some(data.as_mut_ptr()),
            &raw mut byte_len,
        )
    };
    if rc_val.0 != 0 || val_type != bindings::REG_SZ {
        return None;
    }

    let actual_len = usize::try_from(byte_len).ok()?.min(data.len());
    let units: Vec<u16> = data[..actual_len]
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&c| u16::from_le_bytes(c))
        .collect();
    // v1.x used the registry string verbatim (no trimming).
    let value = wide_slice_to_string(&units);
    if value.is_empty() { None } else { Some(value) }
}

fn read_system_volume_serial() -> Option<String> {
    let mut win_dir = vec![0_u16; 260];
    let cap = u32::try_from(win_dir.len()).unwrap_or(260);
    // SAFETY: Queries Windows directory path into `win_dir`.
    let len = unsafe { bindings::GetWindowsDirectoryW(Some(PWSTR(win_dir.as_mut_ptr())), cap) };
    let root_str = if len > 0 {
        let s = wide_slice_to_string(&win_dir[..usize::try_from(len).ok()?.min(win_dir.len())]);
        let drive = s.chars().take(2).collect::<String>();
        if drive.len() == 2 && drive.ends_with(':') {
            format!("{drive}\\")
        } else {
            "C:\\".to_owned()
        }
    } else {
        "C:\\".to_owned()
    };

    let root_wide = to_wide_null(&root_str);
    let mut serial = 0_u32;
    // SAFETY: `root_wide` is NUL-terminated drive root path; only `lpvolumeserialnumber` is queried.
    let ok = unsafe {
        bindings::GetVolumeInformationW(
            PCWSTR(root_wide.as_ptr()),
            None,
            0,
            Some(&raw mut serial),
            None,
            None,
            None,
            0,
        )
    };
    if ok.as_bool() {
        Some(format!("{serial:08X}"))
    } else {
        None
    }
}

fn query_computer_name(kind: bindings::COMPUTER_NAME_FORMAT) -> Option<String> {
    let mut size = 0_u32;
    // SAFETY: Probes required wide buffer size for `GetComputerNameExW`.
    let _ = unsafe { bindings::GetComputerNameExW(kind, None, &raw mut size) };
    if size == 0 {
        return None;
    }
    let mut buf = vec![0_u16; usize::try_from(size).ok()?];
    // SAFETY: `buf` is sized to `size` wide chars per the size probe above.
    let ok =
        unsafe { bindings::GetComputerNameExW(kind, Some(PWSTR(buf.as_mut_ptr())), &raw mut size) };
    if !ok.as_bool() {
        return None;
    }
    let name = wide_slice_to_string(&buf);
    if name.is_empty() { None } else { Some(name) }
}

fn read_bios_uuid_with_retries() -> Option<String> {
    for attempt in 0..3 {
        match query_bios_uuid_once() {
            // v1.x: a successful query ends the retries; a blank UUID yields the empty fallback
            // and a non-blank one is used verbatim (no trimming).
            Ok(uuid) => return (!uuid.trim().is_empty()).then_some(uuid),
            Err(_) if attempt < 2 => std::thread::sleep(Duration::from_millis(150)),
            Err(_) => {}
        }
    }
    None
}

/// Applies the NTLM / call-level / impersonate security blanket required by local WMI proxies.
///
/// The blanket is per interface proxy, so it must be set on the very interface pointer that will
/// be called (a `QueryInterface` to `IUnknown` would configure a different proxy and leave WMI
/// calls at the default identify-only level, which fails with `WBEM_E_ACCESS_DENIED`).
fn set_wmi_proxy_blanket<T: Interface>(proxy: &T) -> CoreResult<()> {
    let raw = proxy.as_raw();
    // SAFETY: `raw` is the live COM interface pointer owned by `proxy`; every COM interface
    // starts with the `IUnknown` vtable, and the borrow does not outlive `proxy`.
    let Some(as_unknown) = (unsafe { windows_core::IUnknown::from_raw_borrowed(&raw) }) else {
        return Err(CoreError::Context("WMI 代理指针为空。".to_owned()));
    };
    // SAFETY: Configures the impersonation blanket on a live WMI COM proxy for local queries.
    unsafe {
        bindings::CoSetProxyBlanket(
            as_unknown,
            u32::try_from(bindings::RPC_C_AUTHN_WINNT).unwrap_or(10),
            u32::try_from(bindings::RPC_C_AUTHZ_NONE).unwrap_or(0),
            None,
            u32::try_from(bindings::RPC_C_AUTHN_LEVEL_CALL).unwrap_or(3),
            u32::try_from(bindings::RPC_C_IMP_LEVEL_IMPERSONATE).unwrap_or(3),
            None,
            u32::try_from(bindings::EOAC_NONE).unwrap_or(0),
        )
    }
    .ok()
    .map_err(|e| CoreError::Context(format!("WMI CoSetProxyBlanket 失败：{e}")))
}

fn query_bios_uuid_once() -> CoreResult<String> {
    let _com = ComApartmentGuard::init(u32::try_from(bindings::COINIT_MULTITHREADED).unwrap_or(0))?;
    // SAFETY: Creates in-process WMI locator COM object.
    let locator: bindings::IWbemLocator = unsafe {
        bindings::CoCreateInstance(&bindings::WbemLocator, None, bindings::CLSCTX_INPROC_SERVER)
    }
    .map_err(|e| CoreError::Context(format!("WbemLocator 创建失败：{e}")))?;

    let ns = BSTR::from(r"root\CIMV2");
    let empty_bstr = BSTR::new();
    // SAFETY: Connects to local WMI namespace `root\CIMV2` with default user credentials.
    let services = unsafe {
        locator.ConnectServer(
            &ns,
            &empty_bstr,
            &empty_bstr,
            &empty_bstr,
            0,
            &empty_bstr,
            None,
        )
    }
    .map_err(|e| CoreError::Context(format!("WMI ConnectServer 失败：{e}")))?;

    set_wmi_proxy_blanket(&services)?;

    let wql = BSTR::from("WQL");
    let query = BSTR::from("SELECT UUID FROM Win32_ComputerSystemProduct");
    let qflags = bindings::WBEM_FLAG_FORWARD_ONLY | bindings::WBEM_FLAG_RETURN_IMMEDIATELY;
    // SAFETY: Executes read-only forward-only WQL query against connected local WMI services.
    let enumerator = unsafe { services.ExecQuery(&wql, &query, qflags, None) }
        .map_err(|e| CoreError::Context(format!("WMI ExecQuery 失败：{e}")))?;

    // Without process-wide `CoInitializeSecurity`, the enumerator proxy keeps the default
    // (identify-only) blanket and `Next` fails with `WBEM_E_ACCESS_DENIED`; give it the same
    // impersonation blanket as the services proxy.
    set_wmi_proxy_blanket(&enumerator)?;

    let mut obj: Option<bindings::IWbemClassObject> = None;
    let mut returned = 0_u32;
    // SAFETY: Retrieves first row from `IEnumWbemClassObject`.
    let hr =
        unsafe { enumerator.Next(bindings::WBEM_INFINITE, 1, &raw mut obj, &raw mut returned) };
    if hr.is_err() {
        return Err(CoreError::Context(format!(
            "WMI 枚举 BIOS UUID 失败：0x{:08X}",
            hr.0.cast_unsigned()
        )));
    }
    if returned == 0 {
        return Ok(String::new());
    }
    let Some(obj) = obj else {
        return Ok(String::new());
    };

    let prop_name = to_wide_null("UUID");
    let mut variant = bindings::VARIANT::default();
    let mut cim_type = bindings::CIMTYPE(0);
    let mut flavor = 0_i32;
    // SAFETY: Reads `UUID` property from WMI object into zeroed `VARIANT`.
    let get_hr = unsafe {
        obj.Get(
            PCWSTR(prop_name.as_ptr()),
            0,
            &raw mut variant,
            &raw mut cim_type,
            &raw mut flavor,
        )
    };
    let uuid_str = if get_hr.is_ok() {
        // SAFETY: Reads `vt` discriminant from initialized `VARIANT` union.
        let vt = unsafe { variant.Anonymous.Anonymous.vt };
        let vt_bstr = bindings::VARTYPE(u16::try_from(bindings::VT_BSTR).unwrap_or(8));
        if vt == vt_bstr {
            // SAFETY: `variant.Anonymous.Anonymous` is active because `VARIANT` union holds `VARIANT_0_0`.
            let inner = unsafe { &variant.Anonymous.Anonymous };
            // SAFETY: `vt == VT_BSTR` guarantees `bstrVal` inside `inner.Anonymous` is a valid `BSTR`.
            let bstr = unsafe { &*inner.Anonymous.bstrVal };
            String::from_utf16_lossy(bstr)
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    // SAFETY: Always releases resources held by `variant` via `VariantClear`.
    unsafe {
        let _ = bindings::VariantClear(&raw mut variant);
    }

    Ok(uuid_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machine_code_uses_32_char_alphabet_and_4_char_groups() {
        let hash = [0xAA_u8; 32];
        let code = format_machine_code(&hash);
        assert_eq!(code.len(), 14);
        assert_eq!(code.chars().nth(4), Some('-'));
        assert_eq!(code.chars().nth(9), Some('-'));
        for ch in code.chars().filter(|&c| c != '-') {
            assert!(CODE_ALPHABET.contains(&(ch as u8)));
        }
    }

    #[test]
    fn device_snapshot_collects_non_empty_fingerprint_on_windows() {
        let snap = collect_device_snapshot().unwrap();
        assert_eq!(snap.machine_code.len(), 14);
        assert_ne!(snap.computer_name, "");
        assert_ne!(snap.primary_ipv4, "");
        assert_ne!(snap.machine_guid, "");
        assert_eq!(snap.volume_serial.len(), 8);
        // The WMI query itself must succeed (a blank UUID is only legitimate on odd firmware).
        assert!(query_bios_uuid_once().is_ok());
        // NetBIOS names are upper-case and at most 15 characters (Environment.MachineName).
        assert!(snap.computer_name.chars().count() <= 15);
        assert_eq!(snap.computer_name, snap.computer_name.to_uppercase());
        assert_eq!(
            snap.machine_hash,
            compute_machine_hash(
                &snap.machine_guid,
                &snap.bios_uuid,
                &snap.volume_serial,
                &snap.computer_name
            )
            .unwrap()
        );
    }

    /// Prints this machine's identity so it can be compared with the code shown by v1.x.
    #[test]
    #[ignore = "diagnostic; run manually with --ignored --nocapture"]
    fn print_current_machine_identity() {
        let snap = collect_device_snapshot().unwrap();
        println!("MACHINE_CODE={}", snap.machine_code);
        println!("MACHINE_HASH={}", hex(&snap.machine_hash));
        println!(
            "FIELD_LENGTHS guid={} bios={} volume={} name={}",
            snap.machine_guid.len(),
            snap.bios_uuid.len(),
            snap.volume_serial.len(),
            snap.computer_name.len()
        );
        println!("BIOS_QUERY={:?}", query_bios_uuid_once().map(|u| u.len()));
    }

    fn hex(bytes: &[u8]) -> String {
        use std::fmt::Write as _;
        bytes.iter().fold(String::new(), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
    }

    // Known answers below were produced independently of this crate with
    // `hashlib.sha256(...)` over the v1.x format string
    // "MG={guid}|BIOS={uuid}|VOL={serial}|OS={Environment.MachineName}" and the v1.x
    // `FormatMachineCode` algorithm (alphabet index = byte & 0x1F, first 12 bytes).
    #[test]
    fn machine_hash_matches_v1_known_answer_for_fixed_tuple() {
        let source = machine_fingerprint_source(
            "8f2c1d4e-5b6a-4c3d-9e8f-0a1b2c3d4e5f",
            "4C4C4544-0042-3010-8054-B7C04F4D5332",
            "1A2B3C4D",
            "WORKSTATION-A01",
        );
        assert_eq!(
            source,
            "MG=8f2c1d4e-5b6a-4c3d-9e8f-0a1b2c3d4e5f|BIOS=4C4C4544-0042-3010-8054-B7C04F4D5332|VOL=1A2B3C4D|OS=WORKSTATION-A01"
        );
        let hash = compute_machine_hash(
            "8f2c1d4e-5b6a-4c3d-9e8f-0a1b2c3d4e5f",
            "4C4C4544-0042-3010-8054-B7C04F4D5332",
            "1A2B3C4D",
            "WORKSTATION-A01",
        )
        .unwrap();
        assert_eq!(
            hex(&hash),
            "be233190c69381adbf9ad32336cb20c826327e31cf8d7756d04d5bd9ef89a04a"
        );
        assert_eq!(format_machine_code(&hash), "8DTS-GVBP-94VD");
    }

    #[test]
    fn machine_hash_uses_empty_strings_for_unreadable_fields_like_v1() {
        let hash = compute_machine_hash("", "", "", "PC").unwrap();
        assert_eq!(
            hex(&hash),
            "ed532c2ac883b2794bea1611ef8ffb91dc398aa4768b9ac6e7b4586536b7ea8b"
        );
        assert_eq!(format_machine_code(&hash), "PVNL-JDU3-MLYT");
    }
}
