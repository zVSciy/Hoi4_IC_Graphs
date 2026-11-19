//! Win32 process enumeration and `LoadLibraryW` injection.

use std::{ffi::c_void, io, os::windows::ffi::OsStrExt, path::Path, ptr};

use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_BAD_LENGTH, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0},
    System::{
        Diagnostics::{
            Debug::WriteProcessMemory,
            ToolHelp::{
                CreateToolhelp32Snapshot, MODULEENTRY32W, Module32FirstW, Module32NextW,
                PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPMODULE,
                TH32CS_SNAPMODULE32, TH32CS_SNAPPROCESS,
            },
        },
        LibraryLoader::{GetModuleHandleW, GetProcAddress},
        Memory::{
            MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAllocEx, VirtualFreeEx,
        },
        Threading::{
            CreateRemoteThread, GetExitCodeThread, OpenProcess, PROCESS_CREATE_THREAD,
            PROCESS_NAME_WIN32, PROCESS_QUERY_INFORMATION, PROCESS_QUERY_LIMITED_INFORMATION,
            PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE, QueryFullProcessImageNameW,
            WaitForSingleObject,
        },
    },
};

use super::InjectError;

/// How long to wait for `LoadLibraryW` to return inside the game.
const LOAD_TIMEOUT_MS: u32 = 60_000;

/// Closes the wrapped handle on drop.
struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn new(handle: HANDLE) -> io::Result<Self> {
        if handle.is_null() || handle == INVALID_HANDLE_VALUE {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: we own a valid handle.
        unsafe { CloseHandle(self.0) };
    }
}

/// Memory allocated inside another process, released on drop.
struct RemoteAlloc<'a> {
    process: &'a OwnedHandle,
    address: *mut c_void,
}

impl Drop for RemoteAlloc<'_> {
    fn drop(&mut self) {
        // SAFETY: address came from VirtualAllocEx on this process.
        unsafe { VirtualFreeEx(self.process.0, self.address, 0, MEM_RELEASE) };
    }
}

fn wide_eq_ignore_case(wide: &[u16], s: &str) -> bool {
    let len = wide.iter().position(|&c| c == 0).unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..len]).eq_ignore_ascii_case(s)
}

fn io_err(context: &'static str) -> impl FnOnce(io::Error) -> InjectError {
    move |source| InjectError::Io { context, source }
}

/// Full path of a running process's executable.
pub fn process_image_path(pid: u32) -> Option<std::path::PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    // SAFETY: the buffer and its length are passed together; the handle is owned.
    unsafe {
        let process =
            OwnedHandle::new(OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid)).ok()?;
        let mut buffer = [0u16; 1024];
        let mut len = buffer.len() as u32;
        if QueryFullProcessImageNameW(process.0, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut len)
            == 0
        {
            return None;
        }
        Some(std::ffi::OsString::from_wide(&buffer[..len as usize]).into())
    }
}

/// PID of the first process whose executable name matches (case-insensitive).
pub fn find_process(name: &str) -> Option<u32> {
    // SAFETY: plain Toolhelp32 enumeration with correctly sized structs.
    unsafe {
        let snapshot = OwnedHandle::new(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)).ok()?;
        let mut entry = PROCESSENTRY32W {
            dwSize: size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Process32FirstW(snapshot.0, &mut entry);
        while ok != 0 {
            if wide_eq_ignore_case(&entry.szExeFile, name) {
                return Some(entry.th32ProcessID);
            }
            ok = Process32NextW(snapshot.0, &mut entry);
        }
        None
    }
}

/// Whether a module with this file name is loaded in `pid`.
pub fn is_module_loaded(pid: u32, module_file_name: &str) -> io::Result<bool> {
    // SAFETY: plain Toolhelp32 enumeration with correctly sized structs.
    unsafe {
        // The snapshot can fail spuriously with ERROR_BAD_LENGTH while the target loads modules.
        let mut attempts = 0;
        let snapshot = loop {
            match OwnedHandle::new(CreateToolhelp32Snapshot(
                TH32CS_SNAPMODULE | TH32CS_SNAPMODULE32,
                pid,
            )) {
                Err(err)
                    if err.raw_os_error() == Some(ERROR_BAD_LENGTH as i32) && attempts < 10 =>
                {
                    attempts += 1
                }
                other => break other?,
            }
        };
        let mut entry = MODULEENTRY32W {
            dwSize: size_of::<MODULEENTRY32W>() as u32,
            ..Default::default()
        };
        let mut ok = Module32FirstW(snapshot.0, &mut entry);
        while ok != 0 {
            if wide_eq_ignore_case(&entry.szModule, module_file_name) {
                return Ok(true);
            }
            ok = Module32NextW(snapshot.0, &mut entry);
        }
        Ok(false)
    }
}

/// Makes `pid` call `LoadLibraryW(dll_path)` on a remote thread and waits for it.
pub fn inject_library(pid: u32, dll_path: &Path) -> Result<(), InjectError> {
    let file_name = dll_path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let wide: Vec<u16> = dll_path.as_os_str().encode_wide().chain(Some(0)).collect();
    let byte_len = wide.len() * size_of::<u16>();

    // SAFETY: every pointer passed below is either owned by us (`wide`) or was
    // returned by the corresponding Win32 allocation call and checked for null.
    unsafe {
        let process = OwnedHandle::new(OpenProcess(
            PROCESS_CREATE_THREAD
                | PROCESS_QUERY_INFORMATION
                | PROCESS_VM_OPERATION
                | PROCESS_VM_READ
                | PROCESS_VM_WRITE,
            0,
            pid,
        ))
        .map_err(io_err("Failed to open process"))?;

        let address = VirtualAllocEx(
            process.0,
            ptr::null(),
            byte_len,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if address.is_null() {
            return Err(io_err("Failed to allocate memory in target process")(
                io::Error::last_os_error(),
            ));
        }
        let remote = RemoteAlloc {
            process: &process,
            address,
        };

        let mut written = 0usize;
        if WriteProcessMemory(
            process.0,
            address,
            wide.as_ptr().cast(),
            byte_len,
            &mut written,
        ) == 0
            || written != byte_len
        {
            return Err(io_err("Failed to write DLL path")(
                io::Error::last_os_error(),
            ));
        }

        // kernel32 is mapped at the same address in every process of a session,
        // so our LoadLibraryW address is valid inside the game too.
        let kernel32 = GetModuleHandleW(windows_sys::w!("kernel32.dll"));
        let load_library = GetProcAddress(kernel32, c"LoadLibraryW".as_ptr().cast());
        let Some(load_library) = load_library else {
            return Err(io_err("LoadLibraryW not found")(io::Error::last_os_error()));
        };
        let start: unsafe extern "system" fn(*mut c_void) -> u32 =
            std::mem::transmute(load_library);

        let thread = OwnedHandle::new(CreateRemoteThread(
            process.0,
            ptr::null(),
            0,
            Some(start),
            address,
            0,
            ptr::null_mut(),
        ))
        .map_err(io_err("Failed to create remote thread"))?;

        if WaitForSingleObject(thread.0, LOAD_TIMEOUT_MS) != WAIT_OBJECT_0 {
            // The remote thread may still read the path: leak the allocation rather than free it underneath.
            std::mem::forget(remote);
            return Err(io_err("Timed out waiting for LoadLibraryW")(
                io::ErrorKind::TimedOut.into(),
            ));
        }

        let mut exit_code = 0u32;
        if GetExitCodeThread(thread.0, &mut exit_code) == 0 {
            return Err(io_err("Failed to read remote thread result")(
                io::Error::last_os_error(),
            ));
        }
        // The exit code is only the low 32 bits of the returned HMODULE, so zero is
        // ambiguous: confirm against the module list before reporting failure.
        if exit_code == 0 && !is_module_loaded(pid, &file_name).unwrap_or(false) {
            return Err(InjectError::LoadLibraryFailed);
        }
        log::info!("LoadLibraryW returned module handle 0x{exit_code:X} (low 32 bits)");
        drop(remote);
        Ok(())
    }
}
