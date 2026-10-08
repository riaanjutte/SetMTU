//! Listing network adapters, reading their MTU, and changing it.
//!
//! Reading uses the IP Helper API (no admin needed). Changing uses
//! `netsh ... store=persistent`, run by an elevated copy of this exe so the
//! user only sees one UAC prompt per click.

use std::mem::zeroed;
use std::os::windows::process::CommandExt;
use std::process::Command;

use windows_sys::Win32::Foundation::{CloseHandle, ERROR_CANCELLED, GetLastError};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    ConvertInterfaceLuidToIndex, FreeMibTable, GetBestInterfaceEx, GetIfTable2,
    GetIpInterfaceEntry, InitializeIpInterfaceEntry, MIB_IF_TABLE2, MIB_IPINTERFACE_ROW,
};
use windows_sys::Win32::NetworkManagement::Ndis::{IfOperStatusUp, NET_LUID_LH};
use windows_sys::Win32::Networking::WinSock::{
    ADDRESS_FAMILY, AF_INET, AF_INET6, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6,
};
use windows_sys::Win32::System::Threading::{GetExitCodeProcess, INFINITE, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    V4,
    V6,
}

impl Family {
    fn af(self) -> ADDRESS_FAMILY {
        match self {
            Family::V4 => AF_INET,
            Family::V6 => AF_INET6,
        }
    }

    pub fn netsh_name(self) -> &'static str {
        match self {
            Family::V4 => "ipv4",
            Family::V6 => "ipv6",
        }
    }

    fn parse(s: &str) -> Option<Self> {
        match s {
            "ipv4" => Some(Family::V4),
            "ipv6" => Some(Family::V6),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Interface {
    pub index: u32,
    pub luid: u64,
    /// Friendly name, e.g. "Ethernet" or "Wi-Fi".
    pub alias: String,
    /// Hardware description, e.g. "Intel(R) Ethernet Controller".
    pub description: String,
    pub mtu_v4: Option<u32>,
    pub mtu_v6: Option<u32>,
    /// Bytes sent + received since the adapter came up (resets on reboot).
    pub traffic: u64,
    pub up: bool,
    /// Windows would route internet traffic through this interface.
    pub default_route: bool,
}

// From MIB_IF_ROW2.InterfaceAndOperStatusFlags (see netioapi.h).
const FLAG_HARDWARE: u8 = 1 << 0;
const FLAG_FILTER: u8 = 1 << 1;
const IF_TYPE_SOFTWARE_LOOPBACK: u32 = 24;

/// Lists the adapters an MTU can sensibly be set on, busiest first.
///
/// Skips loopback, filter-driver shadow entries and anything without an IP
/// stack. Disconnected adapters are kept only if they're real hardware
/// (e.g. Wi-Fi), so virtual ones like Wi-Fi Direct don't clutter the list.
pub fn list_interfaces() -> Vec<Interface> {
    let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
    if unsafe { GetIfTable2(&mut table) } != 0 || table.is_null() {
        return Vec::new();
    }
    let rows = unsafe {
        std::slice::from_raw_parts((*table).Table.as_ptr(), (*table).NumEntries as usize)
    };
    let default_index = best_interface_v4().or_else(best_interface_v6);

    let mut list: Vec<Interface> = rows
        .iter()
        .filter(|row| {
            let flags = row.InterfaceAndOperStatusFlags._bitfield;
            let up = row.OperStatus == IfOperStatusUp;
            row.Type != IF_TYPE_SOFTWARE_LOOPBACK
                && flags & FLAG_FILTER == 0
                && (up || flags & FLAG_HARDWARE != 0)
        })
        .map(|row| Interface {
            index: row.InterfaceIndex,
            luid: unsafe { row.InterfaceLuid.Value },
            alias: wide_to_string(&row.Alias),
            description: wide_to_string(&row.Description),
            mtu_v4: read_mtu(row.InterfaceIndex, Family::V4),
            mtu_v6: read_mtu(row.InterfaceIndex, Family::V6),
            traffic: row.InOctets.saturating_add(row.OutOctets),
            up: row.OperStatus == IfOperStatusUp,
            default_route: Some(row.InterfaceIndex) == default_index,
        })
        .filter(|i| i.mtu_v4.is_some() || i.mtu_v6.is_some())
        .collect();
    unsafe { FreeMibTable(table as *const _) };

    list.sort_by(|a, b| b.traffic.cmp(&a.traffic));
    list
}

/// "1.87 TB", "512 MB", etc. (decimal units, like Windows' adapter status).
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {}", UNITS[unit])
    }
}

/// Looks up the current index of an interface by its LUID (stable across reboots).
pub fn index_from_luid(luid: u64) -> Option<u32> {
    let luid = NET_LUID_LH { Value: luid };
    let mut index = 0u32;
    let rc = unsafe { ConvertInterfaceLuidToIndex(&luid, &mut index) };
    (rc == 0).then_some(index)
}

fn read_mtu(index: u32, family: Family) -> Option<u32> {
    let mut row: MIB_IPINTERFACE_ROW = unsafe { zeroed() };
    unsafe { InitializeIpInterfaceEntry(&mut row) };
    row.Family = family.af();
    row.InterfaceIndex = index;
    let rc = unsafe { GetIpInterfaceEntry(&mut row) };
    (rc == 0).then_some(row.NlMtu)
}

fn best_interface_v4() -> Option<u32> {
    let mut addr: SOCKADDR_IN = unsafe { zeroed() };
    addr.sin_family = AF_INET;
    addr.sin_addr.S_un.S_addr = u32::from_ne_bytes([8, 8, 8, 8]);
    best_interface(&addr as *const _ as *const SOCKADDR)
}

fn best_interface_v6() -> Option<u32> {
    let mut addr: SOCKADDR_IN6 = unsafe { zeroed() };
    addr.sin6_family = AF_INET6;
    // 2001:4860:4860::8888
    addr.sin6_addr.u.Byte = [
        0x20, 0x01, 0x48, 0x60, 0x48, 0x60, 0, 0, 0, 0, 0, 0, 0, 0, 0x88, 0x88,
    ];
    best_interface(&addr as *const _ as *const SOCKADDR)
}

fn best_interface(addr: *const SOCKADDR) -> Option<u32> {
    let mut index = 0u32;
    let rc = unsafe { GetBestInterfaceEx(addr, &mut index) };
    (rc == 0).then_some(index)
}

fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Sets MTUs on an interface, asking for admin rights via UAC.
/// Blocks until the elevated helper finishes, so call it off the UI thread.
pub fn set_mtus(index: u32, changes: &[(Family, u32)]) -> Result<(), String> {
    if changes.is_empty() {
        return Ok(());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut params = format!("--apply {index}");
    for (family, mtu) in changes {
        params.push_str(&format!(" {}={mtu}", family.netsh_name()));
    }

    let verb = to_wide("runas");
    let file = to_wide(&exe.to_string_lossy());
    let params = to_wide(&params);
    let mut info: SHELLEXECUTEINFOW = unsafe { zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params.as_ptr();
    info.nShow = SW_HIDE;

    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let err = unsafe { GetLastError() };
        return Err(if err == ERROR_CANCELLED {
            "Cancelled at the UAC prompt. No change made.".into()
        } else {
            format!("Could not ask Windows for permission (error {err}).")
        });
    }

    if info.hProcess.is_null() {
        return Err("Windows did not start the helper, so nothing was changed.".into());
    }
    let mut code = u32::MAX;
    let got_code = unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        let ok = GetExitCodeProcess(info.hProcess, &mut code);
        CloseHandle(info.hProcess);
        ok != 0
    };
    if !got_code {
        return Err("Could not tell whether the change worked.".into());
    }
    match code {
        0 => Ok(()),
        c => Err(format!("Windows refused the change (netsh exit code {c}).")),
    }
}

/// The `netsh` arguments the helper runs; also shown in the UI as a preview.
pub fn netsh_args(index: u32, family: Family, mtu: u32) -> Vec<String> {
    [
        "interface",
        family.netsh_name(),
        "set",
        "subinterface",
        &index.to_string(),
        &format!("mtu={mtu}"),
        "store=persistent",
    ]
    .map(String::from)
    .to_vec()
}

/// Entry point for the elevated helper: `--apply <index> ipv4=<mtu> [ipv6=<mtu>]`.
/// Returns the process exit code.
pub fn run_helper(args: &[String]) -> i32 {
    let Some(index) = args.first().and_then(|s| s.parse::<u32>().ok()) else {
        return 2;
    };
    for arg in &args[1..] {
        let Some((family, mtu)) = arg.split_once('=') else {
            return 2;
        };
        let (Some(family), Ok(mtu)) = (Family::parse(family), mtu.parse::<u32>()) else {
            return 2;
        };
        let status = Command::new("netsh")
            .args(netsh_args(index, family, mtu))
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => return s.code().unwrap_or(1),
            Err(_) => return 3,
        }
    }
    0
}
