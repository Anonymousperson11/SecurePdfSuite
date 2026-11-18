#[inline]
pub unsafe fn BCryptCloseAlgorithmProvider(
    halgorithm: BCRYPT_ALG_HANDLE,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptCloseAlgorithmProvider(halgorithm : BCRYPT_ALG_HANDLE, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe { BCryptCloseAlgorithmProvider(halgorithm as _, dwflags) }
}
#[inline]
pub unsafe fn BCryptCreateHash(
    halgorithm: BCRYPT_ALG_HANDLE,
    phhash: *mut BCRYPT_HASH_HANDLE,
    pbhashobject: Option<*mut u8>,
    cbhashobject: u32,
    pbsecret: Option<&[u8]>,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptCreateHash(halgorithm : BCRYPT_ALG_HANDLE, phhash : *mut BCRYPT_HASH_HANDLE, pbhashobject : *mut u8, cbhashobject : u32, pbsecret : *const u8, cbsecret : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptCreateHash(
            halgorithm as _,
            phhash as _,
            pbhashobject.unwrap_or(core::mem::zeroed()) as _,
            cbhashobject,
            pbsecret.map_or(core::ptr::null(), |slice| slice.as_ptr()),
            pbsecret.map_or(0, |slice| slice.len().try_into().unwrap()),
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptDestroyHash(hhash: BCRYPT_HASH_HANDLE) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptDestroyHash(hhash : BCRYPT_HASH_HANDLE) -> windows_core::NTSTATUS);
    unsafe { BCryptDestroyHash(hhash as _) }
}
#[inline]
pub unsafe fn BCryptFinishHash(
    hhash: BCRYPT_HASH_HANDLE,
    pboutput: *mut u8,
    cboutput: u32,
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptFinishHash(hhash : BCRYPT_HASH_HANDLE, pboutput : *mut u8, cboutput : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe { BCryptFinishHash(hhash as _, pboutput as _, cboutput, dwflags) }
}
#[inline]
pub unsafe fn BCryptHash(
    halgorithm: BCRYPT_ALG_HANDLE,
    pbsecret: Option<&[u8]>,
    pbinput: &[u8],
    pboutput: *mut u8,
    cboutput: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptHash(halgorithm : BCRYPT_ALG_HANDLE, pbsecret : *const u8, cbsecret : u32, pbinput : *const u8, cbinput : u32, pboutput : *mut u8, cboutput : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptHash(
            halgorithm as _,
            pbsecret.map_or(core::ptr::null(), |slice| slice.as_ptr()),
            pbsecret.map_or(0, |slice| slice.len().try_into().unwrap()),
            pbinput.as_ptr(),
            pbinput.len().try_into().unwrap(),
            pboutput as _,
            cboutput,
        )
    }
}
#[inline]
pub unsafe fn BCryptHashData(
    hhash: BCRYPT_HASH_HANDLE,
    pbinput: &[u8],
    dwflags: u32,
) -> windows_core::NTSTATUS {
    windows_core::link!("bcrypt.dll" "system" fn BCryptHashData(hhash : BCRYPT_HASH_HANDLE, pbinput : *const u8, cbinput : u32, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptHashData(
            hhash as _,
            pbinput.as_ptr(),
            pbinput.len().try_into().unwrap(),
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn BCryptOpenAlgorithmProvider<P1, P2>(
    phalgorithm: *mut BCRYPT_ALG_HANDLE,
    pszalgid: P1,
    pszimplementation: P2,
    dwflags: u32,
) -> windows_core::NTSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
    P2: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("bcrypt.dll" "system" fn BCryptOpenAlgorithmProvider(phalgorithm : *mut BCRYPT_ALG_HANDLE, pszalgid : windows_core::PCWSTR, pszimplementation : windows_core::PCWSTR, dwflags : u32) -> windows_core::NTSTATUS);
    unsafe {
        BCryptOpenAlgorithmProvider(
            phalgorithm as _,
            pszalgid.param().abi(),
            pszimplementation.param().abi(),
            dwflags,
        )
    }
}
#[inline]
pub unsafe fn CloseCompressor(compressorhandle: COMPRESSOR_HANDLE) -> windows_core::BOOL {
    windows_core::link!("cabinet.dll" "system" fn CloseCompressor(compressorhandle : COMPRESSOR_HANDLE) -> windows_core::BOOL);
    unsafe { CloseCompressor(compressorhandle) }
}
#[inline]
pub unsafe fn CloseDecompressor(decompressorhandle: DECOMPRESSOR_HANDLE) -> windows_core::BOOL {
    windows_core::link!("cabinet.dll" "system" fn CloseDecompressor(decompressorhandle : DECOMPRESSOR_HANDLE) -> windows_core::BOOL);
    unsafe { CloseDecompressor(decompressorhandle) }
}
#[inline]
pub unsafe fn CloseHandle(hobject: HANDLE) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn CloseHandle(hobject : HANDLE) -> windows_core::BOOL);
    unsafe { CloseHandle(hobject) }
}
#[inline]
pub unsafe fn CoCreateInstance<P1, T>(
    rclsid: *const windows_core::GUID,
    punkouter: P1,
    dwclscontext: u32,
) -> windows_core::Result<T>
where
    P1: windows_core::Param<windows_core::IUnknown>,
    T: windows_core::Interface,
{
    windows_core::link!("ole32.dll" "system" fn CoCreateInstance(rclsid : *const windows_core::GUID, punkouter : *mut core::ffi::c_void, dwclscontext : u32, riid : *const windows_core::GUID, ppv : *mut *mut core::ffi::c_void) -> windows_core::HRESULT);
    let mut result__ = core::ptr::null_mut();
    unsafe {
        CoCreateInstance(
            rclsid,
            punkouter.param().abi(),
            dwclscontext,
            &T::IID,
            &mut result__,
        )
        .and_then(|| windows_core::imp::Type::from_abi(result__))
    }
}
#[inline]
pub unsafe fn CoInitializeEx(
    pvreserved: Option<*const core::ffi::c_void>,
    dwcoinit: u32,
) -> windows_core::HRESULT {
    windows_core::link!("ole32.dll" "system" fn CoInitializeEx(pvreserved : *const core::ffi::c_void, dwcoinit : u32) -> windows_core::HRESULT);
    unsafe { CoInitializeEx(pvreserved.unwrap_or(core::mem::zeroed()) as _, dwcoinit) }
}
#[inline]
pub unsafe fn CoUninitialize() {
    windows_core::link!("ole32.dll" "system" fn CoUninitialize());
    unsafe { CoUninitialize() }
}
#[inline]
pub unsafe fn Compress(
    compressorhandle: COMPRESSOR_HANDLE,
    uncompresseddata: Option<*const core::ffi::c_void>,
    uncompresseddatasize: usize,
    compressedbuffer: Option<*mut core::ffi::c_void>,
    compressedbuffersize: usize,
    compresseddatasize: *mut usize,
) -> windows_core::BOOL {
    windows_core::link!("cabinet.dll" "system" fn Compress(compressorhandle : COMPRESSOR_HANDLE, uncompresseddata : *const core::ffi::c_void, uncompresseddatasize : usize, compressedbuffer : *mut core::ffi::c_void, compressedbuffersize : usize, compresseddatasize : *mut usize) -> windows_core::BOOL);
    unsafe {
        Compress(
            compressorhandle,
            uncompresseddata.unwrap_or(core::mem::zeroed()) as _,
            uncompresseddatasize,
            compressedbuffer.unwrap_or(core::mem::zeroed()) as _,
            compressedbuffersize,
            compresseddatasize as _,
        )
    }
}
#[inline]
pub unsafe fn CreateCompressor(
    algorithm: u32,
    allocationroutines: Option<*const COMPRESS_ALLOCATION_ROUTINES>,
    compressorhandle: *mut COMPRESSOR_HANDLE,
) -> windows_core::BOOL {
    windows_core::link!("cabinet.dll" "system" fn CreateCompressor(algorithm : u32, allocationroutines : *const COMPRESS_ALLOCATION_ROUTINES, compressorhandle : *mut COMPRESSOR_HANDLE) -> windows_core::BOOL);
    unsafe {
        CreateCompressor(
            algorithm,
            allocationroutines.unwrap_or(core::mem::zeroed()) as _,
            compressorhandle as _,
        )
    }
}
#[inline]
pub unsafe fn CreateDecompressor(
    algorithm: u32,
    allocationroutines: Option<*const COMPRESS_ALLOCATION_ROUTINES>,
    decompressorhandle: *mut COMPRESSOR_HANDLE,
) -> windows_core::BOOL {
    windows_core::link!("cabinet.dll" "system" fn CreateDecompressor(algorithm : u32, allocationroutines : *const COMPRESS_ALLOCATION_ROUTINES, decompressorhandle : *mut COMPRESSOR_HANDLE) -> windows_core::BOOL);
    unsafe {
        CreateDecompressor(
            algorithm,
            allocationroutines.unwrap_or(core::mem::zeroed()) as _,
            decompressorhandle as _,
        )
    }
}
#[inline]
pub unsafe fn CreateToolhelp32Snapshot(dwflags: u32, th32processid: u32) -> HANDLE {
    windows_core::link!("kernel32.dll" "system" fn CreateToolhelp32Snapshot(dwflags : u32, th32processid : u32) -> HANDLE);
    unsafe { CreateToolhelp32Snapshot(dwflags, th32processid) }
}
#[inline]
pub unsafe fn Decompress(
    decompressorhandle: DECOMPRESSOR_HANDLE,
    compresseddata: Option<*const core::ffi::c_void>,
    compresseddatasize: usize,
    uncompressedbuffer: Option<*mut core::ffi::c_void>,
    uncompressedbuffersize: usize,
    uncompresseddatasize: Option<*mut usize>,
) -> windows_core::BOOL {
    windows_core::link!("cabinet.dll" "system" fn Decompress(decompressorhandle : DECOMPRESSOR_HANDLE, compresseddata : *const core::ffi::c_void, compresseddatasize : usize, uncompressedbuffer : *mut core::ffi::c_void, uncompressedbuffersize : usize, uncompresseddatasize : *mut usize) -> windows_core::BOOL);
    unsafe {
        Decompress(
            decompressorhandle,
            compresseddata.unwrap_or(core::mem::zeroed()) as _,
            compresseddatasize,
            uncompressedbuffer.unwrap_or(core::mem::zeroed()) as _,
            uncompressedbuffersize,
            uncompresseddatasize.unwrap_or(core::mem::zeroed()) as _,
        )
    }
}
#[inline]
pub unsafe fn FindResourceW<P1, P2>(hmodule: Option<HMODULE>, lpname: P1, lptype: P2) -> HRSRC
where
    P1: windows_core::Param<windows_core::PCWSTR>,
    P2: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("kernel32.dll" "system" fn FindResourceW(hmodule : HMODULE, lpname : windows_core::PCWSTR, lptype : windows_core::PCWSTR) -> HRSRC);
    unsafe {
        FindResourceW(
            hmodule.unwrap_or(core::mem::zeroed()) as _,
            lpname.param().abi(),
            lptype.param().abi(),
        )
    }
}
#[inline]
pub unsafe fn GetLastError() -> u32 {
    windows_core::link!("kernel32.dll" "system" fn GetLastError() -> u32);
    unsafe { GetLastError() }
}
#[inline]
pub unsafe fn GetModuleHandleW<P0>(lpmodulename: P0) -> HMODULE
where
    P0: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("kernel32.dll" "system" fn GetModuleHandleW(lpmodulename : windows_core::PCWSTR) -> HMODULE);
    unsafe { GetModuleHandleW(lpmodulename.param().abi()) }
}
#[inline]
pub unsafe fn LoadResource(hmodule: Option<HMODULE>, hresinfo: HRSRC) -> HGLOBAL {
    windows_core::link!("kernel32.dll" "system" fn LoadResource(hmodule : HMODULE, hresinfo : HRSRC) -> HGLOBAL);
    unsafe { LoadResource(hmodule.unwrap_or(core::mem::zeroed()) as _, hresinfo) }
}
#[inline]
pub unsafe fn LockResource(hresdata: HGLOBAL) -> *mut core::ffi::c_void {
    windows_core::link!("kernel32.dll" "system" fn LockResource(hresdata : HGLOBAL) -> *mut core::ffi::c_void);
    unsafe { LockResource(hresdata) }
}
#[inline]
pub unsafe fn MessageBoxW<P1, P2>(hwnd: Option<HWND>, lptext: P1, lpcaption: P2, utype: u32) -> i32
where
    P1: windows_core::Param<windows_core::PCWSTR>,
    P2: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("user32.dll" "system" fn MessageBoxW(hwnd : HWND, lptext : windows_core::PCWSTR, lpcaption : windows_core::PCWSTR, utype : u32) -> i32);
    unsafe {
        MessageBoxW(
            hwnd.unwrap_or(core::mem::zeroed()) as _,
            lptext.param().abi(),
            lpcaption.param().abi(),
            utype,
        )
    }
}
#[inline]
pub unsafe fn OpenProcess(dwdesiredaccess: u32, binherithandle: bool, dwprocessid: u32) -> HANDLE {
    windows_core::link!("kernel32.dll" "system" fn OpenProcess(dwdesiredaccess : u32, binherithandle : windows_core::BOOL, dwprocessid : u32) -> HANDLE);
    unsafe { OpenProcess(dwdesiredaccess, binherithandle.into(), dwprocessid) }
}
#[inline]
pub unsafe fn Process32FirstW(hsnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn Process32FirstW(hsnapshot : HANDLE, lppe : *mut PROCESSENTRY32W) -> windows_core::BOOL);
    unsafe { Process32FirstW(hsnapshot, lppe as _) }
}
#[inline]
pub unsafe fn Process32NextW(hsnapshot: HANDLE, lppe: *mut PROCESSENTRY32W) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn Process32NextW(hsnapshot : HANDLE, lppe : *mut PROCESSENTRY32W) -> windows_core::BOOL);
    unsafe { Process32NextW(hsnapshot, lppe as _) }
}
#[inline]
pub unsafe fn QueryFullProcessImageNameW(
    hprocess: HANDLE,
    dwflags: u32,
    lpexename: windows_core::PWSTR,
    lpdwsize: *mut u32,
) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn QueryFullProcessImageNameW(hprocess : HANDLE, dwflags : u32, lpexename : windows_core::PWSTR, lpdwsize : *mut u32) -> windows_core::BOOL);
    unsafe { QueryFullProcessImageNameW(hprocess, dwflags, lpexename, lpdwsize as _) }
}
#[inline]
pub unsafe fn RegCloseKey(hkey: HKEY) -> LSTATUS {
    windows_core::link!("advapi32.dll" "system" fn RegCloseKey(hkey : HKEY) -> LSTATUS);
    unsafe { RegCloseKey(hkey) }
}
#[inline]
pub unsafe fn RegCreateKeyExW<P1, P3>(
    hkey: HKEY,
    lpsubkey: P1,
    reserved: Option<u32>,
    lpclass: P3,
    dwoptions: u32,
    samdesired: REGSAM,
    lpsecurityattributes: Option<*const SECURITY_ATTRIBUTES>,
    phkresult: *mut HKEY,
    lpdwdisposition: Option<*mut u32>,
) -> LSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
    P3: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("advapi32.dll" "system" fn RegCreateKeyExW(hkey : HKEY, lpsubkey : windows_core::PCWSTR, reserved : u32, lpclass : windows_core::PCWSTR, dwoptions : u32, samdesired : REGSAM, lpsecurityattributes : *const SECURITY_ATTRIBUTES, phkresult : *mut HKEY, lpdwdisposition : *mut u32) -> LSTATUS);
    unsafe {
        RegCreateKeyExW(
            hkey,
            lpsubkey.param().abi(),
            reserved.unwrap_or(core::mem::zeroed()) as _,
            lpclass.param().abi(),
            dwoptions,
            samdesired,
            lpsecurityattributes.unwrap_or(core::mem::zeroed()) as _,
            phkresult as _,
            lpdwdisposition.unwrap_or(core::mem::zeroed()) as _,
        )
    }
}
#[inline]
pub unsafe fn RegDeleteTreeW<P1>(hkey: HKEY, lpsubkey: P1) -> LSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("advapi32.dll" "system" fn RegDeleteTreeW(hkey : HKEY, lpsubkey : windows_core::PCWSTR) -> LSTATUS);
    unsafe { RegDeleteTreeW(hkey, lpsubkey.param().abi()) }
}
#[inline]
pub unsafe fn RegSetValueExW<P1>(
    hkey: HKEY,
    lpvaluename: P1,
    reserved: Option<u32>,
    dwtype: u32,
    lpdata: Option<&[u8]>,
) -> LSTATUS
where
    P1: windows_core::Param<windows_core::PCWSTR>,
{
    windows_core::link!("advapi32.dll" "system" fn RegSetValueExW(hkey : HKEY, lpvaluename : windows_core::PCWSTR, reserved : u32, dwtype : u32, lpdata : *const u8, cbdata : u32) -> LSTATUS);
    unsafe {
        RegSetValueExW(
            hkey,
            lpvaluename.param().abi(),
            reserved.unwrap_or(core::mem::zeroed()) as _,
            dwtype,
            lpdata.map_or(core::ptr::null(), |slice| slice.as_ptr()),
            lpdata.map_or(0, |slice| slice.len().try_into().unwrap()),
        )
    }
}
#[inline]
pub unsafe fn SHChangeNotify(
    weventid: i32,
    uflags: u32,
    dwitem1: Option<*const core::ffi::c_void>,
    dwitem2: Option<*const core::ffi::c_void>,
) {
    windows_core::link!("shell32.dll" "system" fn SHChangeNotify(weventid : i32, uflags : u32, dwitem1 : *const core::ffi::c_void, dwitem2 : *const core::ffi::c_void));
    unsafe {
        SHChangeNotify(
            weventid,
            uflags,
            dwitem1.unwrap_or(core::mem::zeroed()) as _,
            dwitem2.unwrap_or(core::mem::zeroed()) as _,
        )
    }
}
#[inline]
pub unsafe fn SHGetFolderPathW(
    hwnd: Option<HWND>,
    csidl: i32,
    htoken: Option<HANDLE>,
    dwflags: u32,
    pszpath: windows_core::PWSTR,
) -> windows_core::HRESULT {
    windows_core::link!("shell32.dll" "system" fn SHGetFolderPathW(hwnd : HWND, csidl : i32, htoken : HANDLE, dwflags : u32, pszpath : windows_core::PWSTR) -> windows_core::HRESULT);
    unsafe {
        SHGetFolderPathW(
            hwnd.unwrap_or(core::mem::zeroed()) as _,
            csidl,
            htoken.unwrap_or(core::mem::zeroed()) as _,
            dwflags,
            pszpath,
        )
    }
}
#[inline]
pub unsafe fn SetCompressorInformation(
    compressorhandle: COMPRESSOR_HANDLE,
    compressinformationclass: COMPRESS_INFORMATION_CLASS,
    compressinformation: *const core::ffi::c_void,
    compressinformationsize: usize,
) -> windows_core::BOOL {
    windows_core::link!("cabinet.dll" "system" fn SetCompressorInformation(compressorhandle : COMPRESSOR_HANDLE, compressinformationclass : COMPRESS_INFORMATION_CLASS, compressinformation : *const core::ffi::c_void, compressinformationsize : usize) -> windows_core::BOOL);
    unsafe {
        SetCompressorInformation(
            compressorhandle,
            compressinformationclass,
            compressinformation,
            compressinformationsize,
        )
    }
}
#[inline]
pub unsafe fn SizeofResource(hmodule: Option<HMODULE>, hresinfo: HRSRC) -> u32 {
    windows_core::link!("kernel32.dll" "system" fn SizeofResource(hmodule : HMODULE, hresinfo : HRSRC) -> u32);
    unsafe { SizeofResource(hmodule.unwrap_or(core::mem::zeroed()) as _, hresinfo) }
}
#[inline]
pub unsafe fn TerminateProcess(hprocess: HANDLE, uexitcode: u32) -> windows_core::BOOL {
    windows_core::link!("kernel32.dll" "system" fn TerminateProcess(hprocess : HANDLE, uexitcode : u32) -> windows_core::BOOL);
    unsafe { TerminateProcess(hprocess, uexitcode) }
}
#[inline]
pub unsafe fn WaitForSingleObject(hhandle: HANDLE, dwmilliseconds: u32) -> u32 {
    windows_core::link!("kernel32.dll" "system" fn WaitForSingleObject(hhandle : HANDLE, dwmilliseconds : u32) -> u32);
    unsafe { WaitForSingleObject(hhandle, dwmilliseconds) }
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct ACCESS_MASK(pub u32);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BCRYPT_ALG_HANDLE(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct BCRYPT_HASH_HANDLE(pub *mut core::ffi::c_void);
pub const BCRYPT_SHA256_ALGORITHM: windows_core::PCWSTR = windows_core::w!("SHA256");
pub type CLSCTX = u32;
pub const CLSCTX_INPROC_SERVER: CLSCTX = 1;
pub type COINIT = i32;
pub const COINIT_APARTMENTTHREADED: COINIT = 2;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct COMPRESSOR_HANDLE(pub *mut core::ffi::c_void);
pub const COMPRESS_ALGORITHM_LZMS: i32 = 5;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct COMPRESS_ALLOCATION_ROUTINES {
    pub Allocate: PFN_COMPRESS_ALLOCATE,
    pub Free: PFN_COMPRESS_FREE,
    pub UserContext: *mut core::ffi::c_void,
}
pub type COMPRESS_INFORMATION_CLASS = i32;
pub const COMPRESS_INFORMATION_CLASS_BLOCK_SIZE: COMPRESS_INFORMATION_CLASS = 1;
pub const CSIDL_DESKTOPDIRECTORY: i32 = 16;
pub const CSIDL_PROGRAMS: i32 = 2;
pub type DECOMPRESSOR_HANDLE = COMPRESSOR_HANDLE;
pub const ERROR_INSUFFICIENT_BUFFER: i32 = 122;
pub const ERROR_MORE_DATA: i32 = 234;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FILETIME {
    pub dwLowDateTime: u32,
    pub dwHighDateTime: u32,
}
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HANDLE(pub *mut core::ffi::c_void);
pub type HGLOBAL = HANDLE;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HINSTANCE(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HKEY(pub *mut core::ffi::c_void);
pub const HKEY_CURRENT_USER: HKEY = HKEY(-2147483647 as _);
pub type HMODULE = HINSTANCE;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HRSRC(pub *mut core::ffi::c_void);
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct HWND(pub *mut core::ffi::c_void);
windows_core::imp::define_interface!(
    IPersist,
    IPersist_Vtbl,
    0x0000010c_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IPersist, windows_core::IUnknown);
#[repr(C)]
pub struct IPersist_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    GetClassID: usize,
}
impl windows_core::RuntimeName for IPersist {}
windows_core::imp::define_interface!(
    IPersistFile,
    IPersistFile_Vtbl,
    0x0000010b_0000_0000_c000_000000000046
);
impl core::ops::Deref for IPersistFile {
    type Target = IPersist;
    fn deref(&self) -> &Self::Target {
        unsafe { core::mem::transmute(self) }
    }
}
windows_core::imp::interface_hierarchy!(IPersistFile, windows_core::IUnknown, IPersist);
impl IPersistFile {
    pub unsafe fn IsDirty(&self) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).IsDirty)(windows_core::Interface::as_raw(self))
        }
    }
    pub unsafe fn Load<P0>(&self, pszfilename: P0, dwmode: u32) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).Load)(
                windows_core::Interface::as_raw(self),
                pszfilename.param().abi(),
                dwmode,
            )
        }
    }
    pub unsafe fn Save<P0>(&self, pszfilename: P0, fremember: bool) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).Save)(
                windows_core::Interface::as_raw(self),
                pszfilename.param().abi(),
                fremember.into(),
            )
        }
    }
    pub unsafe fn SaveCompleted<P0>(&self, pszfilename: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).SaveCompleted)(
                windows_core::Interface::as_raw(self),
                pszfilename.param().abi(),
            )
        }
    }
    pub unsafe fn GetCurFile(&self) -> windows_core::Result<windows_core::PWSTR> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetCurFile)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
}
#[repr(C)]
pub struct IPersistFile_Vtbl {
    pub base__: IPersist_Vtbl,
    pub IsDirty: unsafe extern "system" fn(*mut core::ffi::c_void) -> windows_core::HRESULT,
    pub Load: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        u32,
    ) -> windows_core::HRESULT,
    pub Save: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        windows_core::BOOL,
    ) -> windows_core::HRESULT,
    pub SaveCompleted: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub GetCurFile: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut windows_core::PWSTR,
    ) -> windows_core::HRESULT,
}
impl windows_core::RuntimeName for IPersistFile {}
windows_core::imp::define_interface!(
    IShellLinkW,
    IShellLinkW_Vtbl,
    0x000214f9_0000_0000_c000_000000000046
);
windows_core::imp::interface_hierarchy!(IShellLinkW, windows_core::IUnknown);
impl IShellLinkW {
    pub unsafe fn GetPath(
        &self,
        pszfile: windows_core::PWSTR,
        cch: i32,
        pfd: *mut WIN32_FIND_DATAW,
        fflags: u32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetPath)(
                windows_core::Interface::as_raw(self),
                pszfile,
                cch,
                pfd as _,
                fflags,
            )
        }
    }
    pub unsafe fn GetIDList(&self) -> windows_core::Result<LPITEMIDLIST> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetIDList)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn SetIDList(&self, pidl: *const ITEMIDLIST) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetIDList)(
                windows_core::Interface::as_raw(self),
                pidl,
            )
        }
    }
    pub unsafe fn GetDescription(
        &self,
        pszname: windows_core::PWSTR,
        cch: i32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetDescription)(
                windows_core::Interface::as_raw(self),
                pszname,
                cch,
            )
        }
    }
    pub unsafe fn SetDescription<P0>(&self, pszname: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).SetDescription)(
                windows_core::Interface::as_raw(self),
                pszname.param().abi(),
            )
        }
    }
    pub unsafe fn GetWorkingDirectory(
        &self,
        pszdir: windows_core::PWSTR,
        cch: i32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetWorkingDirectory)(
                windows_core::Interface::as_raw(self),
                pszdir,
                cch,
            )
        }
    }
    pub unsafe fn SetWorkingDirectory<P0>(&self, pszdir: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).SetWorkingDirectory)(
                windows_core::Interface::as_raw(self),
                pszdir.param().abi(),
            )
        }
    }
    pub unsafe fn GetArguments(
        &self,
        pszargs: windows_core::PWSTR,
        cch: i32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetArguments)(
                windows_core::Interface::as_raw(self),
                pszargs,
                cch,
            )
        }
    }
    pub unsafe fn SetArguments<P0>(&self, pszargs: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).SetArguments)(
                windows_core::Interface::as_raw(self),
                pszargs.param().abi(),
            )
        }
    }
    pub unsafe fn GetHotkey(&self) -> windows_core::Result<u16> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetHotkey)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn SetHotkey(&self, whotkey: u16) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetHotkey)(
                windows_core::Interface::as_raw(self),
                whotkey,
            )
        }
    }
    pub unsafe fn GetShowCmd(&self) -> windows_core::Result<i32> {
        unsafe {
            let mut result__ = core::mem::zeroed();
            (windows_core::Interface::vtable(self).GetShowCmd)(
                windows_core::Interface::as_raw(self),
                &mut result__,
            )
            .map(|| result__)
        }
    }
    pub unsafe fn SetShowCmd(&self, ishowcmd: i32) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).SetShowCmd)(
                windows_core::Interface::as_raw(self),
                ishowcmd,
            )
        }
    }
    pub unsafe fn GetIconLocation(
        &self,
        psziconpath: windows_core::PWSTR,
        cch: i32,
        piicon: *mut i32,
    ) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).GetIconLocation)(
                windows_core::Interface::as_raw(self),
                psziconpath,
                cch,
                piicon as _,
            )
        }
    }
    pub unsafe fn SetIconLocation<P0>(&self, psziconpath: P0, iicon: i32) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).SetIconLocation)(
                windows_core::Interface::as_raw(self),
                psziconpath.param().abi(),
                iicon,
            )
        }
    }
    pub unsafe fn SetRelativePath<P0>(
        &self,
        pszpathrel: P0,
        dwreserved: u32,
    ) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).SetRelativePath)(
                windows_core::Interface::as_raw(self),
                pszpathrel.param().abi(),
                dwreserved,
            )
        }
    }
    pub unsafe fn Resolve(&self, hwnd: HWND, fflags: u32) -> windows_core::HRESULT {
        unsafe {
            (windows_core::Interface::vtable(self).Resolve)(
                windows_core::Interface::as_raw(self),
                hwnd,
                fflags,
            )
        }
    }
    pub unsafe fn SetPath<P0>(&self, pszfile: P0) -> windows_core::HRESULT
    where
        P0: windows_core::Param<windows_core::PCWSTR>,
    {
        unsafe {
            (windows_core::Interface::vtable(self).SetPath)(
                windows_core::Interface::as_raw(self),
                pszfile.param().abi(),
            )
        }
    }
}
#[repr(C)]
pub struct IShellLinkW_Vtbl {
    pub base__: windows_core::IUnknown_Vtbl,
    pub GetPath: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PWSTR,
        i32,
        *mut WIN32_FIND_DATAW,
        u32,
    ) -> windows_core::HRESULT,
    pub GetIDList: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *mut LPITEMIDLIST,
    ) -> windows_core::HRESULT,
    pub SetIDList: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        *const ITEMIDLIST,
    ) -> windows_core::HRESULT,
    pub GetDescription: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PWSTR,
        i32,
    ) -> windows_core::HRESULT,
    pub SetDescription: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub GetWorkingDirectory: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PWSTR,
        i32,
    ) -> windows_core::HRESULT,
    pub SetWorkingDirectory: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub GetArguments: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PWSTR,
        i32,
    ) -> windows_core::HRESULT,
    pub SetArguments: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
    pub GetHotkey:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut u16) -> windows_core::HRESULT,
    pub SetHotkey: unsafe extern "system" fn(*mut core::ffi::c_void, u16) -> windows_core::HRESULT,
    pub GetShowCmd:
        unsafe extern "system" fn(*mut core::ffi::c_void, *mut i32) -> windows_core::HRESULT,
    pub SetShowCmd: unsafe extern "system" fn(*mut core::ffi::c_void, i32) -> windows_core::HRESULT,
    pub GetIconLocation: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PWSTR,
        i32,
        *mut i32,
    ) -> windows_core::HRESULT,
    pub SetIconLocation: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        i32,
    ) -> windows_core::HRESULT,
    pub SetRelativePath: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
        u32,
    ) -> windows_core::HRESULT,
    pub Resolve:
        unsafe extern "system" fn(*mut core::ffi::c_void, HWND, u32) -> windows_core::HRESULT,
    pub SetPath: unsafe extern "system" fn(
        *mut core::ffi::c_void,
        windows_core::PCWSTR,
    ) -> windows_core::HRESULT,
}
pub trait IShellLinkW_Impl: windows_core::IUnknownImpl {
    fn GetPath(
        &self,
        pszfile: windows_core::PWSTR,
        cch: i32,
        pfd: *mut WIN32_FIND_DATAW,
        fflags: u32,
    ) -> windows_core::Result<()>;
    fn GetIDList(&self) -> windows_core::Result<LPITEMIDLIST>;
    fn SetIDList(&self, pidl: *const ITEMIDLIST) -> windows_core::Result<()>;
    fn GetDescription(&self, pszname: windows_core::PWSTR, cch: i32) -> windows_core::Result<()>;
    fn SetDescription(&self, pszname: &windows_core::PCWSTR) -> windows_core::Result<()>;
    fn GetWorkingDirectory(
        &self,
        pszdir: windows_core::PWSTR,
        cch: i32,
    ) -> windows_core::Result<()>;
    fn SetWorkingDirectory(&self, pszdir: &windows_core::PCWSTR) -> windows_core::Result<()>;
    fn GetArguments(&self, pszargs: windows_core::PWSTR, cch: i32) -> windows_core::Result<()>;
    fn SetArguments(&self, pszargs: &windows_core::PCWSTR) -> windows_core::Result<()>;
    fn GetHotkey(&self) -> windows_core::Result<u16>;
    fn SetHotkey(&self, whotkey: u16) -> windows_core::Result<()>;
    fn GetShowCmd(&self) -> windows_core::Result<i32>;
    fn SetShowCmd(&self, ishowcmd: i32) -> windows_core::Result<()>;
    fn GetIconLocation(
        &self,
        psziconpath: windows_core::PWSTR,
        cch: i32,
        piicon: *mut i32,
    ) -> windows_core::Result<()>;
    fn SetIconLocation(
        &self,
        psziconpath: &windows_core::PCWSTR,
        iicon: i32,
    ) -> windows_core::Result<()>;
    fn SetRelativePath(
        &self,
        pszpathrel: &windows_core::PCWSTR,
        dwreserved: u32,
    ) -> windows_core::Result<()>;
    fn Resolve(&self, hwnd: HWND, fflags: u32) -> windows_core::Result<()>;
    fn SetPath(&self, pszfile: &windows_core::PCWSTR) -> windows_core::Result<()>;
}
impl IShellLinkW_Vtbl {
    pub const fn new<Identity: IShellLinkW_Impl, const OFFSET: isize>() -> Self {
        unsafe extern "system" fn GetPath<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pszfile: windows_core::PWSTR,
            cch: i32,
            pfd: *mut WIN32_FIND_DATAW,
            fflags: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::GetPath(
                    this,
                    core::mem::transmute_copy(&pszfile),
                    core::mem::transmute_copy(&cch),
                    core::mem::transmute_copy(&pfd),
                    core::mem::transmute_copy(&fflags),
                )
                .into()
            }
        }
        unsafe extern "system" fn GetIDList<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            ppidl: *mut LPITEMIDLIST,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IShellLinkW_Impl::GetIDList(this) {
                    Ok(ok__) => {
                        ppidl.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn SetIDList<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pidl: *const ITEMIDLIST,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::SetIDList(this, core::mem::transmute_copy(&pidl)).into()
            }
        }
        unsafe extern "system" fn GetDescription<
            Identity: IShellLinkW_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pszname: windows_core::PWSTR,
            cch: i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::GetDescription(
                    this,
                    core::mem::transmute_copy(&pszname),
                    core::mem::transmute_copy(&cch),
                )
                .into()
            }
        }
        unsafe extern "system" fn SetDescription<
            Identity: IShellLinkW_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pszname: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::SetDescription(this, core::mem::transmute(&pszname)).into()
            }
        }
        unsafe extern "system" fn GetWorkingDirectory<
            Identity: IShellLinkW_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pszdir: windows_core::PWSTR,
            cch: i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::GetWorkingDirectory(
                    this,
                    core::mem::transmute_copy(&pszdir),
                    core::mem::transmute_copy(&cch),
                )
                .into()
            }
        }
        unsafe extern "system" fn SetWorkingDirectory<
            Identity: IShellLinkW_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pszdir: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::SetWorkingDirectory(this, core::mem::transmute(&pszdir)).into()
            }
        }
        unsafe extern "system" fn GetArguments<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pszargs: windows_core::PWSTR,
            cch: i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::GetArguments(
                    this,
                    core::mem::transmute_copy(&pszargs),
                    core::mem::transmute_copy(&cch),
                )
                .into()
            }
        }
        unsafe extern "system" fn SetArguments<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pszargs: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::SetArguments(this, core::mem::transmute(&pszargs)).into()
            }
        }
        unsafe extern "system" fn GetHotkey<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pwhotkey: *mut u16,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IShellLinkW_Impl::GetHotkey(this) {
                    Ok(ok__) => {
                        pwhotkey.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn SetHotkey<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            whotkey: u16,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::SetHotkey(this, core::mem::transmute_copy(&whotkey)).into()
            }
        }
        unsafe extern "system" fn GetShowCmd<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pishowcmd: *mut i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                match IShellLinkW_Impl::GetShowCmd(this) {
                    Ok(ok__) => {
                        pishowcmd.write(ok__);
                        windows_core::HRESULT(0)
                    }
                    Err(err) => err.into(),
                }
            }
        }
        unsafe extern "system" fn SetShowCmd<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            ishowcmd: i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::SetShowCmd(this, core::mem::transmute_copy(&ishowcmd)).into()
            }
        }
        unsafe extern "system" fn GetIconLocation<
            Identity: IShellLinkW_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            psziconpath: windows_core::PWSTR,
            cch: i32,
            piicon: *mut i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::GetIconLocation(
                    this,
                    core::mem::transmute_copy(&psziconpath),
                    core::mem::transmute_copy(&cch),
                    core::mem::transmute_copy(&piicon),
                )
                .into()
            }
        }
        unsafe extern "system" fn SetIconLocation<
            Identity: IShellLinkW_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            psziconpath: windows_core::PCWSTR,
            iicon: i32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::SetIconLocation(
                    this,
                    core::mem::transmute(&psziconpath),
                    core::mem::transmute_copy(&iicon),
                )
                .into()
            }
        }
        unsafe extern "system" fn SetRelativePath<
            Identity: IShellLinkW_Impl,
            const OFFSET: isize,
        >(
            this: *mut core::ffi::c_void,
            pszpathrel: windows_core::PCWSTR,
            dwreserved: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::SetRelativePath(
                    this,
                    core::mem::transmute(&pszpathrel),
                    core::mem::transmute_copy(&dwreserved),
                )
                .into()
            }
        }
        unsafe extern "system" fn Resolve<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            hwnd: HWND,
            fflags: u32,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::Resolve(
                    this,
                    core::mem::transmute_copy(&hwnd),
                    core::mem::transmute_copy(&fflags),
                )
                .into()
            }
        }
        unsafe extern "system" fn SetPath<Identity: IShellLinkW_Impl, const OFFSET: isize>(
            this: *mut core::ffi::c_void,
            pszfile: windows_core::PCWSTR,
        ) -> windows_core::HRESULT {
            unsafe {
                let this: &Identity =
                    &*((this as *const *const ()).offset(OFFSET) as *const Identity);
                IShellLinkW_Impl::SetPath(this, core::mem::transmute(&pszfile)).into()
            }
        }
        Self {
            base__: windows_core::IUnknown_Vtbl::new::<Identity, OFFSET>(),
            GetPath: GetPath::<Identity, OFFSET>,
            GetIDList: GetIDList::<Identity, OFFSET>,
            SetIDList: SetIDList::<Identity, OFFSET>,
            GetDescription: GetDescription::<Identity, OFFSET>,
            SetDescription: SetDescription::<Identity, OFFSET>,
            GetWorkingDirectory: GetWorkingDirectory::<Identity, OFFSET>,
            SetWorkingDirectory: SetWorkingDirectory::<Identity, OFFSET>,
            GetArguments: GetArguments::<Identity, OFFSET>,
            SetArguments: SetArguments::<Identity, OFFSET>,
            GetHotkey: GetHotkey::<Identity, OFFSET>,
            SetHotkey: SetHotkey::<Identity, OFFSET>,
            GetShowCmd: GetShowCmd::<Identity, OFFSET>,
            SetShowCmd: SetShowCmd::<Identity, OFFSET>,
            GetIconLocation: GetIconLocation::<Identity, OFFSET>,
            SetIconLocation: SetIconLocation::<Identity, OFFSET>,
            SetRelativePath: SetRelativePath::<Identity, OFFSET>,
            Resolve: Resolve::<Identity, OFFSET>,
            SetPath: SetPath::<Identity, OFFSET>,
        }
    }
    pub fn matches(iid: &windows_core::GUID) -> bool {
        iid == &<IShellLinkW as windows_core::Interface>::IID
    }
}
impl windows_core::RuntimeName for IShellLinkW {}
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct ITEMIDLIST {
    pub mkid: SHITEMID,
}
pub const KEY_WRITE: i32 = 131078;
pub type LPITEMIDLIST = *mut ITEMIDLIST;
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct LSTATUS(pub i32);
pub const MB_ICONERROR: i32 = 16;
pub const MB_ICONINFORMATION: i32 = 64;
pub const MB_OK: i32 = 0;
pub type PFN_COMPRESS_ALLOCATE = Option<
    unsafe extern "C" fn(
        usercontext: *const core::ffi::c_void,
        size: usize,
    ) -> *mut core::ffi::c_void,
>;
pub type PFN_COMPRESS_FREE = Option<
    unsafe extern "C" fn(usercontext: *const core::ffi::c_void, memory: *const core::ffi::c_void),
>;
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PROCESSENTRY32W {
    pub dwSize: u32,
    pub cntUsage: u32,
    pub th32ProcessID: u32,
    pub th32DefaultHeapID: usize,
    pub th32ModuleID: u32,
    pub cntThreads: u32,
    pub th32ParentProcessID: u32,
    pub pcPriClassBase: i32,
    pub dwFlags: u32,
    pub szExeFile: [u16; 260],
}
impl Default for PROCESSENTRY32W {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
pub const PROCESS_QUERY_LIMITED_INFORMATION: i32 = 4096;
pub const PROCESS_TERMINATE: i32 = 1;
pub type REGSAM = ACCESS_MASK;
pub const REG_OPTION_NON_VOLATILE: i32 = 0;
pub const REG_SZ: u32 = 1;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SECURITY_ATTRIBUTES {
    pub nLength: u32,
    pub lpSecurityDescriptor: *mut core::ffi::c_void,
    pub bInheritHandle: windows_core::BOOL,
}
pub const SHCNE_ASSOCCHANGED: i32 = 134217728;
pub const SHCNF_IDLIST: i32 = 0;
#[repr(C, packed(1))]
#[derive(Clone, Copy)]
pub struct SHITEMID {
    pub cb: u16,
    pub abID: [u8; 1],
}
impl Default for SHITEMID {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
pub const SYNCHRONIZE: i32 = 1048576;
pub const ShellLink: windows_core::GUID =
    windows_core::GUID::from_u128(0x00021401_0000_0000_c000_000000000046);
pub const TH32CS_SNAPPROCESS: i32 = 2;
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WIN32_FIND_DATAW {
    pub dwFileAttributes: u32,
    pub ftCreationTime: FILETIME,
    pub ftLastAccessTime: FILETIME,
    pub ftLastWriteTime: FILETIME,
    pub nFileSizeHigh: u32,
    pub nFileSizeLow: u32,
    pub dwReserved0: u32,
    pub dwReserved1: u32,
    pub cFileName: [u16; 260],
    pub cAlternateFileName: [u16; 14],
}
impl Default for WIN32_FIND_DATAW {
    fn default() -> Self {
        unsafe { core::mem::zeroed() }
    }
}
