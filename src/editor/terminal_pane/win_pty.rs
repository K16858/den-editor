#![allow(
    non_snake_case,
    clippy::upper_case_acronyms,
    clippy::missing_transmute_annotations,
    clippy::borrow_as_ptr,
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation
)]

use std::ffi::c_void;
use std::io::{self, Read, Write};
use std::mem;
use std::path::Path;
use std::ptr;

type HANDLE = *mut c_void;
type HPCON = HANDLE;
type HRESULT = i32;
type DWORD = u32;
type BOOL = i32;
type WORD = u16;
type WCHAR = u16;
type SizeT = usize;

const INVALID_HANDLE_VALUE: HANDLE = !0usize as HANDLE;
const S_OK: HRESULT = 0;
const EXTENDED_STARTUPINFO_PRESENT: DWORD = 0x0008_0000;
const CREATE_UNICODE_ENVIRONMENT: DWORD = 0x0000_0400;
const STARTF_USESTDHANDLES: DWORD = 0x0000_0100;
const PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE: usize = 0x0002_0016;

#[repr(C)]
struct COORD {
    X: i16,
    Y: i16,
}

#[repr(C)]
#[allow(dead_code)]
struct STARTUPINFOW {
    cb: DWORD,
    lpReserved: *mut WCHAR,
    lpDesktop: *mut WCHAR,
    lpTitle: *mut WCHAR,
    dwX: DWORD,
    dwY: DWORD,
    dwXSize: DWORD,
    dwYSize: DWORD,
    dwXCountChars: DWORD,
    dwYCountChars: DWORD,
    dwFillAttribute: DWORD,
    dwFlags: DWORD,
    wShowWindow: WORD,
    cbReserved2: WORD,
    lpReserved2: *mut u8,
    hStdInput: HANDLE,
    hStdOutput: HANDLE,
    hStdError: HANDLE,
}

#[repr(C)]
struct STARTUPINFOEXW {
    StartupInfo: STARTUPINFOW,
    lpAttributeList: *mut c_void,
}

#[repr(C)]
#[allow(dead_code)]
struct PROCESS_INFORMATION {
    hProcess: HANDLE,
    hThread: HANDLE,
    dwProcessId: DWORD,
    dwThreadId: DWORD,
}

unsafe extern "system" {
    fn CreatePipe(
        hReadPipe: *mut HANDLE,
        hWritePipe: *mut HANDLE,
        lpPipeAttributes: *const c_void,
        nSize: DWORD,
    ) -> BOOL;

    fn CloseHandle(hObject: HANDLE) -> BOOL;

    fn CreateProcessW(
        lpApplicationName: *const WCHAR,
        lpCommandLine: *mut WCHAR,
        lpProcessAttributes: *const c_void,
        lpThreadAttributes: *const c_void,
        bInheritHandles: BOOL,
        dwCreationFlags: DWORD,
        lpEnvironment: *mut c_void,
        lpCurrentDirectory: *const WCHAR,
        lpStartupInfo: *const STARTUPINFOW,
        lpProcessInformation: *mut PROCESS_INFORMATION,
    ) -> BOOL;

    fn InitializeProcThreadAttributeList(
        lpAttributeList: *mut c_void,
        dwAttributeCount: DWORD,
        dwFlags: DWORD,
        lpSize: *mut SizeT,
    ) -> BOOL;

    fn UpdateProcThreadAttribute(
        lpAttributeList: *mut c_void,
        dwFlags: DWORD,
        Attribute: usize,
        lpValue: *mut c_void,
        cbSize: SizeT,
        lpPreviousValue: *mut c_void,
        lpReturnSize: *mut SizeT,
    ) -> BOOL;

    fn DeleteProcThreadAttributeList(lpAttributeList: *mut c_void);

    fn TerminateProcess(hProcess: HANDLE, uExitCode: u32) -> BOOL;

    fn ReadFile(
        hFile: HANDLE,
        lpBuffer: *mut c_void,
        nNumberOfBytesToRead: DWORD,
        lpNumberOfBytesRead: *mut DWORD,
        lpOverlapped: *mut c_void,
    ) -> BOOL;

    fn WriteFile(
        hFile: HANDLE,
        lpBuffer: *const c_void,
        nNumberOfBytesToWrite: DWORD,
        lpNumberOfBytesWritten: *mut DWORD,
        lpOverlapped: *mut c_void,
    ) -> BOOL;

    fn GetModuleHandleW(lpModuleName: *const WCHAR) -> HANDLE;
    fn GetProcAddress(hModule: HANDLE, lpProcName: *const u8) -> *mut c_void;
}

// ---------- ConPTY dynamic API ----------

type CreatePseudoConsoleFn =
    unsafe extern "system" fn(COORD, HANDLE, HANDLE, DWORD, *mut HPCON) -> HRESULT;
type ResizePseudoConsoleFn = unsafe extern "system" fn(HPCON, COORD) -> HRESULT;
type ClosePseudoConsoleFn = unsafe extern "system" fn(HPCON);

struct ConPtyApi {
    create: CreatePseudoConsoleFn,
    resize: ResizePseudoConsoleFn,
    close: ClosePseudoConsoleFn,
}

fn load_conpty_api() -> io::Result<ConPtyApi> {
    let module_name = wide_string("kernel32.dll");
    let module = unsafe { GetModuleHandleW(module_name.as_ptr()) };
    if module.is_null() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "kernel32.dll not found",
        ));
    }

    unsafe {
        let p_create = GetProcAddress(module, c"CreatePseudoConsole".as_ptr().cast());
        let p_resize = GetProcAddress(module, c"ResizePseudoConsole".as_ptr().cast());
        let p_close = GetProcAddress(module, c"ClosePseudoConsole".as_ptr().cast());

        if p_create.is_null() || p_resize.is_null() || p_close.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "ConPTY not available (Windows 10 1809+ required)",
            ));
        }

        Ok(ConPtyApi {
            create: mem::transmute(p_create),
            resize: mem::transmute(p_resize),
            close: mem::transmute(p_close),
        })
    }
}

// ---------- Handle wrappers ----------

struct RawHandle(HANDLE);

unsafe impl Send for RawHandle {}

impl RawHandle {
    fn get(&self) -> HANDLE {
        self.0
    }
}

impl Drop for RawHandle {
    fn drop(&mut self) {
        if !self.0.is_null() && self.0 != INVALID_HANDLE_VALUE {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

pub struct PipeReader(RawHandle);

impl Read for PipeReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut n: DWORD = 0;
        let ok = unsafe {
            ReadFile(
                self.0.get(),
                buf.as_mut_ptr().cast(),
                buf.len() as DWORD,
                &raw mut n,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            let err = io::Error::last_os_error();
            if err.raw_os_error() == Some(109) {
                Ok(0)
            } else {
                Err(err)
            }
        } else {
            Ok(n as usize)
        }
    }
}

struct PipeWriter(RawHandle);

impl Write for PipeWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut n: DWORD = 0;
        let ok = unsafe {
            WriteFile(
                self.0.get(),
                buf.as_ptr().cast(),
                buf.len() as DWORD,
                &raw mut n,
                ptr::null_mut(),
            )
        };
        if ok == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(n as usize)
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

// ---------- helpers ----------

fn wide_string(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn create_pipe() -> io::Result<(RawHandle, RawHandle)> {
    let mut rd: HANDLE = ptr::null_mut();
    let mut wr: HANDLE = ptr::null_mut();
    let ok = unsafe { CreatePipe(&raw mut rd, &raw mut wr, ptr::null(), 0) };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok((RawHandle(rd), RawHandle(wr)))
    }
}

fn strip_unc_prefix(p: &Path) -> String {
    let s = p.to_string_lossy();
    if let Some(stripped) = s.strip_prefix("\\\\?\\") {
        stripped.to_string()
    } else {
        s.into_owned()
    }
}

// ---------- WinPty ----------

pub struct WinPty {
    con: HPCON,
    writer: PipeWriter,
    process: RawHandle,
    api: ConPtyApi,
}

unsafe impl Send for WinPty {}

impl WinPty {
    pub fn spawn(cwd: &Path, cols: u16, rows: u16) -> io::Result<(Self, PipeReader)> {
        let api = load_conpty_api()?;

        let (in_read, in_write) = create_pipe()?;
        let (out_read, out_write) = create_pipe()?;

        let mut hpc: HPCON = INVALID_HANDLE_VALUE;
        let hr = unsafe {
            (api.create)(
                COORD {
                    X: cols as i16,
                    Y: rows as i16,
                },
                in_read.get(),
                out_write.get(),
                0,
                &raw mut hpc,
            )
        };
        if hr != S_OK {
            return Err(io::Error::other(format!(
                "CreatePseudoConsole HRESULT 0x{hr:08X}"
            )));
        }

        drop(in_read);
        drop(out_write);

        let process = spawn_child(hpc, cwd, &api)?;

        Ok((
            Self {
                con: hpc,
                writer: PipeWriter(in_write),
                process,
                api,
            },
            PipeReader(out_read),
        ))
    }

    pub fn write_all(&mut self, data: &[u8]) -> io::Result<()> {
        self.writer.write_all(data)
    }

    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        let hr = unsafe {
            (self.api.resize)(
                self.con,
                COORD {
                    X: cols as i16,
                    Y: rows as i16,
                },
            )
        };
        if hr == S_OK {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "ResizePseudoConsole HRESULT 0x{hr:08X}"
            )))
        }
    }

    pub fn kill(&mut self) {
        unsafe {
            TerminateProcess(self.process.get(), 1);
        }
    }
}

impl Drop for WinPty {
    fn drop(&mut self) {
        self.kill();
        unsafe {
            (self.api.close)(self.con);
        }
    }
}

// ---------- child process ----------

fn spawn_child(hpc: HPCON, cwd: &Path, api: &ConPtyApi) -> io::Result<RawHandle> {
    let mut attr_size: SizeT = 0;
    unsafe {
        InitializeProcThreadAttributeList(ptr::null_mut(), 1, 0, &raw mut attr_size);
    }

    let mut attr_buf: Vec<u8> = vec![0u8; attr_size];
    let attr_ptr = attr_buf.as_mut_ptr().cast();

    if unsafe { InitializeProcThreadAttributeList(attr_ptr, 1, 0, &raw mut attr_size) } == 0 {
        return Err(io::Error::last_os_error());
    }

    if unsafe {
        UpdateProcThreadAttribute(
            attr_ptr,
            0,
            PROC_THREAD_ATTRIBUTE_PSEUDOCONSOLE,
            hpc,
            mem::size_of::<HPCON>(),
            ptr::null_mut(),
            ptr::null_mut(),
        )
    } == 0
    {
        let err = io::Error::last_os_error();
        unsafe {
            DeleteProcThreadAttributeList(attr_ptr);
        }
        return Err(err);
    }

    let mut si: STARTUPINFOEXW = unsafe { mem::zeroed() };
    si.StartupInfo.cb = mem::size_of::<STARTUPINFOEXW>() as DWORD;
    si.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    si.StartupInfo.hStdInput = INVALID_HANDLE_VALUE;
    si.StartupInfo.hStdOutput = INVALID_HANDLE_VALUE;
    si.StartupInfo.hStdError = INVALID_HANDLE_VALUE;
    si.lpAttributeList = attr_ptr;

    let shell = std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_string());
    let mut cmdline = wide_string(&shell);

    let cwd_wide = wide_string(&strip_unc_prefix(cwd));

    let mut pi: PROCESS_INFORMATION = unsafe { mem::zeroed() };
    let ok = unsafe {
        CreateProcessW(
            ptr::null(),
            cmdline.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            EXTENDED_STARTUPINFO_PRESENT | CREATE_UNICODE_ENVIRONMENT,
            ptr::null_mut(),
            cwd_wide.as_ptr(),
            &raw const si.StartupInfo,
            &raw mut pi,
        )
    };

    unsafe {
        DeleteProcThreadAttributeList(attr_ptr);
    }

    if ok == 0 {
        let err = io::Error::last_os_error();
        unsafe {
            (api.close)(hpc);
        }
        return Err(err);
    }

    unsafe {
        CloseHandle(pi.hThread);
    }
    Ok(RawHandle(pi.hProcess))
}
