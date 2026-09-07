//! Windows startup recovery holds the inspected process handle across user consent.
//! No PID/name-wide kill, data deletion, or automatic takeover of a CLI server.

#[cfg(windows)]
pub(crate) mod windows {
    use anyhow::{Context, bail};
    use std::{path::PathBuf, ptr};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, ERROR_INSUFFICIENT_BUFFER, HANDLE, WAIT_OBJECT_0},
        NetworkManagement::IpHelper::{
            GetExtendedTcpTable, MIB_TCPROW_OWNER_PID, MIB_TCPTABLE_OWNER_PID,
            TCP_TABLE_OWNER_PID_LISTENER,
        },
        Networking::WinSock::AF_INET,
        System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
            QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject,
        },
    };

    pub(crate) struct Occupant {
        handle: HANDLE,
        pub pid: u32,
        pub image: PathBuf,
    }

    impl Drop for Occupant {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.handle) };
        }
    }

    pub(crate) fn listener_pid(port: u16) -> anyhow::Result<Option<u32>> {
        // u32 storage gives the alignment required by the Windows table/rows.
        let mut buffer = vec![0u32; 256];
        loop {
            let mut size = (buffer.len() * 4) as u32;
            let status = unsafe {
                GetExtendedTcpTable(
                    buffer.as_mut_ptr().cast(),
                    &mut size,
                    0,
                    AF_INET as u32,
                    TCP_TABLE_OWNER_PID_LISTENER,
                    0,
                )
            };
            if status == ERROR_INSUFFICIENT_BUFFER {
                buffer.resize((size as usize).div_ceil(4), 0);
                continue;
            }
            if status != 0 {
                return Err(std::io::Error::from_raw_os_error(status as i32).into());
            }
            let table = buffer.as_ptr().cast::<MIB_TCPTABLE_OWNER_PID>();
            let count = unsafe { (*table).dwNumEntries } as usize;
            let offset = std::mem::offset_of!(MIB_TCPTABLE_OWNER_PID, table);
            anyhow::ensure!(
                offset + count * std::mem::size_of::<MIB_TCPROW_OWNER_PID>() <= size as usize,
                "invalid Windows listener table"
            );
            let rows = unsafe {
                std::slice::from_raw_parts(
                    ptr::addr_of!((*table).table).cast::<MIB_TCPROW_OWNER_PID>(),
                    count,
                )
            };
            let mut owners = rows
                .iter()
                .filter(|row| {
                    u16::from_be(row.dwLocalPort as u16) == port
                        && (row.dwLocalAddr == 0 || row.dwLocalAddr.to_ne_bytes() == [127, 0, 0, 1])
                })
                .map(|row| row.dwOwningPid)
                .collect::<Vec<_>>();
            owners.sort_unstable();
            owners.dedup();
            anyhow::ensure!(
                owners.len() <= 1,
                "multiple processes own port {port}; automatic cleanup is unavailable"
            );
            return Ok(owners.first().copied());
        }
    }

    pub(crate) fn inspect(port: u16) -> anyhow::Result<Option<Occupant>> {
        let Some(pid) = listener_pid(port)? else {
            return Ok(None);
        };
        anyhow::ensure!(
            pid != std::process::id(),
            "this desktop process already owns port {port}"
        );
        let handle = unsafe {
            OpenProcess(
                PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE | PROCESS_TERMINATE,
                0,
                pid,
            )
        };
        anyhow::ensure!(
            !handle.is_null(),
            "cannot inspect port {port} owner PID {pid}: {}",
            std::io::Error::last_os_error()
        );
        let mut occupant = Occupant {
            handle,
            pid,
            image: PathBuf::new(),
        };
        let mut image = vec![0u16; 32768];
        let mut length = image.len() as u32;
        if unsafe { QueryFullProcessImageNameW(handle, 0, image.as_mut_ptr(), &mut length) } == 0 {
            return Err(std::io::Error::last_os_error()).context("cannot read process executable");
        }
        use std::os::windows::ffi::OsStringExt;
        occupant.image = std::ffi::OsString::from_wide(&image[..length as usize]).into();
        // Recheck after acquiring the handle: a PID alone is never our authority.
        anyhow::ensure!(
            listener_pid(port)? == Some(pid)
                && unsafe { WaitForSingleObject(handle, 0) } != WAIT_OBJECT_0,
            "port owner changed during inspection; launch again to recheck"
        );
        Ok(Some(occupant))
    }

    impl Occupant {
        pub(crate) fn is_cli(&self) -> bool {
            self.image.file_name().is_some_and(|name| {
                name.to_string_lossy()
                    .eq_ignore_ascii_case("ocg-manager-cli.exe")
            })
        }

        pub(crate) fn stop(self, port: u16) -> anyhow::Result<()> {
            anyhow::ensure!(
                self.is_cli(),
                "cleanup is limited to the identified OCG CLI"
            );
            if unsafe { WaitForSingleObject(self.handle, 0) } == WAIT_OBJECT_0 {
                return Ok(());
            }
            anyhow::ensure!(
                listener_pid(port)? == Some(self.pid),
                "port owner changed; cleanup cancelled"
            );
            if unsafe { TerminateProcess(self.handle, 1) } == 0 {
                return Err(std::io::Error::last_os_error())
                    .context("cannot stop the old OCG process");
            }
            if unsafe { WaitForSingleObject(self.handle, 5000) } != WAIT_OBJECT_0 {
                bail!("old OCG process has not exited; retry after it stops");
            }
            Ok(())
        }
    }
}

/// True means cleanup succeeded; the caller must reload state before retrying.
pub(crate) fn prepare(core: &ocg_core::state::CoreState) -> anyhow::Result<bool> {
    let port = core.settings_config().gateway_port;
    // Probe before publishing application state. The actual listener is bound
    // only after tray creation; a late port race still produces a visible error.
    let first_error = match std::net::TcpListener::bind(("127.0.0.1", port)) {
        Ok(listener) => {
            drop(listener);
            return Ok(false);
        }
        Err(error) => error,
    };
    if first_error.kind() != std::io::ErrorKind::AddrInUse {
        return Err(first_error.into());
    }
    #[cfg(windows)]
    {
        if let Some(occupant) = windows::inspect(port)? {
            if occupant.is_cli() {
                if crate::startup_ui::confirm_cleanup(port, occupant.pid, &occupant.image) {
                    occupant.stop(port)?;
                    return Ok(true);
                }
            } else {
                anyhow::bail!(
                    "Port {port} is occupied by PID {} ({}). Close that application and reopen Open Console Gateway. No process was stopped.",
                    occupant.pid,
                    occupant.image.display()
                );
            }
        }
    }
    Err(first_error.into())
}

#[cfg(all(test, windows))]
mod tests;
