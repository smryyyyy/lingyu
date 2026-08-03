// Elevated helper process for Raw Input keyboard capture.
// Runs at HIGH integrity (via ShellExecute "runas").
// Communicates key state via memory-mapped file "LingyuHotkeyState": i32 (0/1).
// Event "LingyuHelperReady": signaled when helper is initialized.

use std::sync::atomic::AtomicU32;

// ── Module-level Win32 FFI ──
extern "system" {
    fn CreateFileMappingW(hFile: isize, lpAttributes: *const std::ffi::c_void,
        flProtect: u32, dwMaximumSizeHigh: u32, dwMaximumSizeLow: u32,
        lpName: *const u16) -> isize;
    fn OpenFileMappingW(dwDesiredAccess: u32, bInheritHandle: i32,
        lpName: *const u16) -> isize;
    fn MapViewOfFile(hFileMappingObject: isize, dwDesiredAccess: u32,
        dwFileOffsetHigh: u32, dwFileOffsetLow: u32,
        dwNumberOfBytesToMap: usize) -> *mut std::ffi::c_void;
    fn UnmapViewOfFile(lpBaseAddress: *const std::ffi::c_void) -> i32;
    fn CloseHandle(hObject: isize) -> i32;
    fn CreateEventW(lpEventAttributes: *const std::ffi::c_void,
        bManualReset: i32, bInitialState: i32, lpName: *const u16) -> isize;
    fn OpenEventW(dwDesiredAccess: u32, bInheritHandle: i32,
        lpName: *const u16) -> isize;
    fn SetEvent(hEvent: isize) -> i32;
    fn WaitForSingleObject(hHandle: isize, dwMilliseconds: u32) -> u32;
    fn GetModuleHandleW(lpModuleName: *const u16) -> isize;
    fn RegisterClassExW(wcx: *const std::ffi::c_void) -> u16;
    fn CreateWindowExW(dwExStyle: u32, lpClassName: *const u16,
        lpWindowName: *const u16, dwStyle: u32, X: i32, Y: i32,
        nWidth: i32, nHeight: i32, hWndParent: isize, hMenu: isize,
        hInstance: isize, lpParam: isize) -> isize;
    fn ShowWindow(hWnd: isize, nCmdShow: i32) -> i32;
    fn DefWindowProcW(hWnd: isize, Msg: u32, wParam: usize, lParam: isize) -> isize;
    fn GetMessageW(lpMsg: *mut std::ffi::c_void, hWnd: isize,
        wMsgFilterMin: u32, wMsgFilterMax: u32) -> i32;
    fn DispatchMessageW(lpMsg: *const std::ffi::c_void) -> isize;
    fn RegisterRawInputDevices(pRawInputDevices: *const std::ffi::c_void,
        uiNumDevices: u32, cbSize: u32) -> i32;
    fn GetRawInputData(hRawInput: isize, uiCommand: u32,
        pData: *mut std::ffi::c_void, pcbSize: *mut u32,
        cbSizeHeader: u32) -> u32;
    fn RegisterHotKey(hWnd: isize, id: i32, fsModifiers: u32, vk: u32) -> i32;
    fn ChangeWindowMessageFilterEx(hWnd: isize, message: u32,
        action: u32, pChangeFilter: *mut std::ffi::c_void) -> i32;
    fn ShellExecuteW(hwnd: isize, lpOperation: *const u16,
        lpFile: *const u16, lpParameters: *const u16,
        lpDirectory: *const u16, nShowCmd: i32) -> isize;
}

const WM_INPUT: u32 = 0x00FF;
const WM_HOTKEY: u32 = 0x0312;
const RID_INPUT: u32 = 0x10000003;
const RIM_TYPEKEYBOARD: u32 = 1;
const RIDEV_INPUTSINK: u32 = 0x0100;
const RI_KEY_BREAK: u16 = 0x0001;
const PAGE_READWRITE: u32 = 0x04;
const FILE_MAP_WRITE: u32 = 0x0002;
const FILE_MAP_READ: u32 = 0x0004;
const SYNCHRONIZE: u32 = 0x00100000;

// ── Entry point for elevated helper process ──
pub fn run() {
    unsafe {
        let mem_name: Vec<u16> = "LingyuHotkeyState\0".encode_utf16().collect();
        let ready_name: Vec<u16> = "LingyuHelperReady\0".encode_utf16().collect();

        // Create shared memory (8 bytes: offset0=i32 keyState, offset4=u32 VK)
        let hmap = CreateFileMappingW(-1isize, std::ptr::null(), PAGE_READWRITE, 0, 8, mem_name.as_ptr());
        let view = MapViewOfFile(hmap, FILE_MAP_WRITE, 0, 0, 8);
        let state_ptr = view as *mut i32;
        let vk_ptr = (view as *mut u32).add(1); // offset 4
        state_ptr.write(0);
        vk_ptr.write(0x79); // default F10 VK

        // Store ptrs for WndProc access
        STATE_PTR = state_ptr;
        VK_PTR = vk_ptr;

        // Signal ready
        let hevent = CreateEventW(std::ptr::null(), 1, 0, ready_name.as_ptr());
        SetEvent(hevent);

        // Create hidden window
        #[repr(C)]
        struct WNDCLASSEXW { cbSize: u32, style: u32,
            lpfnWndProc: unsafe extern "system" fn(isize, u32, usize, isize) -> isize,
            cbClsExtra: i32, cbWndExtra: i32, hInstance: isize, hIcon: isize,
            hCursor: isize, hbrBackground: isize, lpszMenuName: *const u16,
            lpszClassName: *const u16, hIconSm: isize }
        #[repr(C)]
        struct RAWINPUTDEVICE { usUsagePage: u16, usUsage: u16, dwFlags: u32, hwndTarget: isize }

        let hinst = GetModuleHandleW(std::ptr::null());
        let cls: Vec<u16> = "LingyuHelperWnd\0".encode_utf16().collect();
        let title: Vec<u16> = "Lingyu Helper Window\0".encode_utf16().collect();
        let wcx = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32, style: 0,
            lpfnWndProc: wnd_proc, cbClsExtra: 0, cbWndExtra: 0,
            hInstance: hinst, hIcon: 0, hCursor: 0, hbrBackground: 0,
            lpszMenuName: std::ptr::null(), lpszClassName: cls.as_ptr(), hIconSm: 0,
        };
        RegisterClassExW(&wcx as *const _ as *const _);
        let hwnd = CreateWindowExW(0, cls.as_ptr(), title.as_ptr(),
            0x00CF0000, 0, 0, 100, 100, 0, 0, hinst, 0);
        ShowWindow(hwnd, 0); // SW_HIDE

        // Register Raw Input + HotKey
        let mut rid: [RAWINPUTDEVICE; 2] = [
            RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x06, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd },
            RAWINPUTDEVICE { usUsagePage: 0x01, usUsage: 0x02, dwFlags: RIDEV_INPUTSINK, hwndTarget: hwnd },
        ];
        RegisterRawInputDevices(rid.as_mut_ptr() as *const _, 2, std::mem::size_of::<RAWINPUTDEVICE>() as u32);
        ChangeWindowMessageFilterEx(hwnd, WM_INPUT, 1, std::ptr::null_mut());
        let f10_vk = *vk_ptr;
        RegisterHotKey(hwnd, 1, 0x4000, f10_vk);

        // Set 1-second timer to check for VK changes (hotkey update)
        extern "system" { fn SetTimer(hWnd: isize, nIDEvent: usize, uElapse: u32, lpTimerFunc: *const std::ffi::c_void) -> usize; }
        SetTimer(hwnd, 2, 1000, std::ptr::null());

        // Message loop
        let mut msg = [0u8; 48];
        while GetMessageW(msg.as_mut_ptr() as *mut _, 0, 0, 0) > 0 {
            DispatchMessageW(msg.as_mut_ptr() as *const _);
        }

        // Cleanup
        STATE_PTR = std::ptr::null_mut();
        UnmapViewOfFile(view);
        CloseHandle(hmap);
        CloseHandle(hevent);
    }
}

// ── WndProc and shared state ──
static mut STATE_PTR: *mut i32 = std::ptr::null_mut();
static mut VK_PTR: *mut u32 = std::ptr::null_mut();

unsafe extern "system" fn wnd_proc(h: isize, msg: u32, w: usize, lp: isize) -> isize {
    const WM_TIMER: u32 = 0x0113;
    if msg == WM_TIMER && w == 2 {
        // Timer tick: check if VK changed and update hotkey registration
        check_and_update_hotkey(h);
        return 0;
    }
    if msg == WM_INPUT || msg == WM_HOTKEY {
        handle_input(msg, lp);
    }
    DefWindowProcW(h, msg, w, lp)
}

static mut CURRENT_HOTKEY_VK: u32 = 0x79;

fn check_and_update_hotkey(hwnd: isize) {
    unsafe {
        extern "system" { fn UnregisterHotKey(hWnd: isize, id: i32) -> i32; }
        let ptr = STATE_PTR;
        let vk_ptr = VK_PTR;
        if ptr.is_null() || vk_ptr.is_null() { return; }

        // Check for exit signal from main app (-1)
        if *ptr == -1 {
            extern "system" { fn PostQuitMessage(nExitCode: i32); }
            PostQuitMessage(0);
            return;
        }

        let new_vk = *vk_ptr;
        let old_vk = CURRENT_HOTKEY_VK;
        if new_vk != old_vk {
            UnregisterHotKey(hwnd, 1);
            RegisterHotKey(hwnd, 1, 0x4000, new_vk);
            CURRENT_HOTKEY_VK = new_vk;
        }
    }
}

fn handle_input(msg: u32, lp: isize) {
    unsafe {
        let ptr = STATE_PTR;
        let vk_ptr = VK_PTR;
        if ptr.is_null() || vk_ptr.is_null() { return; }
        let target_vk = *vk_ptr;

        if msg == WM_HOTKEY {
            ptr.write(1);
            return;
        }

        // WM_INPUT — parse RAWINPUT struct.
        // RAWINPUTHEADER on x64 (24 bytes):
        //   dwType(0..4), dwSize(4..8), hDevice(8..16=HANDLE), wParam(16..24=WPARAM)
        // RAWKEYBOARD follows at offset 24:
        //   MakeCode(24..26), Flags(26..28), Reserved(28..30), VKey(30..32), Message(32..36)
        let mut size: u32 = 64;
        let mut buf = [0u8; 64];
        let ret = GetRawInputData(lp, RID_INPUT, buf.as_mut_ptr() as *mut _, &mut size, 24);
        if ret != 0xFFFFFFFF && size >= 36 {
            let dw_type = u32::from_ne_bytes([buf[0], buf[1], buf[2], buf[3]]);
            if dw_type == RIM_TYPEKEYBOARD {
                let vkey = u16::from_ne_bytes([buf[30], buf[31]]);
                let flags = u16::from_ne_bytes([buf[26], buf[27]]);
                if vkey as u32 == target_vk {
                    let pressed = (flags & RI_KEY_BREAK) == 0;
                    ptr.write(if pressed { 1 } else { 0 });
                }
            }
        }
    }
}

// ── Main process side: write/read from helper IPC ──
pub struct HelperReader {
    hmap: isize,
    view: *mut std::ffi::c_void,
    /// Process handle of the helper (0 if unknown). Used for crash detection via WaitForSingleObject.
    hprocess: isize,
}

impl HelperReader {
    pub fn is_alive(&self) -> bool { !self.view.is_null() }
    pub fn is_key_pressed(&self) -> bool {
        if self.view.is_null() { return false; }
        unsafe { *(self.view as *const i32) != 0 }
    }
}

impl Drop for HelperReader {
    fn drop(&mut self) {
        if !self.view.is_null() { unsafe { UnmapViewOfFile(self.view); } }
        if self.hmap != 0 { unsafe { CloseHandle(self.hmap); } }
        if self.hprocess != 0 { unsafe { CloseHandle(self.hprocess); } }
    }
}

/// Try to connect to an already-running helper process.
/// Also tries to open the helper process to detect crashes via WaitForSingleObject.
pub fn try_connect() -> Option<HelperReader> {
    unsafe {
        let name: Vec<u16> = "LingyuHotkeyState\0".encode_utf16().collect();
        let hmap = OpenFileMappingW(FILE_MAP_READ, 0, name.as_ptr());
        if hmap == 0 { return None; }
        let view = MapViewOfFile(hmap, FILE_MAP_READ, 0, 0, 8);
        if view.is_null() { CloseHandle(hmap); return None; }

        // Store INVALID_HANDLE_VALUE as hprocess — we can't easily obtain the helper's
        // process handle from try_connect(). Crash detection relies on the retry logic
        // (every ~500ms) which will eventually fail to map and reconnect fresh.
        Some(HelperReader { hmap, view, hprocess: 0 })
    }
}

/// Write new VK code to helper's shared memory (offset 4)
pub fn update_vk(vk: u32) {
    unsafe {
        let name: Vec<u16> = "LingyuHotkeyState\0".encode_utf16().collect();
        let hmap = OpenFileMappingW(FILE_MAP_WRITE, 0, name.as_ptr());
        if hmap == 0 { return; }
        let view = MapViewOfFile(hmap, FILE_MAP_WRITE, 0, 0, 8);
        if !view.is_null() {
            let vk_ptr = (view as *mut u32).add(1);
            *vk_ptr = vk;
            UnmapViewOfFile(view);
        }
        CloseHandle(hmap);
    }
}

/// Signal helper to exit (write -1 to state). Called on app quit.
pub fn signal_exit() {
    unsafe {
        let name: Vec<u16> = "LingyuHotkeyState\0".encode_utf16().collect();
        let hmap = OpenFileMappingW(FILE_MAP_WRITE, 0, name.as_ptr());
        if hmap == 0 { return; }
        let view = MapViewOfFile(hmap, FILE_MAP_WRITE, 0, 0, 8);
        if !view.is_null() {
            *(view as *mut i32) = -1; // exit signal
            UnmapViewOfFile(view);
        }
        CloseHandle(hmap);
    }
}

/// Launch the helper process elevated. Returns true if launched.
pub fn try_launch() -> bool {
    unsafe {
        let runas: Vec<u16> = "runas\0".encode_utf16().collect();
        let args: Vec<u16> = "--helper\0".encode_utf16().collect();

        let exe_path = match std::env::current_exe() {
            Ok(p) => p,
            Err(_) => { crate::log::debug("helper: can't get exe path"); return false; }
        };
        crate::log::debug(&format!("helper: launching '{}' with --helper", exe_path.display()));

        // Use ShellExecuteExW instead of ShellExecuteW for better error handling
        #[repr(C)]
        struct SHELLEXECUTEINFOW {
            cbSize: u32,
            fMask: u32,
            hwnd: isize,
            lpVerb: *const u16,
            lpFile: *const u16,
            lpParameters: *const u16,
            lpDirectory: *const u16,
            nShow: i32,
            hInstApp: isize,
            lpIDList: *const std::ffi::c_void,
            lpClass: *const u16,
            hkeyClass: isize,
            dwHotKey: u32,
            hMonitorOrUnion: isize,
            hProcess: isize,
        }

        extern "system" {
            fn ShellExecuteExW(pExecInfo: *mut SHELLEXECUTEINFOW) -> i32;
        }

        let exe_wide: Vec<u16> = exe_path.to_string_lossy().encode_utf16().collect();
        let args_wide: Vec<u16> = "--helper\0".encode_utf16().collect();
        let runas_wide: Vec<u16> = "runas\0".encode_utf16().collect();

        let mut sei = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: 0x00000040 | 0x00000080, // SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC
            hwnd: 0,
            lpVerb: runas_wide.as_ptr(),
            lpFile: exe_wide.as_ptr(),
            lpParameters: args_wide.as_ptr(),
            lpDirectory: std::ptr::null(),
            nShow: 1, // SW_SHOWNORMAL
            hInstApp: 0,
            lpIDList: std::ptr::null(),
            lpClass: std::ptr::null(),
            hkeyClass: 0,
            dwHotKey: 0,
            hMonitorOrUnion: 0,
            hProcess: 0,
        };

        crate::log::debug("helper: calling ShellExecuteExW(runas)...");
        let result = ShellExecuteExW(&mut sei);
        if result == 0 {
            // Get last error for diagnosis
            extern "system" { fn GetLastError() -> u32; }
            let err = GetLastError();
            crate::log::debug(&format!("helper: ShellExecuteExW FAILED, GetLastError={}", err));
            return false;
        }
        crate::log::debug(&format!("helper: launched, hInstApp={} hProcess=0x{:X}", sei.hInstApp, sei.hProcess));

        // Wait for helper to initialize (up to 5s)
        let ready_name: Vec<u16> = "LingyuHelperReady\0".encode_utf16().collect();
        let hevent = OpenEventW(SYNCHRONIZE, 0, ready_name.as_ptr());
        if hevent != 0 {
            crate::log::debug("helper: waiting for ready event...");
            WaitForSingleObject(hevent, 5000);
            CloseHandle(hevent);
            crate::log::debug("helper: ready event received");
        } else {
            crate::log::debug("helper: ready event not found (helper may not have started)");
        }
        true
    }
}
